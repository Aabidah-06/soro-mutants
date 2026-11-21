//! Empty-selection diagnostics (issue #134): a valid empty selection is
//! explained on stderr, stdout contracts are unchanged, and discovery problems
//! are distinguishable from a legitimately empty result.

use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-soro-mutants"))
        .args(args)
        .output()
        .expect("CLI should run")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn project(name: &str, source: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("soro-empty-{name}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), source).unwrap();
    root
}

#[test]
fn non_empty_selection_emits_no_diagnostics() {
    let output = cli(&["list", "fixtures/weak-auth", "--operator", "AUTH-001"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stderr(&output), "");
}

#[test]
fn filters_that_match_nothing_are_listed() {
    let output = cli(&[
        "list",
        "fixtures/weak-auth",
        "--operator",
        "TTL-001",
        "--file",
        "src/lib.rs",
        "--exclude-path",
        "target/**",
    ]);
    assert_eq!(output.status.code(), Some(0));
    let err = stderr(&output);
    assert!(
        err.contains("note: ") && err.contains("none matched the discovery filters"),
        "{err}"
    );
    assert!(
        err.contains(
            "active selection filters: --operator TTL-001, --file src/lib.rs, --exclude-path target/**"
        ),
        "{err}"
    );
    assert!(!err.contains("warning:"), "{err}");
}

#[test]
fn stdout_contracts_are_unchanged_for_empty_selections() {
    let count = cli(&[
        "list",
        "fixtures/weak-auth",
        "--operator",
        "TTL-001",
        "--count-only",
    ]);
    assert_eq!(String::from_utf8_lossy(&count.stdout), "0\n");
    assert!(stderr(&count).contains("active selection filters: --operator TTL-001"));

    let json = cli(&[
        "list",
        "fixtures/weak-auth",
        "--operator",
        "TTL-001",
        "--json",
    ]);
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(value["mutants"].as_array().unwrap().len(), 0);
    assert!(stderr(&json).contains("none matched the discovery filters"));

    let test = cli(&["test", "fixtures/weak-auth", "--operator", "TTL-001"]);
    assert_eq!(test.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&test.stdout)
        .contains("No matching Soroban semantic mutants found."));
    assert!(stderr(&test).contains("none matched the discovery filters"));
}

#[test]
fn shard_with_no_mutants_names_the_shard() {
    let output = cli(&[
        "list",
        "fixtures/weak-auth",
        "--operator",
        "AUTH-001",
        "--shard",
        "2/2",
    ]);
    assert_eq!(output.status.code(), Some(0));
    let err = stderr(&output);
    assert!(
        err.contains("1 mutant(s) were selected before sharding but --shard 2/2 contains none"),
        "{err}"
    );
}

#[test]
fn source_without_mutation_sites_is_a_valid_empty_result() {
    let root = project("valid", "pub fn add(a: u32, b: u32) -> u32 { a + b }\n");
    let output = cli(&["list", root.to_str().unwrap(), "--count-only"]);
    let _ = fs::remove_dir_all(&root);
    assert_eq!(output.status.code(), Some(0));
    let err = stderr(&output);
    assert!(
        err.contains("valid empty result, not a discovery failure"),
        "{err}"
    );
    assert!(err.contains("no selection filters were active"), "{err}");
    assert!(!err.contains("warning:"), "{err}");
}

#[test]
fn unparsable_sources_are_a_warning_not_a_valid_empty_result() {
    let root = project("broken", "pub fn broken( {\n");
    let output = cli(&["test", root.to_str().unwrap(), "--json"]);
    let _ = fs::remove_dir_all(&root);
    assert_eq!(output.status.code(), Some(0));
    let err = stderr(&output);
    assert!(
        err.contains(
            "warning: 1 Rust source file(s) could not be parsed and were skipped: src/lib.rs"
        ),
        "{err}"
    );
    assert!(!err.contains("valid empty result"), "{err}");
}

#[test]
fn directory_without_rust_sources_is_a_warning() {
    let root = std::env::temp_dir().join(format!("soro-empty-norust-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let output = cli(&["list", root.to_str().unwrap()]);
    let _ = fs::remove_dir_all(&root);
    assert_eq!(output.status.code(), Some(0));
    assert!(stderr(&output).contains("warning: no Rust source files were found under"));
}

#[test]
fn discovery_failure_is_an_error_with_exit_one() {
    let output = cli(&["list", "/this-directory-does-not-exist-soro-mutants"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).starts_with("error: cannot resolve "));
    assert!(output.stdout.is_empty());
}

#[test]
fn positive_min_score_on_empty_selection_still_exits_four() {
    let output = cli(&[
        "test",
        "fixtures/weak-auth",
        "--operator",
        "TTL-001",
        "--min-score",
        "1",
    ]);
    assert_eq!(output.status.code(), Some(4));
    let err = stderr(&output);
    assert!(err.contains("none matched the discovery filters"), "{err}");
    assert!(err.contains("error: semantic score"), "{err}");
}

#[test]
fn changed_selection_that_matches_nothing_is_explained() {
    let output = cli(&[
        "list",
        "fixtures/weak-auth",
        "--changed-file",
        "src/unrelated.rs",
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert!(
        stderr(&output)
            .contains("none are in the changed-code selection: --changed-file src/unrelated.rs"),
        "{}",
        stderr(&output)
    );
}
