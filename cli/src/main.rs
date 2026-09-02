//! `openepl` — the command-line toolchain.
//!
//! Subcommands:
//!   openepl build <in.oir> [-o <out>]   parse -> lower -> clang -> native binary
//!   openepl run   <in.oir> [-o <out>]   build, then execute it
//!     …either with --release            optimised, hardened and stripped
//!   openepl emit  <in.oir>              print the generated LLVM IR to stdout
//!   openepl lsp                         language server (stdio) for editors
//!   openepl commands                    list available commands and components
//!   openepl inspect <in.oir>            dump the form model, one fact per line
//!   openepl templates                   list project templates
//!   openepl new <tmpl> <dir>            create a project from a template
//!   openepl kits                        list resolved kits and where they came from
//!   openepl kit add <path>              install a kit into ~/.openepl/kits
//!   openepl project <file-or-dir>       dump a project file's resolved fields
//!   openepl version                     the toolchain and ABI versions
//!
//! `build`, `run`, `emit` and `inspect` take a `project.oeproj`, or a directory
//! holding one, in place of the `.oir`; the entry file comes from the project.
//!
//! The pipeline lowers a module to LLVM IR, then has `clang` assemble it and
//! link the runtime sources, producing an ordinary native executable.

use std::path::{Path, PathBuf};
use std::process::{exit, Command};

mod kit;
mod libload;
mod lsp;
mod lsp_index;
mod project;
mod templates;

use std::collections::HashMap;

use openepl_backend::lower_module;
use openepl_ir::registry::Registry;
use openepl_ir::validate::{validate_with, Hints};
use openepl_ir::{parse, Module, Target};

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
        "project" => project::cmd_project(rest),
        "version" | "--version" | "-V" => {
            print!("{}", version_text());
            0
        }
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
         openepl kit add <path>              install a kit into ~/.openepl/kits\n  \
         openepl project <file-or-dir>       dump a project file's resolved fields\n  \
         openepl version                     print the toolchain and ABI versions\n\n\
         Wherever <in.oir> is accepted, a project.oeproj or its directory is too.\n"
    );
}

/// `openepl version` — two lines, each one fact, so Studio reads the first
/// and a library author checking compatibility reads the second.
///
/// The ABI number is read out of the header that defines it rather than
/// restated here: a version this command reports and a version the loader
/// checks have to be the same number, and the header is where both live.
fn version_text() -> String {
    const ABI_HEADER: &str = include_str!("../../abi/openepl_abi.h");
    let abi = ABI_HEADER
        .lines()
        .find_map(|l| l.strip_prefix("#define OPENEPL_ABI_VERSION"))
        .map(str::trim)
        .unwrap_or("?");
    format!("openepl {}\nabi {abi}\n", env!("CARGO_PKG_VERSION"))
}

/// What a build/emit invocation was asked to do.
struct Io {
    input: PathBuf,
    output: Option<PathBuf>,
    /// Overrides the module's own `target` declaration when given.
    target: Option<Target>,
    /// Optimise, harden and strip the built program.
    release: bool,
    /// Where the output goes when the input came through a project file: the
    /// project's directory and name. Naming it after the entry would call
    /// every program `main`, and putting it in the working directory would
    /// collide with the project directory itself for `openepl build <dir>`.
    project_output: Option<PathBuf>,
}

/// Parse `<in.oir> [-o out] [--target kind] [--release]` from an argument slice.
///
/// The input may be a project file or its directory. Resolved here, in the one
/// place every subcommand parses its input, so that build, run, emit and
/// inspect cannot disagree about what a project is. The project's `target:`
/// stands in for `--target` only when none was given: a flag on the command
/// line is the more deliberate of the two.
fn parse_io(rest: &[String]) -> Result<Io, String> {
    let mut io = parse_io_args(rest)?;
    if project::is_project_path(&io.input) {
        let p = project::load(&io.input)?;
        io.input = p.main;
        io.target = io.target.or(p.target);
        let dir = p.file.parent().unwrap_or(Path::new(".")).to_path_buf();
        io.project_output = Some(dir.join(&p.name));
    }
    Ok(io)
}

fn parse_io_args(rest: &[String]) -> Result<Io, String> {
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
        project_output: None,
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
        Ok((ll, _plan, _t, _m)) => {
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
        // `kind:` and `editor:` are ADDED lines, never a change to the shape of
        // the existing ones: the designer's catalog, gen-docs.sh and
        // check-docs.sh all read this output by prefix, and a reader that
        // predates a line kind must still see exactly what it saw before. A
        // designer that only knows the kind of the components it was linked
        // against files a kit's visual control under the System tray, so the
        // kind has to travel with the listing.
        let kind = match desc.kind {
            openepl_ir::registry::ComponentKind::Visual => "visual",
            openepl_ir::registry::ComponentKind::NonVisual => "nonvisual",
        };
        println!("kind: {type_name} {kind}");
        for p in &desc.properties {
            println!("property: {type_name} {} {}", p.name, p.ty.as_str());
            // Absent means the plain editor the type implies, which is what the
            // descriptor's empty hint already means.
            if !p.editor.is_empty() {
                println!("editor: {type_name} {} {}", p.name, p.editor);
            }
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
        print_members(&form.name, &form.properties, &form.handlers);
        for c in &form.children {
            println!("component: {} {}", c.id, c.type_name);
            print_members(&c.id, &c.properties, &c.handlers);
        }
    }
    // A module-level component is a DISTINCT line kind. The designer folds
    // `component:` into the form's children, and a timer written back inside
    // the form is source the compiler refuses.
    let spans = module_component_spans(&src, &module);
    for (c, span) in module.components().zip(spans) {
        match span {
            Some((a, b)) => println!("modcomponent: {} {} span={a}..{b}", c.id, c.type_name),
            None => println!("modcomponent: {} {}", c.id, c.type_name),
        }
        print_members(&c.id, &c.properties, &c.handlers);
    }
    0
}

/// The `prop:` and `handler:` lines of a form or component, keyed by its id.
fn print_members(id: &str, props: &[(String, openepl_ir::Expr)], handlers: &[(String, String)]) {
    for (name, value) in props {
        println!("prop: {id} {name} {}", escape_value(&literal_text(value)));
    }
    for (event, handler) in handlers {
        println!("handler: {id} {event} {handler}");
    }
}

/// Where each module-level component sits in the file, in declaration order.
///
/// The parser records a span for a form and not for a component, so this
/// finds it again from the token stream: a header is `type id` at the start of
/// a line outside every form, and the first `end` after it closes the block,
/// because a component body holds only properties and bindings and nothing
/// that nests. Matching on the exact (type, id) pair is what keeps
/// `record point` — the same two tokens — from being taken for one.
///
/// The right home for this is a `line_span` on `ir::Component`. Until then a
/// component the walk cannot place gets no span — which cannot happen for a
/// file the parser accepted, since the header it looks for is the one the
/// parser consumed — and the designer would append it as new on save rather
/// than splice at a guess.
fn module_component_spans(src: &str, module: &Module) -> Vec<Option<(usize, usize)>> {
    use openepl_ir::lexer::{lex, Tok};
    let toks = match lex(src) {
        Ok(t) => t,
        Err(_) => return module.components().map(|_| None).collect(),
    };
    let form_spans: Vec<(usize, usize)> = module.forms().map(|f| f.line_span).collect();
    let mut cursor = 0;
    module
        .components()
        .map(|c| {
            let mut i = cursor;
            while i + 2 < toks.len() {
                let at_line_start = i == 0 || matches!(toks[i - 1].tok, Tok::Newline);
                let line = toks[i].line;
                let header = at_line_start
                    && matches!(&toks[i].tok, Tok::Ident(t) if *t == c.type_name)
                    && matches!(&toks[i + 1].tok, Tok::Ident(id) if *id == c.id)
                    && matches!(toks[i + 2].tok, Tok::Newline)
                    && !form_spans.iter().any(|(a, b)| (*a..=*b).contains(&line));
                if header {
                    let end = toks[i + 3..].iter().find(|t| matches!(t.tok, Tok::End))?;
                    cursor = i + 3;
                    return Some((line, end.line));
                }
                i += 1;
            }
            None
        })
        .collect()
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

/// Keep a `prop:` value on its one line.
///
/// The output is read a line at a time, so a raw newline in a memo's text
/// used to arrive as extra unlabelled lines the reader had to guess were
/// continuations. Backslash is escaped so the reversal is unambiguous, and NUL
/// because a C reader working in `char *` stops at one. Everything else,
/// including tab, cannot break a line and travels raw.
fn escape_value(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for ch in v.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out
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
    let (mut ll, plan, target, module) = match compile(&input, io.target) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("openepl: {e}");
            return 1;
        }
    };
    let out_bin = io
        .output
        .unwrap_or_else(|| default_output(&input, io.project_output.as_deref(), target));

    // Only a program builds a form, so only a program has pictures to carry.
    if target.is_executable() {
        match embed_resources(&module, &input) {
            Ok(Some(table)) => ll.push_str(&table),
            Ok(None) => {}
            Err(e) => {
                eprintln!("openepl: {e}");
                return 1;
            }
        }
    }

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
fn compile(
    input: &Path,
    target_override: Option<Target>,
) -> Result<(String, libload::LibPlan, Target, Module), String> {
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
) -> Result<(String, libload::LibPlan, Target, Module), String> {
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

    if let Err(errs) = validate_hinted(&module, &plan.registry, &repo_root) {
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
    Ok((ll, plan, target, module))
}

/// Validate, and when a command is unknown, say which library has it.
///
/// The same two passes the language server makes (`Server::diagnose`): the
/// cheap one first, and the map of every kit's commands only when an unknown
/// command is what went wrong — it costs an introspection build per kit, and
/// a program that validates cleanly should not pay for it. Without this the
/// editor said "add `use file`" and the terminal did not, and the terminal is
/// where a build fails.
fn validate_hinted(
    module: &Module,
    registry: &Registry,
    repo_root: &Path,
) -> Result<(), Vec<openepl_ir::validate::ValidateError>> {
    let Err(errs) = validate_with(module, registry, &Hints::default()) else {
        return Ok(());
    };
    if !errs.iter().any(|e| e.msg.contains("unknown command `")) {
        return Err(errs);
    }
    let hints = Hints {
        elsewhere: elsewhere(repo_root),
    };
    match validate_with(module, registry, &hints) {
        Err(better) => Err(better),
        Ok(()) => Err(errs),
    }
}

/// Every command of every kit the toolchain can see, and which kit it is in.
/// One kit at a time, as `Server::elsewhere` does: two kits may legitimately
/// export the same name, and a registry holding both would refuse to load.
fn elsewhere(repo_root: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for k in kit::resolve_all(repo_root) {
        let uses = vec![k.name.clone()];
        let Ok(root) = kit::overlay_root(repo_root, &uses) else { continue };
        if let Ok(plan) = libload::load_metadata(&root, &uses) {
            for (name, _) in plan.registry.iter() {
                map.entry(name.to_string()).or_insert_with(|| k.name.clone());
            }
        }
    }
    for name in Registry::core().names() {
        map.remove(name);
    }
    map
}

/// Compile every picture a form names INTO the program.
///
/// `image.source = "logo.png"` is otherwise a promise about the machine the
/// program was built on, and "ship one file" is the claim the whole model rests
/// on. The bytes go into the program's own object as a table the `ui` library
/// reads instead of reaching for the filesystem (`libs/ui/ui_rmlui.cpp`).
///
/// A source that does not exist is a BUILD error. The alternative — an empty
/// picture in a running program — is the same missing file discovered by
/// whoever the program was shipped to.
///
/// Returns `None` when the module names no resources, so a program that has
/// none defines no table at all and the weak declaration on the other side sees
/// the empty one.
fn embed_resources(module: &Module, input: &Path) -> Result<Option<String>, String> {
    // Relative to the SOURCE, not to the working directory: a project is built
    // from wherever the person happens to be standing.
    let base = input.parent().unwrap_or(Path::new("."));
    let mut found: Vec<(String, Vec<u8>)> = Vec::new();
    for form in module.forms() {
        for child in &form.children {
            if child.type_name != "image" {
                continue;
            }
            for (name, value) in &child.properties {
                if name != "source" {
                    continue;
                }
                let src = literal_text(value);
                if src.is_empty() {
                    continue;
                }
                if found.iter().any(|(n, _)| *n == src) {
                    continue;
                }
                let path = base.join(&src);
                let bytes = std::fs::read(&path).map_err(|e| {
                    format!(
                        "{}: `{}` has source `{src}`, which cannot be read: {e}",
                        input.display(),
                        child.id
                    )
                })?;
                if bytes.is_empty() {
                    // A zero-length picture is a file that exists and says
                    // nothing, which is the failure this check is for.
                    return Err(format!(
                        "{}: `{}` has source `{src}`, which is empty",
                        input.display(),
                        child.id
                    ));
                }
                found.push((src, bytes));
            }
        }
    }
    if found.is_empty() {
        return Ok(None);
    }

    let mut out = String::from(
        "\n; Resources embedded at build time; read by libs/ui through a null-terminated\n\
         ; table, so a program that names none can leave the symbol undefined.\n",
    );
    for (i, (name, bytes)) in found.iter().enumerate() {
        out.push_str(&format!(
            "@.res.name{i} = private unnamed_addr constant [{} x i8] c\"{}\\00\"\n",
            name.len() + 1,
            llvm_bytes(name.as_bytes())
        ));
        out.push_str(&format!(
            "@.res.data{i} = private unnamed_addr constant [{} x i8] c\"{}\"\n",
            bytes.len(),
            llvm_bytes(bytes)
        ));
    }
    let mut rows: Vec<String> = found
        .iter()
        .enumerate()
        .map(|(i, (_, bytes))| {
            format!(
                "{{ ptr, ptr, i64 }} {{ ptr @.res.name{i}, ptr @.res.data{i}, i64 {} }}",
                bytes.len()
            )
        })
        .collect();
    rows.push("{ ptr, ptr, i64 } zeroinitializer".to_string());
    out.push_str(&format!(
        "@oe_embedded_resources = constant [{} x {{ ptr, ptr, i64 }}] [{}]\n",
        rows.len(),
        rows.join(", ")
    ));
    Ok(Some(out))
}

/// Bytes as an LLVM string body. Everything outside plain printable ASCII goes
/// as `\XX`, which is the only form that survives a byte a text editor would
/// otherwise eat.
fn llvm_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        if (0x20..0x7f).contains(&b) && b != b'"' && b != b'\\' {
            out.push(b as char);
        } else {
            out.push_str(&format!("\\{b:02X}"));
        }
    }
    out
}

fn default_output(input: &Path, project_output: Option<&Path>, target: Target) -> PathBuf {
    let (dir, stem) = match project_output {
        Some(p) => (
            p.parent().unwrap_or(Path::new(".")).to_path_buf(),
            p.file_name().and_then(|s| s.to_str()).unwrap_or("a").to_string(),
        ),
        None => (
            PathBuf::new(),
            input.file_stem().and_then(|s| s.to_str()).unwrap_or("a").to_string(),
        ),
    };
    // Libraries follow the platform convention, so a host's linker finds them
    // by the name it expects (`-lgreet` wants `libgreet.so`).
    dir.join(match target {
        Target::Console | Target::Gui => stem,
        Target::SharedLib => format!("lib{stem}.so"),
        Target::StaticLib => format!("lib{stem}.a"),
    })
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
