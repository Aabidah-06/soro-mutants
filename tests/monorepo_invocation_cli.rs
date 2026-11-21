//! End-to-end checks for the monorepo invocation patterns documented in
//! `docs/monorepo.md` (issue #133). They use `--test-command true`, so no Cargo
//! build or Soroban SDK is needed.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

const WORKSPACE: &str = "fixtures/workspace-multi-package";

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-soro-mutants"))
        .args(args)
        .output()
        .expect("CLI should run")
}

fn scratch(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("soro-mono-{name}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn count(args: &[&str]) -> usize {
    let mut full = vec!["list"];
    full.extend_from_slice(args);
    full.push("--count-only");
    let output = cli(&full);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap()
}

#[test]
fn documented_scoping_patterns_select_the_expected_mutants() {
    assert_eq!(count(&[WORKSPACE]), 2);
    assert_eq!(count(&[WORKSPACE, "-p", "package-a"]), 1);
    assert_eq!(count(&[WORKSPACE, "-p", "package-b"]), 1);
    assert_eq!(count(&[WORKSPACE, "--exclude-path", "package-b/**"]), 1);
    assert_eq!(
        count(&[
            WORKSPACE,
            "--exclude-path",
            "package-a/**",
            "--exclude-path",
            "package-b/**"
        ]),
        0
    );
}

#[test]
fn documented_package_selection_runs_end_to_end_with_a_separate_report() {
    let dir = scratch("package");
    let report = dir.join("package-a.json");
    let output = cli(&[
        "test",
        WORKSPACE,
        "-p",
        "package-a",
        "--test-command",
        "true",
        "--target-dir",
        dir.join("state").to_str().unwrap(),
        "--no-progress",
        "--json",
        "--output",
        report.to_str().unwrap(),
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty(), "the report goes to --output");
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
    let results = value["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0]["mutant"]["file"]
        .as_str()
        .unwrap()
        .contains("package-a"));
    assert!(dir.join("state/cache-identity.json").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unknown_package_fails_with_a_prefixed_error() {
    let output = cli(&["list", WORKSPACE, "-p", "missing"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim_end(),
        "error: package `missing` not found in workspace"
    );
}

fn write_standalone(root: &Path, name: &str) {
    let project = root.join("tools").join(name);
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(
        project.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n"),
    )
    .unwrap();
    fs::write(
        project.join("src/lib.rs"),
        "pub fn guarded(user: Address) { user.require_auth(); }\n",
    )
    .unwrap();
}

#[test]
fn independent_projects_keep_separate_state_and_reports() {
    let root = scratch("independent");
    write_standalone(&root, "oracle");
    write_standalone(&root, "vesting");
    let reports = root.join("reports");
    fs::create_dir_all(&reports).unwrap();

    for name in ["oracle", "vesting"] {
        let project = root.join("tools").join(name);
        let report = reports.join(format!("{name}.json"));
        let output = cli(&[
            "test",
            project.to_str().unwrap(),
            "--test-command",
            "true",
            "--no-progress",
            "--json",
            "--output",
            report.to_str().unwrap(),
        ]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut roots = Vec::new();
    for name in ["oracle", "vesting"] {
        let project = root.join("tools").join(name);
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(reports.join(format!("{name}.json"))).unwrap())
                .unwrap();
        assert_eq!(report["results"].as_array().unwrap().len(), 1, "{name}");

        let identity: serde_json::Value = serde_json::from_slice(
            &fs::read(project.join(".soro-mutants-target/cache-identity.json")).unwrap(),
        )
        .unwrap();
        roots.push(identity["workspace_root"].as_str().unwrap().to_owned());
    }
    assert_ne!(roots[0], roots[1], "each project records its own root");

    // Cleaning one project leaves the other's state alone.
    let clean = cli(&["clean", root.join("tools/oracle").to_str().unwrap()]);
    assert_eq!(clean.status.code(), Some(0));
    assert!(!root.join("tools/oracle/.soro-mutants-target").exists());
    assert!(root.join("tools/vesting/.soro-mutants-target").exists());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn monorepo_guide_is_linked_and_documents_the_flags_it_demonstrates() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let guide = fs::read_to_string(manifest.join("docs/monorepo.md")).unwrap();
    for needle in [
        "-p vault",
        "--exclude-path",
        "--test-dir",
        "--target-dir",
        "--no-progress",
        "UNVIABLE",
        "workspace root",
    ] {
        assert!(
            guide.contains(needle),
            "docs/monorepo.md must mention {needle}"
        );
    }
    let readme = fs::read_to_string(manifest.join("README.md")).unwrap();
    assert!(readme.contains("docs/monorepo.md"));
    let troubleshooting = fs::read_to_string(manifest.join("docs/troubleshooting.md")).unwrap();
    assert!(troubleshooting.contains("monorepo.md"));
}
