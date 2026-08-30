//! Golden end-to-end test (PRD M0/M1 in miniature): build `examples/hello.oir`
//! to a native binary, run it, and assert the arithmetic output.
use std::path::PathBuf;
use std::process::Command;

#[test]
fn hello_builds_and_runs() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    let example = repo.join("examples/hello.oir");
    let out_bin = std::env::temp_dir().join("openepl_hello_test");

    let status = Command::new(env!("CARGO_BIN_EXE_openepl"))
        .args(["build", example.to_str().unwrap(), "-o", out_bin.to_str().unwrap()])
        .env("OPENEPL_RUNTIME_DIR", repo.join("runtime"))
        .status()
        .expect("run openepl");
    assert!(status.success(), "openepl build failed");

    let out = Command::new(&out_bin).output().expect("run built binary");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec!["OpenEPL — arithmetic demo", "42", "14", "42", "42"],
        "unexpected program output:\n{stdout}"
    );
}
