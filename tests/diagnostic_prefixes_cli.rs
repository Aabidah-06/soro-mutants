//! Diagnostic prefix and capitalization conventions (issue #136).
//!
//! Fatal errors use `error: `, non-fatal problems use `warning: `, and neutral
//! context uses `note: `. Message bodies start lowercase (flags, identifiers,
//! and paths excepted) and end without a period.

use std::{
    fs,
    path::Path,
    process::{Command, Output},
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

#[test]
fn fatal_errors_use_the_lowercase_error_prefix() {
    let output = cli(&["list", "/this-directory-does-not-exist-soro-mutants"]);
    assert_eq!(output.status.code(), Some(1));
    let err = stderr(&output);
    assert!(err.starts_with("error: cannot resolve "), "{err}");
    assert!(!err.contains("Error:"), "{err}");
    assert!(output.stdout.is_empty(), "fatal errors must not use stdout");
}

#[test]
fn every_stderr_line_has_a_known_prefix_for_runtime_errors() {
    let output = cli(&["list", "fixtures/workspace-multi-package", "-p", "missing"]);
    assert_eq!(output.status.code(), Some(1));
    let err = stderr(&output);
    assert_eq!(
        err.trim_end(),
        "error: package `missing` not found in workspace"
    );
}

#[test]
fn baseline_and_threshold_failures_keep_their_exit_codes_and_prefix() {
    let baseline = cli(&[
        "test",
        "fixtures/weak-auth",
        "--operator",
        "AUTH-001",
        "--test-command",
        "false",
    ]);
    assert_eq!(baseline.status.code(), Some(3));
    assert!(stderr(&baseline).starts_with("error: baseline tests failed"));

    let threshold = cli(&[
        "test",
        "fixtures/weak-auth",
        "--operator",
        "AUTH-001",
        "--test-command",
        "true",
        "--min-score",
        "100",
        "--no-progress",
    ]);
    assert_eq!(threshold.status.code(), Some(4));
    assert!(stderr(&threshold).starts_with("error: semantic score"));
}

#[test]
fn context_chains_are_joined_without_repeating_the_prefix() {
    let missing = std::env::temp_dir().join(format!("soro-prefix-{}", std::process::id()));
    let _ = fs::remove_file(&missing);
    let output = cli(&["validate-corpus", missing.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let err = stderr(&output);
    assert_eq!(err.matches("error:").count(), 1, "{err}");
    assert!(
        err.starts_with("error: cannot read corpus manifest "),
        "{err}"
    );
}

#[test]
fn clap_usage_errors_use_the_same_lowercase_prefix() {
    let output = cli(&["list", "--operator", "UNKNOWN"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).starts_with("error: "));
}

#[test]
fn cache_warnings_and_notes_are_prefixed() {
    let root = std::env::temp_dir().join(format!("soro-prefix-cache-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "fn check(user: Address) { user.require_auth(); }\n",
    )
    .unwrap();
    let target = root.join("cache");
    let run = || {
        cli(&[
            "test",
            root.to_str().unwrap(),
            "--test-command",
            "true",
            "--cache",
            "--no-progress",
            "--target-dir",
            target.to_str().unwrap(),
        ])
    };
    let first = run();
    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    assert!(stderr(&first).contains("note: persistent result cache enabled at"));
    assert!(stderr(&first).contains("note: result cache: "));

    fs::write(target.join("result-cache.json"), "{ not json").unwrap();
    let second = run();
    let _ = fs::remove_dir_all(&root);
    assert!(stderr(&second).contains("warning: result cache miss: ignoring malformed cache at"));
}

/// Guards the convention at the source level: every message literal handed to
/// `bail!`, `anyhow!`, or `.context(...)` starts lowercase (or with a flag,
/// placeholder, backtick, or digit) and has no trailing period or `Error:`
/// prefix. Keeps new messages consistent without relying on a runtime path.
#[test]
fn message_literals_follow_the_capitalization_convention() {
    fn literals_after<'a>(source: &'a str, marker: &str) -> Vec<&'a str> {
        let mut found = Vec::new();
        let mut rest = source;
        while let Some(index) = rest.find(marker) {
            rest = &rest[index + marker.len()..];
            let trimmed = rest.trim_start();
            if let Some(body) = trimmed.strip_prefix('"') {
                if let Some(end) = body.find('"') {
                    found.push(&body[..end]);
                }
            }
        }
        found
    }

    let mut violations = Vec::new();
    for file in ["src/main.rs", "src/diagnostics.rs"] {
        let full = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(file)).unwrap();
        // Unit tests intentionally contain arbitrary example text.
        let source = full.split("#[cfg(test)]").next().unwrap();
        for marker in [
            "bail!(",
            "anyhow::anyhow!(",
            ".context(",
            "with_context(|| format!(",
            "diagnostics::warning(",
            "diagnostics::note(",
        ] {
            for literal in literals_after(source, marker) {
                let first = literal.chars().next().unwrap_or('x');
                let allowed_start = first.is_ascii_lowercase()
                    || matches!(first, '-' | '{' | '`' | '\'' | '$')
                    || first.is_ascii_digit();
                if !allowed_start
                    || literal.trim_end().ends_with('.')
                    || literal.starts_with("Error:")
                    || literal.starts_with("error:")
                {
                    violations.push(format!("{file}: {marker} \"{literal}\""));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "message literals must start lowercase and not end with a period:\n{}",
        violations.join("\n")
    );
}
