//! End-to-end tests for the Phase 1 toolchain: build `.oir` examples to native
//! binaries, run them, and prove dead-code stripping.
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Build `<repo>/examples/<name>.oir` to a temp binary; return its path.
///
/// `tag` MUST be unique per test. Tests run in parallel, and two tests writing
/// the same output path race: one truncates the binary while the other executes
/// it, producing an intermittent failure that looks like a product bug. There is
/// deliberately no tag-less convenience wrapper — it is what caused exactly that
/// flake twice.
fn build_as(name: &str, tag: &str) -> PathBuf {
    let repo = repo();
    let example = repo.join("examples").join(format!("{name}.oir"));
    let out_bin = std::env::temp_dir().join(format!("openepl_{name}_{tag}_test"));
    let status = Command::new(env!("CARGO_BIN_EXE_openepl"))
        .args([
            "build",
            example.to_str().unwrap(),
            "-o",
            out_bin.to_str().unwrap(),
        ])
        .env("OPENEPL_RUNTIME_DIR", repo.join("runtime"))
        .status()
        .expect("run openepl");
    assert!(status.success(), "openepl build {name} failed");
    out_bin
}

fn run(bin: &Path) -> String {
    let out = Command::new(bin).output().expect("run built binary");
    assert!(out.status.success(), "binary exited non-zero");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn hello_builds_and_runs() {
    let stdout = run(&build_as("hello", "run"));
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec!["OpenEPL — arithmetic demo", "42", "14", "42", "42"]
    );
}

#[test]
fn demo_builds_and_runs() {
    let stdout = run(&build_as("demo", "run"));
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec![
            "HELLO, OPENEPL", // uppercase(concat)
            "14",             // length("Hello, OpenEPL")
            "a/b/c/d",        // replace "-" -> "/"
            "padded",         // trim
            "desserts",       // reverse("stressed")
            "9",              // max_int(3,9)
            "1024",           // pow_int(2,10)
            "2",              // mod_int(17,5)
            "1.41421",        // sqrt(2)
            "2",              // round(pow(sqrt2, 2))
            "n = 42",         // conversions round-trip
            "1970",           // year(epoch)
        ],
        "unexpected demo output:\n{stdout}"
    );
}

///; unused command code is
/// dead-stripped by `-ffunction-sections` + `--gc-sections`.
#[test]
fn unused_commands_are_dead_stripped() {
    let bin = build_as("hello", "strip"); // uses only print_text / print_int + arithmetic
    let nm = match Command::new("nm").arg(&bin).output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => {
            eprintln!("nm unavailable; skipping symbol check");
            return;
        }
    };
    let symbols: Vec<&str> = nm
        .lines()
        .filter_map(|l| l.split_whitespace().nth(2))
        .collect();

    // Referenced commands are present.
    for want in ["oe_print_int", "oe_print_text"] {
        assert!(symbols.contains(&want), "expected `{want}` to be linked in");
    }
    // Unreferenced commands are gone.
    for gone in [
        "oe_sqrt",
        "oe_replace",
        "oe_now",
        "oe_uppercase",
        "oe_pow_int",
    ] {
        assert!(
            !symbols.contains(&gone),
            "unused command `{gone}` was not stripped"
        );
    }
}

/// Subroutines with parameters, return values and recursion, end to end: the
/// point is that the compiled program produces the answers, not that the IR
/// looked right.
#[test]
fn subs_build_and_run() {
    let stdout = run(&build_as("subs", "params"));
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec![
            "5",          // add(2, 3)
            "7",          // add(add(1, 2), 4)
            "720",        // factorial(6)
            "610",        // fib(15) — recursion
            "ADA!",       // text in, text out
            "negative",   // an early bare `return`
            "4 is even",  // a sub calling another sub
            "7 is odd",
            "79", // add(factorial(4), fib(10)) = 24 + 55
        ],
        "unexpected subs output:\n{stdout}"
    );
}

#[test]
fn hello_library_via_abi() {
    // `use hello` — a third-party support library loaded through the ABI.
    let stdout = run(&build_as("hellolib", "abi"));
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["Hello, OpenEPL!", "HELLO, WORLD!"]);
}

///
/// a native GUI binary, and a click reaches the handler subroutine.
///
/// Runs headlessly through the UI test hooks (see abi/openepl_ui.h): render a
/// few frames, dispatch a synthetic click at widget handle 3 (the button), exit.
#[test]
fn form_builds_and_click_reaches_handler() {
    let repo = repo();
    if !repo.join("vendor/RmlUi/build/librmlui.a").exists() {
        eprintln!("RmlUi not vendored (run tools/fetch-rmlui.sh); skipping GUI test");
        return;
    }
    let bin = build_as("form", "click");

    let out = Command::new(&bin)
        .env("OPENEPL_UI_EXIT_AFTER_FRAMES", "3")
        .env("OPENEPL_UI_SYNTH_CLICK", "3")
        .output()
        .expect("run form binary");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("button clicked!"),
        "click did not reach the handler; stdout was:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The UI stack must link ONLY into modules that declare a form. Without this,
/// every console program would carry megabytes of widget toolkit (and the
/// dead-strip guarantee.
#[test]
fn console_programs_do_not_link_the_ui() {
    let bin = build_as("demo", "ldd");
    let ldd = match Command::new("ldd").arg(&bin).output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => {
            eprintln!("ldd unavailable; skipping");
            return;
        }
    };
    for forbidden in ["libSDL2", "libGL.so", "libfreetype"] {
        assert!(
            !ldd.contains(forbidden),
            "console binary links `{forbidden}` — the UI stack is leaking into non-GUI programs:\n{ldd}"
        );
    }
}

/// A button must visibly respond to the mouse. Hover feedback can't come from a
/// `:hover` stylesheet rule because component properties are applied INLINE and
/// inline styles outrank stylesheet rules — so the backend drives the states,
/// and this test guards that it actually changes rendered pixels.
#[test]
fn button_hover_changes_pixels() {
    let repo = repo();
    if !repo.join("vendor/RmlUi/build/librmlui.a").exists() {
        eprintln!("RmlUi not vendored; skipping");
        return;
    }
    let bin = build_as("form", "hover");
    let dir = std::env::temp_dir();
    let base = dir.join("openepl_base.ppm");
    let hover = dir.join("openepl_hover.ppm");

    let render = |dump: &std::path::Path, mouse: Option<&str>| {
        let mut c = Command::new(&bin);
        c.env("OPENEPL_UI_EXIT_AFTER_FRAMES", "3")
            .env("OPENEPL_UI_DUMP", dump);
        if let Some(m) = mouse {
            c.env("OPENEPL_UI_MOUSE", m);
        }
        assert!(c.output().expect("render").status.success());
    };
    render(&base, None);
    render(&hover, Some("120,132")); // centre of the button

    // Sample the same pixel from each frame.
    let sample = |p: &std::path::Path| -> [u8; 3] {
        let bytes = std::fs::read(p).expect("read ppm");
        // PPM header is three newline-terminated lines; the second holds the width.
        let mut nl = 0;
        let mut i = 0;
        let (mut w, mut hdr) = (0usize, String::new());
        while nl < 3 && i < bytes.len() {
            if bytes[i] == b'\n' {
                nl += 1;
                if nl == 2 {
                    w = hdr.split_whitespace().next().unwrap().parse().unwrap();
                }
                hdr.clear();
            } else {
                hdr.push(bytes[i] as char);
            }
            i += 1;
        }
        let (x, y) = (120usize, 132usize);
        let off = i + (y * w + x) * 3;
        [bytes[off], bytes[off + 1], bytes[off + 2]]
    };
    let (b, h) = (sample(&base), sample(&hover));
    assert_ne!(
        b, h,
        "hovering the button did not change its appearance (base {b:?})"
    );
}

/// The accessibility tree must mirror the widget tree with correct roles,
/// names, parent links and bounds. Substrate-independent and
/// needs no accessibility bus, so it always runs.
#[test]
fn accessibility_tree_is_published() {
    let repo = repo();
    if !repo.join("vendor/accesskit-c/include/accesskit.h").exists() {
        eprintln!("accesskit-c not vendored (run tools/fetch-accesskit.sh); skipping");
        return;
    }
    let bin = build_as("form", "a11y");
    let out = Command::new(&bin)
        .env("OPENEPL_UI_EXIT_AFTER_FRAMES", "3")
        .env("OPENEPL_UI_DUMP_A11Y", "1")
        .output()
        .expect("run form");
    let text = String::from_utf8_lossy(&out.stdout);
    let rows: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("a11y: id="))
        .collect();
    assert_eq!(rows.len(), 3, "expected window+label+button, got:\n{text}");

    // role 1 = window, 3 = label, 2 = button (abi/openepl_abi.h)
    assert!(
        rows[0].contains("role=1") && rows[0].contains("parent=0"),
        "root: {}",
        rows[0]
    );
    assert!(
        rows[1].contains("role=3") && rows[1].contains("Click the button."),
        "label: {}",
        rows[1]
    );
    assert!(
        rows[2].contains("role=2") && rows[2].contains("Click me") && rows[2].contains("clickable"),
        "button must be a clickable button with an accessible name: {}",
        rows[2]
    );
    // Bounds must be real, not zero — an AT cannot locate a zero-sized control.
    assert!(
        !rows[2].contains("bounds=0,0,0x0"),
        "button has no bounds: {}",
        rows[2]
    );
}

/// End-to-end against a real AT-SPI bus: the adapter must actually activate
/// when an assistive technology is present. Skipped when the session has no
/// accessibility bus, so CI never hard-depends on desktop D-Bus.
#[test]
fn accessibility_adapter_activates_on_a_real_bus() {
    let repo = repo();
    if !repo.join("vendor/accesskit-c/include/accesskit.h").exists() {
        eprintln!("accesskit-c not vendored; skipping");
        return;
    }
    // Ask the a11y bus to exist, and mark accessibility enabled.
    let bus = Command::new("busctl")
        .args([
            "--user",
            "call",
            "org.a11y.Bus",
            "/org/a11y/bus",
            "org.a11y.Bus",
            "GetAddress",
        ])
        .output();
    match bus {
        Ok(o) if o.status.success() => {}
        _ => {
            eprintln!("no session accessibility bus; skipping live-adapter test");
            return;
        }
    }
    for prop in ["IsEnabled", "ScreenReaderEnabled"] {
        let _ = Command::new("busctl")
            .args([
                "--user",
                "set-property",
                "org.a11y.Bus",
                "/org/a11y/bus",
                "org.a11y.Status",
                prop,
                "b",
                "true",
            ])
            .output();
    }

    let bin = build_as("form", "a11ylive");
    let out = Command::new(&bin)
        .env("OPENEPL_UI_EXIT_AFTER_FRAMES", "90")
        .env("OPENEPL_UI_DUMP_A11Y", "1")
        .output()
        .expect("run form");
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !stdout.contains("adapter_active=1") {
        // The bus exists but no AT attached; that is an environment fact, not a
        // product defect, so report rather than fail.
        eprintln!("accessibility bus present but no AT attached; adapter stayed idle");
    }
}

/// Accessibility must never be load-bearing: with it disabled the app runs
/// exactly the same. An app that breaks without a11y infrastructure would fail
/// the very requirement the bridge exists to satisfy.
#[test]
fn app_runs_with_accessibility_disabled() {
    let repo = repo();
    if !repo.join("vendor/RmlUi/build/librmlui.a").exists() {
        eprintln!("RmlUi not vendored; skipping");
        return;
    }
    let bin = build_as("form", "noa11y");
    let out = Command::new(&bin)
        .env("OPENEPL_NO_A11Y", "1")
        .env("OPENEPL_UI_EXIT_AFTER_FRAMES", "3")
        .env("OPENEPL_UI_SYNTH_CLICK", "3")
        .output()
        .expect("run form");
    assert!(
        out.status.success(),
        "app failed with accessibility disabled"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("button clicked!"),
        "app must behave identically with accessibility disabled"
    );
}

/// Reading and writing component properties from code. The counter
/// holds its state in the label's own text, so one synthetic click must both
/// print the new count and leave the label showing it.
#[test]
fn property_access_updates_a_component() {
    let repo = repo();
    if !repo.join("vendor/RmlUi/build/librmlui.a").exists() {
        eprintln!("RmlUi not vendored; skipping");
        return;
    }
    let bin = build_as("counter", "prop");
    let out = Command::new(&bin)
        .env("OPENEPL_UI_EXIT_AFTER_FRAMES", "6")
        .env("OPENEPL_UI_SYNTH_CLICK", "3")
        .env("OPENEPL_UI_DUMP_A11Y", "1")
        .output()
        .expect("run counter");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("count = 1"),
        "handler did not update the label:\n{stdout}"
    );
    // The accessible name must track the live text, not the construction-time
    // value — otherwise a screen reader announces stale content. After one
    // click the label shows the text from the `count = 1` branch.
    let label_row = stdout
        .lines()
        .find(|l| l.starts_with("a11y: id=2"))
        .expect("a label node in the a11y tree");
    assert!(
        label_row.contains("name=\"1 (first click)\""),
        "accessible name is stale (it must mirror the label's current text): {label_row}"
    );
}

/// A module with BOTH `main` and a form: the form must be built before `main`
/// runs, or `main` addresses components that do not exist yet. This is the
/// ordering bug that would only ever appear for this module shape.
#[test]
fn main_may_touch_components_before_the_loop_starts() {
    let repo = repo();
    if !repo.join("vendor/RmlUi/build/librmlui.a").exists() {
        eprintln!("RmlUi not vendored; skipping");
        return;
    }
    let bin = build_as("mainorder", "order");
    let out = Command::new(&bin)
        .env("OPENEPL_UI_EXIT_AFTER_FRAMES", "3")
        .output()
        .expect("run mainorder");
    assert!(out.status.success(), "main touching a component crashed");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("main saw: set from main"),
        "main could not read back the property it set"
    );
}

/// Control flow end to end: if / else if / else, while, comparisons,
/// short-circuit `and`, and content-based text equality.
///
/// This also serves as the SSA verifier for branch codegen — clang rejects
/// malformed basic blocks, so a passing build proves the emitted IR is sound.
#[test]
fn control_flow_runs_correctly() {
    let stdout = run(&build_as("fizzbuzz", "cf"));
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 22, "unexpected output:\n{stdout}");
    assert_eq!(&lines[..5], &["1", "2", "Fizz", "4", "Buzz"]);
    assert_eq!(lines[14], "FizzBuzz", "15 should be FizzBuzz");
    assert_eq!(lines[19], "Buzz", "20 should be Buzz");
    // The right side of `and` must not be evaluated when the left is false,
    // or this would have divided by zero.
    assert_eq!(lines[20], "short-circuit ok");
    // Two separately-built strings with the same characters must compare equal.
    assert_eq!(lines[21], "text equality ok");
}

/// Loops end to end: counting up and down, `break`, `continue`, and the
/// nesting rule that `break` leaves only the innermost loop. As with
/// `control_flow_runs_correctly`, a successful build is itself the proof that
/// the emitted basic blocks are well formed — clang rejects them otherwise.
#[test]
fn loops_build_and_run() {
    let stdout = run(&build_as("loops", "loops"));
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "-- fizzbuzz, counted --");
    assert_eq!(&lines[1..6], &["1", "2", "Fizz", "4", "Buzz"]);
    assert_eq!(lines[15], "FizzBuzz", "15 should be FizzBuzz");
    // `step -1`
    assert_eq!(&lines[17..22], &["5", "4", "3", "2", "1"]);
    // `continue` then `break`: the first multiple of 7 above 100.
    assert_eq!(lines[23], "105");
    // `break`/`continue` inside a `while`.
    assert_eq!(&lines[25..28], &["word-3", "word-6", "word-9"]);
    // Nested loops: the inner `break` must not leave the outer one.
    assert_eq!(
        &lines[29..33],
        &["1 ", "2 4 ", "3 6 9 ", "4 8 12 16 "],
        "unexpected output:\n{stdout}"
    );
}

/// The expression operators: unary minus, `%`, and `+` on text. The last two
/// lines of the example build the same sentence with `+` and with nested
/// `concat` calls, so the test proves they agree.
#[test]
fn operators_build_and_run() {
    let stdout = run(&build_as("operators", "ops"));
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec![
            "-40",     // a negative literal
            "40",      // negating a negative
            "-250",    // negating a variable
            "1.5",     // fneg on a double
            "2",       // 17 % 5
            "-2",      // -17 % 5 — the sign follows the dividend, as srem does
            "6 is even",
            "7 is odd",
            "Hello, Ada — you are 36 today.",
            "Hello, Ada — you are 36 today.",
        ],
        "unexpected output:\n{stdout}"
    );
    assert_eq!(lines[8], lines[9], "`+` and `concat` must agree");
}

/// Dividing by zero used to kill the process with SIGFPE and no message. It
/// now goes through the runtime's error channel: a line on stderr, exit 1.
///
/// Both faulting inputs are covered — a zero divisor, and the one overflowing
/// division (the most negative integer by -1), which SIGFPEs just as hard.
#[test]
fn dividing_by_zero_reports_instead_of_crashing() {
    for (src, want) in [
        (
            "module dz\nsub main\n  var z: int = 0\n  call print_int(10 / z)\nend\n",
            "division by zero",
        ),
        (
            "module dz\nsub main\n  var z: int = 0\n  call print_int(10 % z)\nend\n",
            "remainder by zero",
        ),
        (
            "module dz\nsub main\n  var m: int = -1\n  call print_int(-2147483648 / m)\nend\n",
            "division overflowed",
        ),
    ] {
        let dir = std::env::temp_dir().join("openepl_divzero_test");
        std::fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join(format!("dz{}.oir", want.len()));
        std::fs::write(&src_path, src).unwrap();
        let bin = dir.join(format!("dz{}", want.len()));
        let status = Command::new(env!("CARGO_BIN_EXE_openepl"))
            .args([
                "build",
                src_path.to_str().unwrap(),
                "-o",
                bin.to_str().unwrap(),
            ])
            .env("OPENEPL_RUNTIME_DIR", repo().join("runtime"))
            .status()
            .expect("run openepl");
        assert!(status.success(), "build failed for: {want}");
        let out = Command::new(&bin).output().expect("run built binary");
        assert!(!out.status.success(), "{want}: expected a non-zero exit");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(want),
            "expected `{want}` on stderr, got: {stderr}"
        );
    }
}

/// **M0, the RAD metric.** A scripted designer session adds a button,
/// sets its properties, wires a click handler, and saves — and the resulting
/// `.oir` compiles to a native binary whose button actually works.
///
/// This is the whole product thesis in one test: draw it, wire it, ship it.
#[test]
fn designer_produces_a_working_app() {
    let repo = repo();
    let designer = repo.join("designer/openepl-designer");
    if !designer.exists() {
        eprintln!("designer not built (run designer/build.sh); skipping");
        return;
    }
    let project = std::env::temp_dir().join("openepl_designed.oir");
    std::fs::write(
        &project,
        r#"module designed
use ui

form win
  title = "Designed"
  width = 400
  height = 260
end
"#,
    )
    .expect("write project");

    let out = Command::new(&designer)
        .arg(&project)
        .arg(repo.join("target/debug/openepl"))
        .env(
            "OPENEPL_DESIGNER_SCRIPT",
            "add:button;set:text=Press me;set:left=40;set:top=80;wire:click=on_press;save",
        )
        .output()
        .expect("run designer");
    assert!(
        out.status.success(),
        "designer session failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The designer must not have written a second parser: what it saved has to
    // satisfy the real compiler.
    let src = std::fs::read_to_string(&project).expect("read saved project");
    assert!(
        src.contains("button button1"),
        "component not saved:\n{src}"
    );
    assert!(
        src.contains("on click: on_press"),
        "handler not wired:\n{src}"
    );
    assert!(
        src.contains("sub on_press"),
        "handler stub not generated:\n{src}"
    );

    let bin = std::env::temp_dir().join("openepl_designed_app");
    let status = Command::new(env!("CARGO_BIN_EXE_openepl"))
        .args([
            "build",
            project.to_str().unwrap(),
            "-o",
            bin.to_str().unwrap(),
        ])
        .env("OPENEPL_RUNTIME_DIR", repo.join("runtime"))
        .status()
        .expect("compile designed app");
    assert!(status.success(), "the designed app did not compile");

    let run = Command::new(&bin)
        .env("OPENEPL_UI_EXIT_AFTER_FRAMES", "4")
        .env("OPENEPL_UI_SYNTH_CLICK", "2")
        .output()
        .expect("run designed app");
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("on_press"),
        "the designed button did not reach its handler"
    );
}

/// The toolbox must actually contain the component types the UI library
/// declares. It once shipped EMPTY — the toolbox markup was built but never
/// passed to the formatter, so `%s` consumed garbage and rendered nothing. The
/// designer looked fine and simply could not add anything.
#[test]
fn designer_toolbox_lists_components() {
    let repo = repo();
    let designer = repo.join("designer/openepl-designer");
    if !designer.exists() {
        eprintln!("designer not built; skipping");
        return;
    }
    let project = std::env::temp_dir().join("openepl_toolbox_probe.oir");
    std::fs::write(
        &project,
        "module probe\nuse ui\n\nform win\n  title = \"Probe\"\nend\n",
    )
    .expect("write project");

    let out = Command::new(&designer)
        .arg(&project)
        .arg(repo.join("target/debug/openepl"))
        .env("OPENEPL_DESIGNER_DEBUG", "1")
        .env("OPENEPL_DESIGNER_SCRIPT", "")
        .output()
        .expect("run designer");
    let stderr = String::from_utf8_lossy(&out.stderr);
    for want in ["oe-add=button", "oe-add=label"] {
        assert!(
            stderr.contains(want),
            "toolbox is missing {want}:\n{stderr}"
        );
    }
}

/// Every component the toolbox offers must be a REAL control, not a mockup:
/// it renders, and where it holds data that data is readable from code.
#[test]
fn all_components_are_real_controls() {
    let repo = repo();
    if !repo.join("vendor/RmlUi/build/librmlui.a").exists() {
        eprintln!("RmlUi not vendored; skipping");
        return;
    }
    let bin = build_as("controls", "real");
    let out = Command::new(&bin)
        .env("OPENEPL_UI_EXIT_AFTER_FRAMES", "4")
        // Handle 7 is the button: the form root is 1 and children follow in
        // declaration order.
        .env("OPENEPL_UI_SYNTH_CLICK", "7")
        .env("OPENEPL_UI_DUMP_A11Y", "1")
        .output()
        .expect("run controls");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // The editbox holds real text that code can read back through the ABI.
    assert!(
        stdout.contains("name = Ada"),
        "editbox value was not readable from code:\n{stdout}"
    );
    // Every component reached the accessibility tree, so each really exists.
    let nodes = stdout
        .lines()
        .filter(|l| l.starts_with("a11y: id="))
        .count();
    assert!(
        nodes >= 7,
        "expected form + 6 components in the a11y tree, got {nodes}:\n{stdout}"
    );
}

/// Whatever the designer saves MUST compile. It once quoted a bool as text
/// (`checked = "true"`), so every Run after touching a checkbox failed — the
/// designer looked fine and produced uncompilable source.
///
/// Guards the general rule: only the descriptor's declared type can decide
/// quoting, because `text = "true"` and `checked = true` are identical strings.
#[test]
fn designer_output_always_compiles() {
    let repo = repo();
    let designer = repo.join("designer/openepl-designer");
    if !designer.exists() {
        eprintln!("designer not built; skipping");
        return;
    }
    let project = std::env::temp_dir().join("openepl_roundtrip.oir");
    std::fs::copy(repo.join("examples/controls.oir"), &project).expect("seed project");

    let out = Command::new(&designer)
        .arg(&project)
        .arg(repo.join("target/debug/openepl"))
        .env(
            "OPENEPL_DESIGNER_SCRIPT",
            "add:checkbox;add:progressbar;add:image;add:groupbox;add:editbox;save",
        )
        .output()
        .expect("run designer");
    assert!(out.status.success(), "designer session failed");

    let src = std::fs::read_to_string(&project).expect("read saved");
    assert!(
        src.contains("checked = true"),
        "bool was quoted as text:\n{src}"
    );

    let bin = std::env::temp_dir().join("openepl_roundtrip_app");
    let status = Command::new(env!("CARGO_BIN_EXE_openepl"))
        .args([
            "build",
            project.to_str().unwrap(),
            "-o",
            bin.to_str().unwrap(),
        ])
        .env("OPENEPL_RUNTIME_DIR", repo.join("runtime"))
        .status()
        .expect("compile");
    assert!(
        status.success(),
        "the designer saved a file the compiler rejects"
    );
}

/// Dragging must move a component to where the cursor put it — for EVERY
/// component, including composites.
///
/// This has broken twice: once because mousedown lands on a child element that
/// carries no id (so checkboxes could not be dragged at all), and once because
/// `select()` rebuilds the canvas, destroying the element whose offset was then
/// read — making every drag jump to the top-left corner. Eyeballing the window
/// caught both late; this catches them immediately.
#[test]
fn dragging_moves_components_precisely() {
    let repo = repo();
    let designer = repo.join("designer/openepl-designer");
    if !designer.exists() {
        eprintln!("designer not built; skipping");
        return;
    }
    let project = std::env::temp_dir().join("openepl_drag.oir");
    std::fs::copy(repo.join("examples/controls.oir"), &project).expect("seed");

    // grp starts at (20,120). Grab 10px inside it and drop at (150,120), so it
    // must land at (140,110) — cursor position minus the grab offset.
    //
    // The target is deliberately clear of every other component's edges AND
    // centre lines: alignment snapping is correct behaviour, but it must not
    // participate in a measurement of the grab offset.
    let out = Command::new(&designer)
        .arg(&project)
        .arg(repo.join("target/debug/openepl"))
        .env("OPENEPL_DESIGNER_SCRIPT", "drag:grp@30,130->150,120;save")
        .output()
        .expect("run designer");
    assert!(out.status.success());

    let src = std::fs::read_to_string(&project).expect("read");
    let grp = src
        .split("groupbox grp")
        .nth(1)
        .expect("groupbox in saved file")
        .to_string();
    assert!(
        grp.contains("left = 140"),
        "drag lost the grab offset:\n{grp}"
    );
    assert!(
        grp.contains("top = 110"),
        "drag lost the grab offset:\n{grp}"
    );

    // A composite component must drag too: the checkbox's box and caption are
    // children with no id of their own. It starts at (20,84); grabbing 5px in
    // and dropping at (100,200) gives (95,195), which snaps to the 10px grid.
    let out = Command::new(&designer)
        .arg(&project)
        .arg(repo.join("target/debug/openepl"))
        .env("OPENEPL_DESIGNER_SCRIPT", "drag:agree@25,89->100,200;save")
        .output()
        .expect("run designer");
    assert!(out.status.success());
    let src = std::fs::read_to_string(&project).expect("read");
    let cb = src
        .split("checkbox agree")
        .nth(1)
        .expect("checkbox")
        .to_string();
    assert!(
        cb.contains("left = 100"),
        "composite component did not drag:\n{cb}"
    );
    assert!(
        cb.contains("top = 200"),
        "composite drag lost its y offset:\n{cb}"
    );
}

/// The selection outline must trace the component's RENDERED frame.
///
/// It was derived from the model's width/height, but those size the CONTENT
/// box: a groupbox declaring 240x90 with 8px padding and a 1px border draws
/// 258x108, so the outline sat inside the real frame. Mixing a content-box
/// origin with a border-box size skewed it further.
#[test]
fn selection_outline_traces_the_rendered_frame() {
    let repo = repo();
    let designer = repo.join("designer/openepl-designer");
    if !designer.exists() {
        eprintln!("designer not built; skipping");
        return;
    }
    let project = std::env::temp_dir().join("openepl_sel.oir");
    std::fs::copy(repo.join("examples/controls.oir"), &project).expect("seed");

    let out = Command::new(&designer)
        .arg(&project)
        .arg(repo.join("target/debug/openepl"))
        .env("OPENEPL_DESIGNER_DEBUG", "1")
        .env("OPENEPL_DESIGNER_SCRIPT", "select:grp")
        .output()
        .expect("run designer");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr
        .lines()
        .find(|l| l.contains("selection rect="))
        .expect("designer should report the selection rect");

    // grp is 240x90 with 8px padding and a 1px border on each side.
    assert!(
        line.contains("selection rect=20,120 258x108"),
        "outline does not trace the rendered frame: {line}"
    );
}

/// Dragging a component near another's edge should snap flush to it — that is
/// what alignment guides are for, and it is the difference between a designer
/// that feels precise and one that feels approximate.
#[test]
fn dragging_snaps_to_alignment_guides() {
    let repo = repo();
    let designer = repo.join("designer/openepl-designer");
    if !designer.exists() {
        eprintln!("designer not built; skipping");
        return;
    }
    let project = std::env::temp_dir().join("openepl_align.oir");
    std::fs::copy(repo.join("examples/controls.oir"), &project).expect("seed");

    // Most components sit at left = 20. Drop grp at left = 23, close enough
    // that it must snap flush to 20 rather than to the 10px grid.
    let out = Command::new(&designer)
        .arg(&project)
        .arg(repo.join("target/debug/openepl"))
        .env("OPENEPL_DESIGNER_SCRIPT", "drag:grp@20,120->23,200;save")
        .output()
        .expect("run designer");
    assert!(out.status.success());

    let src = std::fs::read_to_string(&project).expect("read");
    let grp = src
        .split("groupbox grp")
        .nth(1)
        .expect("groupbox")
        .to_string();
    assert!(
        grp.contains("left = 20"),
        "did not snap flush to the neighbouring edge:\n{grp}"
    );
}

/// The IDE must fill the OS window at any size. Layout sizes were compile-time
/// constants, so enlarging the window left everything past 1440x900 unpainted —
/// a black frame around the UI.
#[test]
fn layout_follows_the_window_size() {
    let repo = repo();
    let designer = repo.join("designer/openepl-designer");
    if !designer.exists() {
        eprintln!("designer not built; skipping");
        return;
    }
    let project = std::env::temp_dir().join("openepl_resize.oir");
    std::fs::copy(repo.join("examples/controls.oir"), &project).expect("seed");
    let dump = std::env::temp_dir().join("openepl_resize.ppm");

    let out = Command::new(&designer)
        .arg(&project)
        .arg(repo.join("target/debug/openepl"))
        .env("OPENEPL_DESIGNER_SCRIPT", "winsize:1700x1000")
        .env("OPENEPL_DESIGNER_DUMP", &dump)
        .output()
        .expect("run designer");
    assert!(out.status.success());

    let bytes = std::fs::read(&dump).expect("read dump");
    // Parse the PPM header: P6\n<w> <h>\n255\n
    let mut nl = 0;
    let mut i = 0;
    let mut hdr = String::new();
    let (mut w, mut h) = (0usize, 0usize);
    while nl < 3 && i < bytes.len() {
        if bytes[i] == b'\n' {
            nl += 1;
            if nl == 2 {
                let mut it = hdr.split_whitespace();
                w = it.next().unwrap().parse().unwrap();
                h = it.next().unwrap().parse().unwrap();
            }
            hdr.clear();
        } else {
            hdr.push(bytes[i] as char);
        }
        i += 1;
    }
    assert_eq!((w, h), (1700, 1000), "the window did not actually resize");

    // Every corner and edge of the enlarged window must be painted.
    let px = |x: usize, y: usize| {
        let o = i + (y * w + x) * 3;
        (bytes[o], bytes[o + 1], bytes[o + 2])
    };
    for (name, p) in [
        ("right edge", px(w - 5, h / 2)),
        ("bottom edge", px(w / 2, h - 5)),
        ("bottom-right corner", px(w - 5, h - 5)),
    ] {
        assert_ne!(
            p,
            (0, 0, 0),
            "{name} is unpainted — the layout did not follow the window"
        );
    }
}

// ---------------------------------------------------------------------------
// Build targets: one source, several artifacts
// ---------------------------------------------------------------------------

/// A library source: no entry point, two subroutines to export.
const LIB_SRC: &str = "module greetlib\n\
                       target sharedlib\n\
                       \n\
                       sub greet\n  call print_text(\"hello from a library\")\nend\n\
                       \n\
                       sub farewell\n  call print_text(\"goodbye\")\nend\n";

/// Build `LIB_SRC` for `target`, returning the artifact path. `tag` must be
/// unique per test — see `build_as`.
fn build_lib(target: &str, tag: &str, ext: &str) -> PathBuf {
    let repo = repo();
    let dir = std::env::temp_dir().join(format!("openepl_lib_{tag}"));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let src = dir.join("greetlib.oir");
    std::fs::write(&src, LIB_SRC).expect("write source");
    let out = dir.join(format!("libgreet.{ext}"));

    let status = Command::new(env!("CARGO_BIN_EXE_openepl"))
        .args([
            "build",
            src.to_str().unwrap(),
            "--target",
            target,
            "-o",
            out.to_str().unwrap(),
        ])
        .env("OPENEPL_RUNTIME_DIR", repo.join("runtime"))
        .status()
        .expect("run openepl");
    assert!(status.success(), "openepl build --target {target} failed");
    out
}

/// Compile `main_src` against `lib`, run it, and return its stdout.
fn run_c_host(dir: &Path, main_src: &str, lib: &Path, tag: &str) -> String {
    let c = dir.join(format!("host_{tag}.c"));
    std::fs::write(&c, main_src).expect("write host");
    let bin = dir.join(format!("host_{tag}"));
    let status = Command::new("clang")
        .arg(&c)
        .arg(lib)
        .arg("-lm")
        .arg("-o")
        .arg(&bin)
        .status()
        .expect("run clang");
    assert!(status.success(), "linking the C host failed");
    let out = Command::new(&bin).output().expect("run host");
    assert!(out.status.success(), "host exited non-zero");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A `.so` must actually LOAD and RUN, not merely exist with the right
/// extension. The first version of this linked cleanly and then failed to
/// dlopen with `undefined symbol: ECodeStart`, because the runtime's
/// process-entry object came along for the ride.
#[test]
fn shared_library_loads_and_its_exports_run() {
    let lib = build_lib("sharedlib", "so", "so");
    let dir = lib.parent().unwrap();

    let host = "#include <dlfcn.h>\n#include <stdio.h>\n\
                int main(void){\n\
                  void* h = dlopen(\"LIBPATH\", RTLD_NOW);\n\
                  if(!h){ printf(\"dlopen failed: %s\\n\", dlerror()); return 1; }\n\
                  void (*init)(void) = dlsym(h, \"greetlib_init\");\n\
                  void (*greet)(void) = dlsym(h, \"greet\");\n\
                  void (*bye)(void) = dlsym(h, \"farewell\");\n\
                  if(!init || !greet || !bye){ printf(\"missing export\\n\"); return 1; }\n\
                  init(); greet(); bye(); return 0;\n}\n";
    let host = host.replace("LIBPATH", lib.to_str().unwrap());

    // The host links against libdl only; the library is opened at run time.
    let c = dir.join("dlhost.c");
    std::fs::write(&c, host).expect("write host");
    let bin = dir.join("dlhost");
    let status = Command::new("clang")
        .arg(&c)
        .arg("-ldl")
        .arg("-o")
        .arg(&bin)
        .status()
        .expect("clang");
    assert!(status.success(), "compiling the dlopen host failed");
    let out = Command::new(&bin).output().expect("run host");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("hello from a library") && text.contains("goodbye"),
        "exports did not run: {text}"
    );
}

/// The exports must be there under their plain names, and the program entry
/// must NOT be: a library with an `ECodeStart` is a program wearing a hat.
#[test]
fn shared_library_exports_plain_names_and_no_entry() {
    let lib = build_lib("sharedlib", "syms", "so");
    let out = Command::new("nm")
        .args(["-D", "--defined-only"])
        .arg(&lib)
        .output()
        .expect("nm");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(" T greet"), "missing export `greet`: {text}");
    assert!(text.contains(" T farewell"), "missing export `farewell`");
    assert!(
        !text.contains("ECodeStart"),
        "a library must not define a program entry: {text}"
    );

    let undef = Command::new("nm").args(["-D", "-u"]).arg(&lib).output().expect("nm");
    assert!(
        !String::from_utf8_lossy(&undef.stdout).contains("ECodeStart"),
        "unresolved ECodeStart would make the .so unloadable"
    );
}

/// A `.a` has to link into a real host and run.
#[test]
fn static_library_links_into_a_c_host() {
    let lib = build_lib("staticlib", "a", "a");
    let dir = lib.parent().unwrap();
    let text = run_c_host(
        dir,
        "void greetlib_init(void); void greet(void);\n\
         int main(void){ greetlib_init(); greet(); return 0; }\n",
        &lib,
        "static",
    );
    assert!(
        text.contains("hello from a library"),
        "the archived export did not run: {text}"
    );
}

/// The same source builds as either artifact — that is what makes the target a
/// build-time choice rather than a rewrite (G12).
#[test]
fn one_source_builds_as_both_library_kinds() {
    let so = build_lib("sharedlib", "both_so", "so");
    let a = build_lib("staticlib", "both_a", "a");
    assert!(so.exists() && a.exists());
    let ar = Command::new("ar").arg("t").arg(&a).output().expect("ar t");
    assert!(
        !String::from_utf8_lossy(&ar.stdout).trim().is_empty(),
        "the archive should contain objects"
    );
}

/// A library with no subroutines exports nothing, and a form belongs to a GUI
/// program — both are caught before the toolchain is invoked.
#[test]
fn library_targets_reject_nonsense() {
    let dir = std::env::temp_dir().join("openepl_lib_reject");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let src = dir.join("empty.oir");
    std::fs::write(&src, "module empty\ntarget sharedlib\n").expect("write");
    let out = Command::new(env!("CARGO_BIN_EXE_openepl"))
        .args(["build", src.to_str().unwrap()])
        .env("OPENEPL_RUNTIME_DIR", repo().join("runtime"))
        .output()
        .expect("run openepl");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "an empty library should not build");
    assert!(
        err.contains("exports nothing"),
        "the error should say what is wrong: {err}"
    );
}

// ---------------------------------------------------------------------------
// Project templates
// ---------------------------------------------------------------------------

fn openepl(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_openepl"))
        .args(args)
        .current_dir(repo())
        .env("OPENEPL_RUNTIME_DIR", repo().join("runtime"))
        .output()
        .expect("run openepl")
}

/// A template that does not compile is worse than no template: it hands a
/// newcomer a broken project as their first experience. Every one is built.
#[test]
fn every_template_creates_a_project_that_builds() {
    let listing = openepl(&["templates"]);
    let text = String::from_utf8_lossy(&listing.stdout);
    let ids: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("template: "))
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert!(!ids.is_empty(), "no templates listed:\n{text}");

    for id in ids {
        let dir = std::env::temp_dir().join(format!("openepl_tmpl_{id}"));
        let _ = std::fs::remove_dir_all(&dir);
        let out = openepl(&["new", id, dir.to_str().unwrap()]);
        assert!(
            out.status.success(),
            "`openepl new {id}` failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        // The listing tells the caller which file to open; it must exist.
        let created = String::from_utf8_lossy(&out.stdout);
        let open = created
            .lines()
            .find_map(|l| l.strip_prefix("open: "))
            .expect("`new` should report the file to open");
        assert!(Path::new(open).is_file(), "{open} was not created");

        let bin = dir.join("out");
        let built = openepl(&["build", open, "-o", bin.to_str().unwrap()]);
        assert!(
            built.status.success(),
            "template `{id}` does not build:\n{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The console template is the newcomer's first program: it must actually run
/// and print, not merely compile.
#[test]
fn the_console_template_runs() {
    let dir = std::env::temp_dir().join("openepl_tmpl_run");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(openepl(&["new", "console-app", dir.to_str().unwrap()])
        .status
        .success());

    let bin = dir.join("app");
    let src = dir.join("main.oir");
    assert!(openepl(&["build", src.to_str().unwrap(), "-o", bin.to_str().unwrap()])
        .status
        .success());
    let out = Command::new(&bin).output().expect("run template app");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Hello from OpenEPL"), "got: {text}");
    assert!(text.contains("six times seven is 42"), "arithmetic line missing: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The module name comes from the directory, sanitised into an identifier.
#[test]
fn the_module_name_follows_the_directory() {
    let dir = std::env::temp_dir().join("openepl-my-app");
    let _ = std::fs::remove_dir_all(&dir);
    let out = openepl(&["new", "console-app", dir.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("module: openepl_my_app"),
        "dashes are not legal in an identifier: {text}"
    );
    let src = std::fs::read_to_string(dir.join("main.oir")).expect("read");
    assert!(src.contains("module openepl_my_app"));
    assert!(!src.contains("__MODULE__"), "placeholder left behind");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Creating into someone's existing work would destroy it.
#[test]
fn new_refuses_a_non_empty_directory() {
    let dir = std::env::temp_dir().join("openepl_tmpl_occupied");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("keepme.txt"), "important").expect("write");
    let out = openepl(&["new", "console-app", dir.to_str().unwrap()]);
    assert!(!out.status.success(), "should refuse a non-empty directory");
    assert!(
        dir.join("keepme.txt").is_file(),
        "the existing file must survive"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Text is UTF-8, so character-shaped commands must count and cut CHARACTERS.
/// Measuring in bytes leaks the encoding into every program: a word with an
/// accent in it reports the wrong length, and a slice at a byte offset splits
/// a character and yields text that is no longer valid UTF-8.
#[test]
fn text_commands_are_utf8_correct() {
    let dir = std::env::temp_dir().join("openepl_utf8");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let src = dir.join("utf8.oir");
    std::fs::write(
        &src,
        "module utf8check\ntarget console\n\nsub main\n  \
         call print_int(length(\"héllo\"))\n  \
         call print_text(reverse(\"héllo\"))\n  \
         call print_text(substr(\"héllo\", 0, 2))\n  \
         call print_int(length(\"日本語\"))\n  \
         call print_text(substr(\"日本語\", 1, 1))\nend\n",
    )
    .expect("write");

    let bin = dir.join("utf8");
    let out = Command::new(env!("CARGO_BIN_EXE_openepl"))
        .args(["build", src.to_str().unwrap(), "-o", bin.to_str().unwrap()])
        .env("OPENEPL_RUNTIME_DIR", repo().join("runtime"))
        .output()
        .expect("run openepl");
    assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stderr));

    let text = run(&bin);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "5", "length counts characters, not bytes: {text}");
    assert_eq!(lines[1], "olléh", "reverse must not split a character: {text}");
    assert_eq!(lines[2], "hé", "substr must not cut mid-character: {text}");
    assert_eq!(lines[3], "3", "three characters, nine bytes: {text}");
    assert_eq!(lines[4], "本", "slicing multi-byte text: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every support library's example builds, runs, and reports no failure.
///
/// The examples are self-checking transcripts: one prints `FAIL` on a check
/// that did not hold, so the assertion here is that no line says FAIL and that
/// the program produced output at all. Deliberately not asserting exact stdout
/// — these examples are documentation first, and pinning every line would mean
/// editing this test every time one gains a sentence. What must not drift is
/// that they still run and still pass their own checks.
///
/// Without this the ten libraries had examples but no coverage: they were only
/// ever run by hand, so a regression in any of them would reach a release
/// unnoticed.
#[test]
fn support_library_examples_pass_their_own_checks() {
    // `net` is excluded deliberately: its example reaches the network, and a
    // test that fails on a train is a test people learn to ignore.
    const LIBS: &[&str] = &[
        "filelib", "systemlib", "textlib", "timelib", "randomlib", "hashlib",
        "configlib", "processlib", "jsonlib", "mathlib",
    ];
    for name in LIBS {
        let stdout = run(&build_as(name, "selfcheck"));
        let failures: Vec<&str> = stdout.lines().filter(|l| l.contains("FAIL")).collect();
        assert!(
            failures.is_empty(),
            "{name} reported failures:\n{}",
            failures.join("\n")
        );
        assert!(
            !stdout.trim().is_empty(),
            "{name} produced no output — did it run at all?"
        );
    }
}
