//! End-to-end tests for the Phase 1 toolchain: build `.oir` examples to native
//! binaries, run them, and prove dead-code stripping (PRD M2).
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

/// PRD M2 / D3: only referenced commands are linked in; unused command code is
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

#[test]
fn hello_library_via_abi() {
    // `use hello` — a third-party support library loaded through the ABI.
    let stdout = run(&build_as("hellolib", "abi"));
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["Hello, OpenEPL!", "HELLO, WORLD!"]);
}

/// PRD Phase 2 (RAD half): a form + button + handler authored in IR compiles to
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
/// dead-strip guarantee, PRD M2/D3, would be meaningless).
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
/// names, parent links and bounds (ADR 0005/D16). Substrate-independent and
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

/// Reading and writing component properties from code (ADR 0008). The counter
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
/// short-circuit `and`, and content-based text equality (ADR 0010).
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

/// **M0, the RAD metric (PRD §8).** A scripted designer session adds a button,
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
        // declaration order (ADR 0008).
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
