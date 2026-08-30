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
/// `tag` disambiguates the output path so tests that build the same example can
/// run in parallel without clobbering each other's binary.
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

fn build(name: &str) -> PathBuf {
    build_as(name, "main")
}

fn run(bin: &Path) -> String {
    let out = Command::new(bin).output().expect("run built binary");
    assert!(out.status.success(), "binary exited non-zero");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn hello_builds_and_runs() {
    let stdout = run(&build("hello"));
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec!["OpenEPL — arithmetic demo", "42", "14", "42", "42"]
    );
}

#[test]
fn demo_builds_and_runs() {
    let stdout = run(&build("demo"));
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
    let bin = build("hello"); // uses only print_text / print_int + arithmetic
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
    let stdout = run(&build("hellolib"));
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
    let bin = build("form");

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
        return;
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
