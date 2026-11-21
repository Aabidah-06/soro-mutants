//! `--no-progress` contract (issue #135): text output keeps only the final
//! summary, machine-readable output is unaffected, and exit codes are unchanged.

use std::{
    fs,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

/// Runs `test` against a private project so parallel tests never share
/// build-cache or scratch state.
fn test_cmd(args: &[&str]) -> Output {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "soro-noprogress-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "pub fn guarded(user: Address) { user.require_auth(); }\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-soro-mutants"))
        .args(["test", root.to_str().unwrap(), "--operator", "AUTH-001"])
        .args(["--test-command", "true"])
        .args(args)
        .output()
        .expect("CLI should run");
    let _ = fs::remove_dir_all(&root);
    output
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn default_text_output_still_prints_progress() {
    let output = test_cmd(&[]);
    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    assert!(text.contains("Baseline: true"), "{text}");
    assert!(text.contains("Baseline PASS"), "{text}");
    assert!(text.contains("SURVIVED"), "{text}");
}

#[test]
fn no_progress_keeps_only_the_summary() {
    let output = test_cmd(&["--no-progress"]);
    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    assert!(!text.contains("Baseline"), "{text}");
    assert!(!text.contains("SURVIVED   AUTH-001"), "{text}");
    assert!(text.contains("Summary"), "{text}");
    assert!(text.contains("Generated : 1"), "{text}");
    assert!(text.contains("Survived  : 1"), "{text}");
}

#[test]
fn no_progress_does_not_change_exit_codes() {
    let failed_threshold = test_cmd(&["--no-progress", "--min-score", "100"]);
    assert_eq!(failed_threshold.status.code(), Some(4));
    assert!(stdout(&failed_threshold).contains("Summary"));
    assert!(String::from_utf8_lossy(&failed_threshold.stderr).contains("error: "));
}

#[test]
fn no_progress_is_a_no_op_for_json_output() {
    let plain = test_cmd(&["--json"]);
    let quiet = test_cmd(&["--json", "--no-progress"]);
    assert_eq!(plain.status.code(), Some(0));
    assert_eq!(quiet.status.code(), Some(0));
    let mut plain: serde_json::Value = serde_json::from_slice(&plain.stdout).unwrap();
    let mut quiet: serde_json::Value = serde_json::from_slice(&quiet.stdout).unwrap();
    // Run metadata records timing, which legitimately differs between runs.
    plain.as_object_mut().unwrap().remove("metadata");
    quiet.as_object_mut().unwrap().remove("metadata");
    assert_eq!(plain, quiet);
}

#[test]
fn no_progress_keeps_stderr_diagnostics_and_stdout_separation() {
    let output = test_cmd(&["--no-progress", "--no-cache"]);
    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("note: persistent result cache disabled"),
        "{stderr}"
    );
    assert!(!stdout(&output).contains("note:"));
}

#[test]
fn help_documents_no_progress() {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-soro-mutants"))
        .args(["test", "--help"])
        .output()
        .unwrap();
    assert!(stdout(&output).contains("--no-progress"));
}

#[test]
fn list_rejects_no_progress_with_usage_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-soro-mutants"))
        .args(["list", "fixtures/weak-auth", "--no-progress"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}
