//! `openepl` — the command-line toolchain.
//!
//! Subcommands:
//!   openepl build <in.oir> [-o <out>]   parse -> lower -> clang -> native binary
//!   openepl run   <in.oir> [-o <out>]   build, then execute it
//!     …either with --release            optimised, hardened and stripped
//!   openepl emit  <in.oir>              print the generated LLVM IR to stdout
//!   openepl lsp                         language server (stdio) for editors
//!   openepl commands                    list available commands and components
//!   openepl templates                   list project templates
//!   openepl new <tmpl> <dir>            create a project from a template
//!   openepl kits                        list resolved kits and where they came from
//!   openepl kit add <path>              install a kit into ~/.openepl/kits
//!
//! The pipeline lowers a module to LLVM IR, then has `clang` assemble it and
//! link the runtime sources, producing an ordinary native executable.

use std::path::{Path, PathBuf};
use std::process::{exit, Command};

mod kit;
mod libload;
mod lsp;
mod lsp_index;
mod templates;

use openepl_backend::lower_module;
use openepl_ir::{parse, validate, Target};

fn main() {
    // Die quietly when a reader goes away, the way every other command-line
    // tool does: `openepl commands | head` should not print a panic.
    #[cfg(unix)]
    unsafe {
        libc_signal_default();
    }
    let args: Vec<String> = std::env::args().collect();
    let code = run(&args[1..]);
    exit(code);
}

/// Restore the default SIGPIPE disposition, which Rust's runtime overrides.
#[cfg(unix)]
unsafe fn libc_signal_default() {
    // SIG_DFL for SIGPIPE (13). Declared here rather than adding a dependency
    // on `libc` for one constant.
    unsafe extern "C" {
        fn signal(sig: i32, handler: usize) -> usize;
    }
    unsafe { signal(13, 0) };
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
        "inspect" => cmd_inspect(rest),
        "lsp" => lsp::run(),
        "commands" => match find_repo_root() {
            Some(root) => cmd_commands(&root, rest),
            None => {
                eprintln!("openepl: could not locate the OpenEPL runtime");
                1
            }
        },
        "kits" => match find_repo_root() {
            Some(root) => kit::cmd_list(&root),
            None => {
                eprintln!("openepl: could not locate the OpenEPL libraries");
                1
            }
        },
        "kit" => match rest.split_first() {
            Some((verb, kit_args)) if verb == "add" => kit::cmd_add(kit_args),
            Some((verb, _)) => {
                eprintln!("openepl: unknown `kit` verb `{verb}` — expected `add`");
                2
            }
            None => {
                eprintln!("openepl: usage: openepl kit add <path-or-tarball>");
                2
            }
        },
        "templates" => match find_repo_root() {
            Some(root) => templates::cmd_list(&root),
            None => {
                eprintln!("openepl: could not locate the OpenEPL templates directory");
                1
            }
        },
        "new" => match find_repo_root() {
            Some(root) => templates::cmd_new(&root, rest),
            None => {
                eprintln!("openepl: could not locate the OpenEPL templates directory");
                1
            }
        },
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
        "openepl — the OpenEPL toolchain\n\n\
         USAGE:\n  \
         openepl build <in.oir> [-o <out>]   compile to a native binary\n  \
         openepl run   <in.oir> [-o <out>]   compile and run\n  \
         openepl build|run --release         …optimised, hardened and stripped\n  \
         openepl emit  <in.oir>              print generated LLVM IR\n  \
         openepl inspect <in.oir>            dump the form model (for the designer)\n  \
         openepl lsp                         language server over stdio (see docs/editors.md)\n  \
         openepl commands [--use <lib>]      list the commands and components available\n  \
         openepl templates                   list the available project templates\n  \
         openepl new <template> <dir>        create a project from a template\n  \
         openepl kits                        list the kits found, and from where\n  \
         openepl kit add <path>              install a kit into ~/.openepl/kits\n"
    );
}

/// What a build/emit invocation was asked to do.
struct Io {
    input: PathBuf,
    output: Option<PathBuf>,
    /// Overrides the module's own `target` declaration when given.
    target: Option<Target>,
    /// Optimise, harden and strip the built program.
    release: bool,
}

/// Parse `<in.oir> [-o out] [--target kind] [--release]` from an argument slice.
fn parse_io(rest: &[String]) -> Result<Io, String> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut target: Option<Target> = None;
    let mut release = false;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                let v = rest.get(i).ok_or("`-o` needs a path")?;
                output = Some(PathBuf::from(v));
            }
            "--target" | "-t" => {
                i += 1;
                let v = rest.get(i).ok_or("`--target` needs a kind")?;
                target = Some(Target::parse(v).ok_or_else(|| {
                    format!("unknown target `{v}` — expected console, gui, sharedlib or staticlib")
                })?);
            }
            "--release" => release = true,
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
    Ok(Io {
        input,
        output,
        target,
        release,
    })
}

fn cmd_emit(rest: &[String]) -> i32 {
    let io = match parse_io(rest) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("openepl: {e}");
            return 2;
        }
    };
    match compile_with(&io.input, io.target, false) {
        Ok((ll, _plan, _t)) => {
            print!("{ll}");
            0
        }
        Err(e) => {
            eprintln!("openepl: {e}");
            1
        }
    }
}

/// List everything a program can call or place: commands from the core runtime
/// plus any `use`d libraries, and the visual components they contribute.
///
/// Line-based like `inspect` and `templates`, so it can be read by a script as
/// easily as by a person — the documentation's reference pages are generated
/// from this rather than written by hand, which is the only way they stay true.
fn cmd_commands(repo_root: &Path, args: &[String]) -> i32 {
    let mut uses: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--use" | "-u" => {
                i += 1;
                match args.get(i) {
                    Some(v) => uses.push(v.clone()),
                    None => {
                        eprintln!("openepl: `--use` needs a library name");
                        return 2;
                    }
                }
            }
            s => {
                eprintln!("openepl: unexpected argument `{s}`");
                return 2;
            }
        }
        i += 1;
    }

    // Metadata only: listing what exists must not require the ability to link
    // it, or `openepl commands --use ui` would fail on any machine that has not
    // vendored the UI stack.
    let root = match kit::overlay_root(repo_root, &uses) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("openepl: {e}");
            return 1;
        }
    };
    let plan = match libload::load_metadata(&root, &uses) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("openepl: {e}");
            return 1;
        }
    };

    let mut names: Vec<&str> = plan.registry.names().collect();
    names.sort_unstable();
    for name in names {
        let Some(cmd) = plan.registry.get(name) else { continue };
        let params = cmd
            .sig
            .params
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        match cmd.sig.ret {
            Some(r) => println!("command: {name}({params}) -> {}", r.as_str()),
            None => println!("command: {name}({params})"),
        }
    }

    let mut components: Vec<&str> = plan.registry.component_names().collect();
    components.sort_unstable();
    for type_name in components {
        let Some(desc) = plan.registry.component(type_name) else { continue };
        println!("component: {type_name}");
        for p in &desc.properties {
            println!("property: {type_name} {} {}", p.name, p.ty.as_str());
        }
        for e in &desc.events {
            println!("event: {type_name} {e}");
        }
    }
    0
}

/// Dump a module's form model as plain lines, for the designer to read.
///
/// This is the designer's ONLY way to learn a file's contents: the Rust parser
/// stays the single reader of `.oir`. If the designer ever parsed the text
/// itself there would be two grammars to keep in step, and they would drift
///.
///
/// Line-based rather than JSON so neither side needs a serialisation library.
fn cmd_inspect(rest: &[String]) -> i32 {
    let io = match parse_io(rest) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("openepl: {e}");
            return 2;
        }
    };
    let input = io.input;
    let src = match std::fs::read_to_string(&input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("openepl: cannot read {}: {e}", input.display());
            return 1;
        }
    };
    let module = match openepl_ir::parse(&src) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("openepl: {e}");
            return 1;
        }
    };

    println!("module: {}", module.name);
    for u in &module.uses {
        println!("use: {u}");
    }
    for sub in module.subs() {
        // `sub:` stays the bare name — the designer reads the rest of the line
        // as a handler name and binds `on click: <that>`. Parameters and a
        // return type go on their own line, and only when there are any, so a
        // reader that predates them sees exactly what it saw before.
        println!("sub: {}", sub.name);
        if !sub.is_plain() {
            let params: Vec<String> = sub
                .params
                .iter()
                .map(|(n, t)| format!("{n}:{}", t.as_str()))
                .collect();
            println!(
                "subsig: {} ({}) {}",
                sub.name,
                params.join(", "),
                sub.ret.map_or("-", |t| t.as_str())
            );
        }
    }
    for form in module.forms() {
        println!(
            "form: {} span={}..{}",
            form.name, form.line_span.0, form.line_span.1
        );
        for (name, value) in &form.properties {
            println!("prop: {} {} {}", form.name, name, literal_text(value));
        }
        for (event, handler) in &form.handlers {
            println!("handler: {} {} {}", form.name, event, handler);
        }
        for c in &form.children {
            println!("component: {} {}", c.id, c.type_name);
            for (name, value) in &c.properties {
                println!("prop: {} {} {}", c.id, name, literal_text(value));
            }
            for (event, handler) in &c.handlers {
                println!("handler: {} {} {}", c.id, event, handler);
            }
        }
    }
    0
}

/// Render a property literal as the designer should display and re-emit it.
fn literal_text(e: &openepl_ir::Expr) -> String {
    use openepl_ir::Expr;
    match e {
        Expr::TextLit(s) => s.clone(),
        Expr::IntLit(v) => v.to_string(),
        Expr::DoubleLit(v) => v.to_string(),
        Expr::BoolLit(b) => b.to_string(),
        _ => String::new(),
    }
}

fn cmd_build(rest: &[String], then_run: bool) -> i32 {
    let io = match parse_io(rest) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("openepl: {e}");
            return 2;
        }
    };
    let input = io.input;
    let (ll, plan, target) = match compile(&input, io.target) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("openepl: {e}");
            return 1;
        }
    };
    let out_bin = io.output.unwrap_or_else(|| default_output(&input, target));

    let ll_path = out_bin.with_extension("ll");
    if let Err(e) = std::fs::write(&ll_path, &ll) {
        eprintln!("openepl: cannot write {}: {e}", ll_path.display());
        return 1;
    }

    let repo_root = find_repo_root().expect("runtime located during compile()");
    if let Err(code) = clang_link(&ll_path, &repo_root, &plan, &out_bin, target, io.release) {
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
fn compile(input: &Path, target_override: Option<Target>) -> Result<(String, libload::LibPlan, Target), String> {
    compile_with(input, target_override, true)
}

/// `require_impl` is false when the caller only wants the IR: emitting it
/// exercises parsing, validation and lowering, none of which need a library to
/// be linkable. Demanding a vendored UI stack to *print* IR would make the
/// check unavailable exactly where it is most useful — a fresh checkout.
fn compile_with(
    input: &Path,
    target_override: Option<Target>,
    require_impl: bool,
) -> Result<(String, libload::LibPlan, Target), String> {
    let src = std::fs::read_to_string(input)
        .map_err(|e| format!("cannot read {}: {e}", input.display()))?;
    let mut module = parse(&src).map_err(|e| e.to_string())?;
    // An explicit --target wins over the module's declaration: the same source
    // should be buildable as a program or a library without editing it.
    if let Some(t) = target_override {
        module.target = Some(t);
    }
    let target = module.target();

    let repo_root = find_repo_root().ok_or_else(|| {
        "could not locate the OpenEPL runtime (runtime/openepl_core.h); \
         set OPENEPL_RUNTIME_DIR or run from the repo root"
            .to_string()
    })?;

    // Introspect `core` + each `use`d library for command signatures (the
    // authoritative source — no hard-coded table).
    // A kit resolved outside `libs/` is presented to the loader through a
    // staged root, so listing a command and calling it agree by construction.
    let lib_root = kit::overlay_root(&repo_root, &module.uses)?;
    let plan = if require_impl {
        libload::load(&lib_root, &module.uses)?
    } else {
        libload::load_metadata(&lib_root, &module.uses)?
    };

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
    Ok((ll, plan, target))
}

fn default_output(input: &Path, target: Target) -> PathBuf {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("a");
    // Libraries follow the platform convention, so a host's linker finds them
    // by the name it expects (`-lgreet` wants `libgreet.so`).
    match target {
        Target::Console | Target::Gui => PathBuf::from(stem),
        Target::SharedLib => PathBuf::from(format!("lib{stem}.so")),
        Target::StaticLib => PathBuf::from(format!("lib{stem}.a")),
    }
}

/// Invoke clang to assemble the `.ll` and static-link the library implementation
/// sources into a native executable, dead-stripping unused commands.
fn clang_link(
    ll_path: &Path,
    repo_root: &Path,
    plan: &libload::LibPlan,
    out_bin: &Path,
    target: Target,
    release: bool,
) -> Result<(), i32> {
    let cfg = &plan.build;
    let driver = if cfg.needs_cxx { "clang++" } else { "clang" };

    // Flags every invocation needs, whether we are linking a program or
    // compiling one object at a time for an archive.
    let mut common: Vec<String> = vec![
        "-ffunction-sections".into(),
        "-fdata-sections".into(),
        "-Wno-override-module".into(),
        "-I".into(),
        repo_root.join("abi").display().to_string(),
        "-I".into(),
        repo_root.join("runtime").display().to_string(),
    ];
    // A debug build is the default and stays exactly as it was: fast to
    // produce, and the shape a developer iterates on.
    if release {
        common.splice(0..0, release_cflags(driver, target.is_executable()));
    } else {
        common.insert(0, "-O0".into());
    }
    for d in &cfg.include_dirs {
        common.push("-I".into());
        common.push(d.display().to_string());
    }
    for d in &cfg.defines {
        common.push(format!("-D{d}"));
    }
    match libload::pkg_config_flags(&cfg.pkg_config, "--cflags") {
        Ok(flags) => common.extend(flags),
        Err(e) => {
            eprintln!("openepl: {e}");
            return Err(1);
        }
    }
    // Both library kinds must be position independent: a shared object requires
    // it, and a static archive is routinely linked into one.
    if !target.is_executable() {
        common.push("-fPIC".into());
    }

    // The inputs, each with the language it must be compiled as. When any
    // library needs C++ the driver is clang++, which would otherwise compile
    // our .c files as C++ and mangle their symbols, breaking the C ABI the
    // emitted IR calls.
    let mut inputs: Vec<(PathBuf, Option<&'static str>)> = vec![(ll_path.to_path_buf(), None)];
    for s in &plan.impl_sources {
        // The process-entry object provides `main`, which calls `ECodeStart`.
        // A library has no `ECodeStart`, so linking it in leaves an undefined
        // symbol and the `.so` fails to dlopen — a file with the right
        // extension that cannot actually be loaded. oe_start.c lives in its own
        // TU precisely so a build target can drop it.
        if !target.is_executable() && s.file_name().and_then(|f| f.to_str()) == Some("oe_start.c") {
            continue;
        }
        let is_cxx = matches!(
            s.extension().and_then(|e| e.to_str()),
            Some("cpp") | Some("cc") | Some("cxx")
        );
        let lang = if cfg.needs_cxx {
            Some(if is_cxx { "c++" } else { "c" })
        } else {
            None
        };
        inputs.push((s.clone(), lang));
    }

    if target == Target::StaticLib {
        return build_archive(driver, &common, &inputs, out_bin);
    }

    // Everything after the inputs, in the order the link has always used it.
    let mut libs: Vec<String> = cfg.link_args.clone();
    match libload::pkg_config_flags(&cfg.pkg_config, "--libs") {
        Ok(flags) => libs.extend(flags),
        Err(e) => {
            eprintln!("openepl: {e}");
            return Err(1);
        }
    }
    libs.push("-lm".into()); // libm for the floating-point commands
    if target == Target::SharedLib {
        libs.push("-shared".into());
    } else {
        // Dead-strip: the headline property of the BlackMoon model.
        // Only for programs — a library must keep exports no host has linked
        // yet, and --gc-sections would drop every one of them.
        libs.push("-Wl,--gc-sections".into());
    }

    let link = |common: &[String], ldflags: &[String], quiet: bool| -> Result<bool, i32> {
        let mut cmd = Command::new(driver);
        cmd.args(common);
        for (path, lang) in &inputs {
            if let Some(l) = lang {
                cmd.arg("-x").arg(l);
            }
            cmd.arg(path);
        }
        cmd.args(&libs);
        cmd.args(ldflags);
        cmd.arg("-o").arg(out_bin);
        if quiet {
            // Held back rather than dropped: a first attempt that succeeds
            // still has its warnings to say.
            match cmd.output() {
                Ok(o) => {
                    if o.status.success() {
                        eprint!("{}", String::from_utf8_lossy(&o.stderr));
                    }
                    Ok(o.status.success())
                }
                Err(e) => {
                    eprintln!("openepl: could not invoke {driver}: {e}");
                    Err(1)
                }
            }
        } else {
            match cmd.status() {
                Ok(s) if s.success() => Ok(true),
                Ok(s) => {
                    eprintln!("openepl: clang failed with status {s}");
                    Ok(false)
                }
                Err(e) => {
                    eprintln!("openepl: could not invoke clang: {e}");
                    Err(1)
                }
            }
        }
    };

    let ldflags = if release {
        release_ldflags(driver, &common, target.is_executable())
    } else {
        Vec::new()
    };

    // A probe proves the driver accepts -pie; only the real link proves the
    // objects allow it. A vendored static library built without -fPIC — the UI
    // stack is one — cannot go into a position-independent program, and that
    // is a fact about the dependency, not a reason to fail the build.
    let pie = ldflags.iter().any(|f| f.ends_with("pie"));
    if pie {
        if link(&common, &ldflags, true)? {
            return Ok(());
        }
        eprintln!(
            "openepl: this program links a library that is not position-independent; \
             building the release without PIE"
        );
        let common: Vec<String> = common.iter().filter(|f| *f != "-fPIE").cloned().collect();
        let ldflags: Vec<String> = ldflags
            .iter()
            .filter(|f| !f.ends_with("pie"))
            .cloned()
            .collect();
        return if link(&common, &ldflags, false)? {
            Ok(())
        } else {
            Err(1)
        };
    }

    if link(&common, &ldflags, false)? {
        Ok(())
    } else {
        Err(1)
    }
}

/// One hardening requirement, as the argument lists that would satisfy it,
/// best first. `-pie` and `-Wl,-pie` ask the same thing of the driver and of
/// the linker, and which of them works is a property of the local install:
/// with GNU ld, `-Wl,-pie` links the non-PIE start files and fails.
type Requirement = Vec<Vec<String>>;

fn req(alternatives: &[&[&str]]) -> Requirement {
    alternatives
        .iter()
        .map(|alt| alt.iter().map(|s| s.to_string()).collect())
        .collect()
}

/// The compile-time half of the release profile, in the order it must be
/// probed: `_FORTIFY_SOURCE` is a no-op that warns unless optimisation is
/// already on, so `-O2` has to be accepted before it is offered.
fn release_cflags(driver: &str, executable: bool) -> Vec<String> {
    let mut want = vec![
        req(&[&["-O2"]]),
        // The distribution may have fortified the compiler already, and
        // redefining the macro is a warning — which the probe reads as a no.
        req(&[&["-U_FORTIFY_SOURCE", "-D_FORTIFY_SOURCE=2"]]),
        req(&[&["-fstack-protector-strong"]]),
    ];
    // Position independence for a program only: libraries are compiled -fPIC
    // already, and -fPIE would contradict it.
    if executable {
        want.push(req(&[&["-fPIE"]]));
    }
    probe(driver, &[], &want, &[])
}

/// The link-time half, probed on top of the compile flags that were accepted —
/// `-pie` is only meaningful over objects compiled `-fPIE`.
///
/// `-Wl,-s` is the strip: done in the link it needs no second tool and cannot
/// leave a half-stripped file behind when it fails.
fn release_ldflags(driver: &str, cflags: &[String], executable: bool) -> Vec<String> {
    let mut want: Vec<Requirement> = Vec::new();
    if executable {
        want.push(req(&[&["-pie"], &["-Wl,-pie"]]));
    }
    want.push(req(&[&["-Wl,-z,relro"]]));
    want.push(req(&[&["-Wl,-z,now"]]));
    want.push(req(&[&["-Wl,-s"]]));
    // A linker answers an option it does not know with a warning and carries
    // on, which would leave us believing in hardening that is not there.
    probe(driver, cflags, &want, &["-Wl,--fatal-warnings".to_string()])
}

/// Ask the local toolchain which of `want` it actually accepts, by building a
/// trivial program with each requirement in turn on top of the ones already
/// accepted.
///
/// A flag this compiler rejects is dropped and said out loud. Passing it
/// regardless would be worse than leaving it out: the build still succeeds and
/// the binary is not hardened, which is the failure nobody notices.
fn probe(driver: &str, base: &[String], want: &[Requirement], extra: &[String]) -> Vec<String> {
    let dir = std::env::temp_dir().join(format!("openepl_probe_{}", std::process::id()));
    // The extension picks the language: clang++ handed a .c file treats it as
    // C++ and says so as a deprecation warning, which -Werror turns into a
    // rejection of every flag we ask about.
    let src = dir.join(if driver.ends_with("++") {
        "probe.cpp"
    } else {
        "probe.c"
    });
    if std::fs::create_dir_all(&dir).is_err()
        || std::fs::write(&src, "int main(void){return 0;}\n").is_err()
    {
        eprintln!("openepl: cannot write a probe program — building the release unhardened");
        return Vec::new();
    }
    let out = dir.join("probe");

    let mut taken: Vec<String> = Vec::new();
    for alternatives in want {
        let accepted = alternatives.iter().find(|alt| {
            Command::new(driver)
                .args(base)
                .args(&taken)
                .args(*alt)
                .args(extra)
                .arg("-Werror")
                .arg(&src)
                .arg("-o")
                .arg(&out)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        });
        match accepted {
            Some(alt) => taken.extend(alt.iter().cloned()),
            None => eprintln!(
                "openepl: {driver} does not accept {} — building the release without it",
                alternatives[0].join(" ")
            ),
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    taken
}

/// Compile each input to its own object and archive them.
///
/// `clang -c` refuses a single `-o` for several inputs, so an archive has to be
/// built one object at a time rather than in one command like a link.
fn build_archive(
    driver: &str,
    common: &[String],
    inputs: &[(PathBuf, Option<&'static str>)],
    out_lib: &Path,
) -> Result<(), i32> {
    let dir = std::env::temp_dir().join(format!("openepl_ar_{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("openepl: cannot create {}: {e}", dir.display());
        return Err(1);
    }

    let mut objects: Vec<PathBuf> = Vec::new();
    for (i, (path, lang)) in inputs.iter().enumerate() {
        let obj = dir.join(format!("{i}.o"));
        let mut cmd = Command::new(driver);
        cmd.args(common).arg("-c");
        if let Some(l) = lang {
            cmd.arg("-x").arg(l);
        }
        cmd.arg(path).arg("-o").arg(&obj);
        match cmd.status() {
            Ok(s) if s.success() => objects.push(obj),
            Ok(s) => {
                eprintln!("openepl: clang failed with status {s} on {}", path.display());
                let _ = std::fs::remove_dir_all(&dir);
                return Err(1);
            }
            Err(e) => {
                eprintln!("openepl: could not invoke {driver}: {e}");
                let _ = std::fs::remove_dir_all(&dir);
                return Err(1);
            }
        }
    }

    // `ar rcs` replaces rather than appends, so a stale archive of the same name
    // cannot leave old objects behind.
    let _ = std::fs::remove_file(out_lib);
    let mut ar = Command::new("ar");
    ar.arg("rcs").arg(out_lib).args(&objects);
    let status = ar.status();
    let _ = std::fs::remove_dir_all(&dir);
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => {
            eprintln!("openepl: ar failed with status {s}");
            Err(1)
        }
        Err(e) => {
            eprintln!("openepl: could not invoke ar: {e}");
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
    // Walking up from the working directory covers running inside the repo.
    if let Some(found) = std::env::current_dir().ok().and_then(walk_up_for_runtime) {
        return Some(found);
    }
    // …and walking up from the executable covers everything else: `openepl new`
    // is run from wherever the user's project will live, and the templates and
    // runtime are next to the binary, not next to them.
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .and_then(walk_up_for_runtime)
}

fn walk_up_for_runtime(start: PathBuf) -> Option<PathBuf> {
    let mut dir = start;
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
