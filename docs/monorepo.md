# Running Soro Mutants in a monorepo

A monorepo can hold several Rust projects: one Cargo workspace with many
contracts, independent standalone crates, or both. These patterns scope a run to
the project you mean to test. They use only options that exist today, so they do
not depend on any other planned work.

## How a run is scoped

Three facts decide what a run touches:

- **The `PATH` argument is the project root.** Discovery walks every `.rs` file
  below it. The clean baseline runs there, and each mutant is tested in a scratch
  copy of that whole tree. `.git`, `target`, `node_modules`, and Soro Mutants'
  own state directories are not copied.
- **Generated state lives under the project root.** Build caches, the progress
  file, and the optional result cache default to `PATH/.soro-mutants-target`.
  Two projects with different roots therefore never share state. `--target-dir`
  moves it elsewhere.
- **`-p`/`--package` narrows discovery inside a workspace.** It does not change
  the project root. With the default test command it also appends `-p NAME` to
  `cargo test -q`. See [workspace package selection](../README.md#workspace-package-selection).

## Pattern 1: one package of a workspace

Point at the **workspace root** and select the package with `-p`:

```bash
cargo soro-mutants list . -p vault --count-only
cargo soro-mutants test . -p vault --json --output reports/vault.json
```

Repeat `-p` to select several packages. An unknown package name fails with
exit code 1 and `error: package `NAME` not found in workspace`.

> **Do not point `PATH` at a workspace member's directory.** The scratch copy
> of a member is no longer listed in the parent workspace's `members`, so Cargo
> rejects it, the compile gate fails, and every mutant is reported `UNVIABLE`.
> Unviable mutants are excluded from the score, so this looks like an empty run
> rather than a failure. Use the workspace root with `-p` instead.

## Pattern 2: independent projects in one repository

A project that is not a member of a parent workspace (it has its own
`[workspace]` table or no workspace above it) can be used directly as `PATH`.
Run each project separately and keep the reports apart:

```bash
for project in tools/oracle tools/vesting; do
  name=$(basename "$project")
  cargo soro-mutants test "$project" \
    --test-command 'cargo test -q --locked' \
    --json --output "reports/$name.json" || status=$?
done
exit "${status:-0}"
```

Each project gets its own `.soro-mutants-target`, so one project's cached
outcomes can never be reused for another. The loop records a failing exit code
(3 for a failed baseline, 4 for a missed `--min-score`) but still runs the
remaining projects.

## Pattern 3: exclude nested projects from a larger root

When a workspace root contains projects you do not want mutated, exclude them
with project-relative globs. Exclusions apply to discovery only, so use them
together with a test command that does not run the excluded projects.

```bash
cargo soro-mutants list . --exclude-path 'tools/**' --exclude-path 'vendor/**'
```

Compare counts before a long run:

```bash
cargo soro-mutants list . --count-only                       # everything
cargo soro-mutants list . -p vault --count-only              # one package
cargo soro-mutants list . --exclude-path 'tools/**' --count-only
```

## Pattern 4: tests run from a subdirectory

If the tests must be started from a subdirectory of the project, keep `PATH` at
the project root and pass `--test-dir`, which is relative to `PATH`:

```bash
cargo soro-mutants test . --test-dir contracts/vault --test-command 'cargo test -q'
```

## Pattern 5: shared cache location in CI

Give every project its own `--target-dir` when you want caches outside the
checkout. Use the same value with `clean`:

```bash
cargo soro-mutants test . -p vault --target-dir "$RUNNER_TEMP/soro/vault"
cargo soro-mutants clean . --target-dir "$RUNNER_TEMP/soro/vault"
```

Never point two different projects at one `--target-dir`. The directory records
the identity of the last project that used it (including its root), and a
mismatch discards the earlier build caches, so the projects keep evicting each
other. The progress file and result cache also live there.

## Checking an invocation before running it

1. `cargo soro-mutants list PATH ... --count-only` should print the number you
   expect. When it prints `0`, stderr explains which filter or stage removed
   every mutant. See [empty selections](troubleshooting.md#empty-selections).
2. Add `--no-progress` in CI to keep logs to the final summary.
3. Read the exit status: 0 success, 1 operational error, 2 invalid arguments,
   3 failed baseline, 4 score threshold. Reports go to stdout (or `--output`);
   diagnostics always go to stderr.

## Troubleshooting

| Symptom | Likely cause | Fix |
| --- | --- | --- |
| Every mutant is `UNVIABLE` | `PATH` is a workspace member directory | Use the workspace root with `-p NAME`. |
| `error: package `X` not found in workspace` | `-p` names a package that is not a workspace member of `PATH` | Run from the workspace root and check the package's `name` in its `Cargo.toml`. |
| `warning: no Rust source files were found under ...` | `PATH` is not the project you intended | Check the path; `PATH` is relative to the shell's working directory. |
| Mutants from an unexpected project appear | `PATH` is above several projects | Use `-p`, a narrower `PATH`, or `--exclude-path`. |
