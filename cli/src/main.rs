//! `openepl` — the Phase 0 command-line driver.
//!
//! Subcommands:
//!   openepl build <in.oir> [-o <out>]   parse -> lower -> clang -> native binary
//!   openepl run   <in.oir> [-o <out>]   build, then execute it
//!   openepl emit  <in.oir>              print the generated LLVM IR to stdout
//!
//! The pipeline is the BlackMoon model with `clang` standing in for the raw
//! obj-emit + system-linker steps (PRD §5.2): IR -> `.ll` -> `clang` assembles
//! and links the runtime objects -> a standard native executable.

use std::path::{Path, PathBuf};
use std::process::{exit, Command};

mod libload;

use openepl_backend::lower_module;
use openepl_ir::{parse, validate};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = run(&args[1..]);
    exit(code);
}

fn run(args: &[String]) -> i32 {
    let (cmd, rest) = match args.split_first() {
        Some(x) => x,
        None => {
            usage();
            return 2;
        }
    };
    match cmd.as_str() {
        "build" => cmd_build(rest, false),
        "run" => cmd_build(rest, true),
        "emit" => cmd_emit(rest),
        "-h" | "--help" | "help" => {
            usage();
            0
        }
        other => {
            eprintln!("openepl: unknown subcommand `{other}`\n");
            usage();
            2
        }
    }
}

fn usage() {
    eprintln!(
        "openepl (Phase 0)\n\n\
         USAGE:\n  \
         openepl build <in.oir> [-o <out>]   compile to a native binary\n  \
         openepl run   <in.oir> [-o <out>]   compile and run\n  \
         openepl emit  <in.oir>              print generated LLVM IR\n"
    );
}

/// Parse `<in.oir> [-o out]` from an argument slice.
fn parse_io(rest: &[String]) -> Result<(PathBuf, Option<PathBuf>), String> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                let v = rest.get(i).ok_or("`-o` needs a path")?;
                output = Some(PathBuf::from(v));
            }
            s if s.starts_with('-') => return Err(format!("unknown flag `{s}`")),
            s => {
                if input.is_some() {
                    return Err("multiple input files given".into());
                }
                input = Some(PathBuf::from(s));
            }
        }
        i += 1;
    }
    let input = input.ok_or("no input .oir file given")?;
    Ok((input, output))
}

fn cmd_emit(rest: &[String]) -> i32 {
    let (input, _) = match parse_io(rest) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("openepl: {e}");
            return 2;
        }
    };
    match compile(&input) {
        Ok((ll, _)) => {
            print!("{ll}");
            0
        }
        Err(e) => {
            eprintln!("openepl: {e}");
            1
        }
    }
}

fn cmd_build(rest: &[String], then_run: bool) -> i32 {
    let (input, output) = match parse_io(rest) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("openepl: {e}");
            return 2;
        }
    };
    let out_bin = output.unwrap_or_else(|| default_output(&input));

    let (ll, impl_sources) = match compile(&input) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("openepl: {e}");
            return 1;
        }
    };

    let ll_path = out_bin.with_extension("ll");
    if let Err(e) = std::fs::write(&ll_path, &ll) {
        eprintln!("openepl: cannot write {}: {e}", ll_path.display());
        return 1;
    }

    let repo_root = find_repo_root().expect("runtime located during compile()");
    if let Err(code) = clang_link(&ll_path, &repo_root, &impl_sources, &out_bin) {
        return code;
    }
    eprintln!("openepl: wrote {}", out_bin.display());

    if then_run {
        let status = Command::new(absolutize(&out_bin)).status();
        match status {
            Ok(s) => s.code().unwrap_or(1),
            Err(e) => {
                eprintln!("openepl: failed to run {}: {e}", out_bin.display());
                1
            }
        }
    } else {
        0
    }
}

/// Parse, introspect libraries, validate, and lower to LLVM IR.
/// Returns the `.ll` text and the implementation sources to static-link.
fn compile(input: &Path) -> Result<(String, Vec<PathBuf>), String> {
    let src = std::fs::read_to_string(input)
        .map_err(|e| format!("cannot read {}: {e}", input.display()))?;
    let module = parse(&src).map_err(|e| e.to_string())?;

    let repo_root = find_repo_root().ok_or_else(|| {
        "could not locate the OpenEPL runtime (runtime/openepl_core.h); \
         set OPENEPL_RUNTIME_DIR or run from the repo root"
            .to_string()
    })?;

    // Introspect `core` + each `use`d library for command signatures (the
    // authoritative source — no hard-coded table).
    let plan = libload::load(&repo_root, &module.uses)?;

    if let Err(errs) = validate(&module, &plan.registry) {
        let joined = errs
            .iter()
            .map(|e| format!("  - {e}"))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!(
            "{} error(s) in {}:\n{joined}",
            errs.len(),
            input.display()
        ));
    }
    let ll = lower_module(&module, &plan.registry).map_err(|e| e.to_string())?;
    Ok((ll, plan.impl_sources))
}

fn default_output(input: &Path) -> PathBuf {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("a");
    PathBuf::from(stem)
}

/// Invoke clang to assemble the `.ll` and static-link the library implementation
/// sources into a native executable, dead-stripping unused commands (PRD D3).
fn clang_link(
    ll_path: &Path,
    repo_root: &Path,
    impl_sources: &[PathBuf],
    out_bin: &Path,
) -> Result<(), i32> {
    let mut cmd = Command::new("clang");
    cmd.arg("-O0")
        .arg("-ffunction-sections")
        .arg("-fdata-sections")
        .arg("-Wl,--gc-sections")
        .arg("-Wno-override-module")
        .arg("-I")
        .arg(repo_root.join("abi"))
        .arg("-I")
        .arg(repo_root.join("runtime"))
        .arg(ll_path);
    for s in impl_sources {
        cmd.arg(s);
    }
    cmd.arg("-lm"); // libm for the floating-point commands
    cmd.arg("-o").arg(out_bin);

    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => {
            eprintln!("openepl: clang failed with status {s}");
            Err(1)
        }
        Err(e) => {
            eprintln!("openepl: could not invoke clang: {e}");
            Err(1)
        }
    }
}

/// Find the runtime directory: `$OPENEPL_RUNTIME_DIR`, else walk up from cwd
/// looking for `runtime/openepl_core.h`.
fn find_runtime_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("OPENEPL_RUNTIME_DIR") {
        let p = PathBuf::from(d);
        if p.join("openepl_core.h").is_file() {
            return Some(p);
        }
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let cand = dir.join("runtime");
        if cand.join("openepl_core.h").is_file() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// The repository root (parent of the located `runtime/` directory).
fn find_repo_root() -> Option<PathBuf> {
    find_runtime_dir().and_then(|d| d.parent().map(|p| p.to_path_buf()))
}

/// Make a bare relative path executable-invokable (`./name`).
fn absolutize(p: &Path) -> PathBuf {
    if p.components().count() == 1 {
        let mut pb = PathBuf::from(".");
        pb.push(p);
        pb
    } else {
        p.to_path_buf()
    }
}
