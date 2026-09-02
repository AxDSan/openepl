//! The project file — `project.oeproj` — and `openepl project`.
//!
//! A project is a directory with a `project.oeproj` in it, and the file is the
//! same `key: value` lines as `template.meta`: the CLI is the only reader of a
//! project file, so Studio learns a project's shape through `openepl project`
//! rather than by parsing this itself, exactly as it learns a form's shape
//! through `openepl inspect`. A second parser in C++ would drift from this one
//! the first time a field was added.
//!
//! Fields:
//!
//!   name:    what the project is called; also the default output name
//!   main:    the entry `.oir`, relative to the project file
//!   target:  console | gui | sharedlib | staticlib
//!   kits:    the `use` set, so a fresh checkout knows what to `kit add`
//!   version: the PROJECT's version — not the toolchain's, which is what
//!            `openepl version` reports. `new` writes `0.1.0`.
//!
//! `build`, `run`, `emit` and `inspect` accept the file or its directory in
//! place of an `.oir`, and resolve `main` from it.

use std::path::{Path, PathBuf};

use openepl_ir::Target;

pub const FILE_NAME: &str = "project.oeproj";

#[derive(Debug)]
pub struct Project {
    /// The project file itself, as given (not canonicalised — a message that
    /// names a path should name the one the person typed).
    pub file: PathBuf,
    pub name: String,
    /// Resolved: the project's directory joined with the `main:` line, so a
    /// caller standing anywhere opens the right file.
    pub main: PathBuf,
    pub target: Option<Target>,
    pub kits: Vec<String>,
    pub version: String,
}

/// Is this argument a project rather than a source file?
///
/// A directory always is: there is nothing else `openepl build <dir>` could
/// mean. A file is one by extension only, so a source file named oddly is
/// still handed to the parser.
pub fn is_project_path(p: &Path) -> bool {
    p.is_dir() || p.extension().and_then(|e| e.to_str()) == Some("oeproj")
}

/// The project file for a path that names a project or its directory.
pub fn locate(p: &Path) -> Result<PathBuf, String> {
    if p.is_dir() {
        let file = p.join(FILE_NAME);
        if file.is_file() {
            Ok(file)
        } else {
            Err(format!(
                "{} has no {FILE_NAME} — run `openepl new` to create a project, or name the .oir directly",
                p.display()
            ))
        }
    } else if p.is_file() {
        Ok(p.to_path_buf())
    } else {
        Err(format!("cannot read {}: no such file or directory", p.display()))
    }
}

pub fn load(path: &Path) -> Result<Project, String> {
    let file = locate(path)?;
    let text = std::fs::read_to_string(&file)
        .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    let dir = file.parent().unwrap_or(Path::new(".")).to_path_buf();

    let mut name = String::new();
    let mut main = String::new();
    let mut target = None;
    let mut kits = Vec::new();
    let mut version = String::new();
    for line in text.lines() {
        // Comments and blank lines, so a person can annotate the file by hand.
        if line.trim_start().starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "name" => name = value.to_string(),
            "main" => main = value.to_string(),
            "target" => {
                target = Some(Target::parse(value).ok_or_else(|| {
                    format!(
                        "{}: unknown target `{value}` — expected console, gui, sharedlib or staticlib",
                        file.display()
                    )
                })?)
            }
            // Either shape reads the same, so a file written by hand with
            // commas is not wrong.
            "kits" => kits.extend(
                value
                    .split(|c: char| c == ',' || c.is_whitespace())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
            ),
            "version" => version = value.to_string(),
            _ => {}
        }
    }
    // The entry is the one field with no sensible default: a project that
    // does not say what to build cannot be built.
    if main.is_empty() {
        return Err(format!("{}: no `main:` line", file.display()));
    }
    if name.is_empty() {
        name = dir
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("app")
            .to_string();
    }
    Ok(Project {
        main: dir.join(&main),
        file,
        name,
        target,
        kits,
        version,
    })
}

/// The file's text, in the shape `load` reads.
pub fn render(name: &str, main: &str, target: Target, kits: &[String], version: &str) -> String {
    format!(
        "name: {name}\nmain: {main}\ntarget: {}\nkits: {}\nversion: {version}\n",
        target.as_str(),
        kits.join(" ")
    )
}

/// `openepl project <file-or-dir>` — the resolved fields, one per line.
///
/// `main:` is printed resolved rather than as written, because the reader is
/// Studio deciding what to open, and Studio is not standing in the project's
/// directory. `kit:` is one line per kit, like `use:` in `inspect`, so a
/// reader never splits a value.
pub fn cmd_project(args: &[String]) -> i32 {
    let Some(path) = args.first() else {
        eprintln!("openepl: usage: openepl project <project.oeproj | directory>");
        return 2;
    };
    if args.len() > 1 {
        eprintln!("openepl: unexpected argument `{}`", args[1]);
        return 2;
    }
    match load(Path::new(path)) {
        Ok(p) => {
            println!("project: {}", p.file.display());
            println!("name: {}", p.name);
            println!("main: {}", p.main.display());
            if let Some(t) = p.target {
                println!("target: {}", t.as_str());
            }
            for k in &p.kits {
                println!("kit: {k}");
            }
            if !p.version.is_empty() {
                println!("version: {}", p.version);
            }
            0
        }
        Err(e) => {
            eprintln!("openepl: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_and_load_agree() {
        let dir = std::env::temp_dir().join("openepl_project_unit");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let text = render("demo", "src/main.oir", Target::Gui, &["ui".into(), "file".into()], "0.1.0");
        std::fs::write(dir.join(FILE_NAME), text).unwrap();

        let p = load(&dir).unwrap();
        assert_eq!(p.name, "demo");
        assert_eq!(p.main, dir.join("src/main.oir"));
        assert_eq!(p.target, Some(Target::Gui));
        assert_eq!(p.kits, vec!["ui", "file"]);
        assert_eq!(p.version, "0.1.0");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_directory_without_a_project_says_so() {
        let dir = std::env::temp_dir().join("openepl_project_unit_empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let err = load(&dir).unwrap_err();
        assert!(err.contains(FILE_NAME), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
