//! The lines the designer reads: `openepl inspect` and `openepl commands`.
//!
//! Studio never parses `.oir` itself, so these two listings are its entire
//! knowledge of a file and of a library. A line kind that changes shape, or a
//! value that spills onto a second line, is not a formatting bug there — it is
//! a save that writes something else back.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openepl_inspect_{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch");
    dir
}

fn openepl(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_openepl"))
        .args(args)
        .current_dir(cwd)
        .env("OPENEPL_RUNTIME_DIR", repo().join("runtime"))
        .output()
        .expect("run openepl");
    assert!(
        out.status.success(),
        "openepl {args:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn has_line(out: &str, line: &str) -> bool {
    out.lines().any(|l| l == line)
}

const SOURCE: &str = "module rt
use ui

form win
  title = \"Round trip\"

  memo notes
    text = \"first\\nsecond \\\\ back\"
  end
end

sub on_tick
  if 1 > 2
    call print_text(\"never\")
  end
end

record point
  x: int
end

timer ticker
  interval = 500
  on tick: on_tick
end
";

#[test]
fn module_components_are_a_distinct_line_kind_with_a_span() {
    let dir = scratch("modcomponent");
    std::fs::write(dir.join("rt.oir"), SOURCE).unwrap();
    let out = openepl(&dir, &["inspect", "rt.oir"]);

    // `record point` is the same two tokens as a component header, and the
    // sub above it holds a nested `end`; neither may shift the span.
    assert!(
        has_line(&out, "modcomponent: ticker timer span=22..25"),
        "{out}"
    );
    assert!(has_line(&out, "prop: ticker interval 500"), "{out}");
    assert!(has_line(&out, "handler: ticker tick on_tick"), "{out}");
    // Never as a form child: the designer would write it back inside the form.
    assert!(!has_line(&out, "component: ticker timer"), "{out}");
    assert!(has_line(&out, "component: notes memo"), "{out}");
    assert!(has_line(&out, "form: win span=4..10"), "{out}");
}

#[test]
fn a_property_value_stays_on_one_line() {
    let dir = scratch("escape");
    std::fs::write(dir.join("rt.oir"), SOURCE).unwrap();
    let out = openepl(&dir, &["inspect", "rt.oir"]);

    assert!(
        has_line(&out, "prop: notes text first\\nsecond \\\\ back"),
        "{out}"
    );
    // Every line announces its kind — nothing is left for a reader to guess
    // is a continuation of the value above it.
    for l in out.lines() {
        let kind = l.split(": ").next().unwrap_or("");
        assert!(
            matches!(
                kind,
                "module"
                    | "use"
                    | "sub"
                    | "subsig"
                    | "form"
                    | "component"
                    | "modcomponent"
                    | "prop"
                    | "handler"
            ),
            "unlabelled line {l:?} in:\n{out}"
        );
    }
}

#[test]
fn commands_reports_kind_and_editor() {
    let dir = scratch("commands");
    let core = openepl(&dir, &["commands"]);
    assert!(has_line(&core, "component: timer"), "{core}");
    assert!(has_line(&core, "kind: timer nonvisual"), "{core}");

    let ui = openepl(&dir, &["commands", "--use", "ui"]);
    assert!(has_line(&ui, "kind: button visual"), "{ui}");
    assert!(has_line(&ui, "editor: memo text multiline"), "{ui}");
    // The lines a reader already parses are untouched: an `editor:` line is
    // added beside a `property:` line, never folded into it.
    assert!(has_line(&ui, "property: memo text text"), "{ui}");
    assert!(has_line(&ui, "event: button click"), "{ui}");
    for l in ui.lines() {
        let kind = l.split(": ").next().unwrap_or("");
        assert!(
            matches!(
                kind,
                "command" | "component" | "kind" | "property" | "editor" | "event"
            ),
            "unexpected line {l:?}"
        );
    }
}
