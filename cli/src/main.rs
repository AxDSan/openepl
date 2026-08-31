//! `openepl` — the command-line toolchain.
//!
//! Subcommands:
//!   openepl build <in.oir> [-o <out>]   parse -> lower -> clang -> native binary
//!   openepl run   <in.oir> [-o <out>]   build, then execute it
//!   openepl emit  <in.oir>              print the generated LLVM IR to stdout
//!   openepl lsp                         language server (stdio) for editors
//!   openepl commands                    list available commands and components
//!   openepl templates                   list project templates
//!   openepl new <tmpl> <dir>            create a project from a template
//!
//! The pipeline lowers a module to LLVM IR, then has `clang` assemble it and
//! link the runtime sources, producing an ordinary native executable.

use std::path::{Path, PathBuf};
use std::process::{exit, Command};

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
         openepl emit  <in.oir>              print generated LLVM IR\n  \
         openepl inspect <in.oir>            dump the form model (for the designer)\n  \
         openepl lsp                         language server over stdio (see docs/editors.md)\n  \
         openepl commands [--use <lib>]      list the commands and components available\n  \
         openepl templates                   list the available project templates\n  \
         openepl new <template> <dir>        create a project from a template\n"
    );
}

/// What a build/emit invocation was asked to do.
struct Io {
    input: PathBuf,
    output: Option<PathBuf>,
    /// Overrides the module's own `target` declaration when given.
    target: Option<Target>,
}

/// Parse `<in.oir> [-o out] [--target kind]` from an argument slice.
fn parse_io(rest: &[String]) -> Result<Io, String> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut target: Option<Target> = None;
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
    let plan = match libload::load_metadata(repo_root, &uses) {
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
        println!("sub: {}", sub.name);
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
    if let Err(code) = clang_link(&ll_path, &repo_root, &plan, &out_bin, target) {
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
    let plan = if require_impl {
        libload::load(&repo_root, &module.uses)?
    } else {
        libload::load_metadata(&repo_root, &module.uses)?
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
) -> Result<(), i32> {
    let cfg = &plan.build;
    let driver = if cfg.needs_cxx { "clang++" } else { "clang" };

    // Flags every invocation needs, whether we are linking a program or
    // compiling one object at a time for an archive.
    let mut common: Vec<String> = vec![
        "-O0".into(),
        "-ffunction-sections".into(),
        "-fdata-sections".into(),
        "-Wno-override-module".into(),
        "-I".into(),
        repo_root.join("abi").display().to_string(),
        "-I".into(),
        repo_root.join("runtime").display().to_string(),
    ];
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

    let mut cmd = Command::new(driver);
    cmd.args(&common);
    for (path, lang) in &inputs {
        if let Some(l) = lang {
            cmd.arg("-x").arg(l);
        }
        cmd.arg(path);
    }
    for a in &cfg.link_args {
        cmd.arg(a);
    }
    match libload::pkg_config_flags(&cfg.pkg_config, "--libs") {
        Ok(flags) => {
            for f in flags {
                cmd.arg(f);
            }
        }
        Err(e) => {
            eprintln!("openepl: {e}");
            return Err(1);
        }
    }
    cmd.arg("-lm"); // libm for the floating-point commands
    if target == Target::SharedLib {
        cmd.arg("-shared");
    } else {
        // Dead-strip: the headline property of the BlackMoon model.
        // Only for programs — a library must keep exports no host has linked
        // yet, and --gc-sections would drop every one of them.
        cmd.arg("-Wl,--gc-sections");
    }
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
