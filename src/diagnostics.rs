//! Consistent, testable stderr diagnostics.
//!
//! Every human-readable diagnostic the CLI writes to stderr uses one of three
//! lowercase prefixes, matching Cargo, rustc, and Clap's own argument errors:
//!
//! * `error: ` — a fatal condition; the command exits non-zero.
//! * `warning: ` — a non-fatal problem the user may want to act on.
//! * `note: ` — neutral context, such as cache activity or why a selection is empty.
//!
//! Message bodies start with a lowercase letter (flags, identifiers, and paths
//! excepted) and have no trailing period. Anything written through [`emit`] has
//! environment secrets redacted, so call sites cannot forget to do so.

use crate::redaction::redact_environment_secrets;
use std::{
    io::{self, Write},
    path::PathBuf,
};

/// Severity of a diagnostic, which determines its stderr prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Level {
    Error,
    Warning,
    Note,
}

impl Level {
    pub(crate) fn prefix(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warning => "warning",
            Level::Note => "note",
        }
    }
}

/// Formats one diagnostic line (without a trailing newline), redacting secrets.
pub(crate) fn format_line(level: Level, message: &str) -> String {
    format!(
        "{}: {}",
        level.prefix(),
        redact_environment_secrets(message)
    )
}

/// Writes a diagnostic to `sink`. Multi-line messages are prefixed on every line.
pub(crate) fn emit_to(sink: &mut dyn Write, level: Level, message: &str) -> io::Result<()> {
    for line in message.lines() {
        writeln!(sink, "{}", format_line(level, line))?;
    }
    Ok(())
}

/// Writes a diagnostic to stderr. A broken stderr must never abort a run.
pub(crate) fn emit(level: Level, message: &str) {
    let _ = emit_to(&mut io::stderr(), level, message);
}

pub(crate) fn warning(message: &str) {
    emit(Level::Warning, message);
}

pub(crate) fn note(message: &str) {
    emit(Level::Note, message);
}

/// Prints a fatal error, including its full context chain, to stderr.
pub(crate) fn fatal(error: &anyhow::Error) {
    emit(Level::Error, &format!("{error:#}"));
}

/// Facts about a finished selection, used to explain an empty result.
///
/// The three counts describe the selection pipeline in order: discovery
/// filters (`--package`, `--operator`, `--file`, `--function`,
/// `--exclude-path`), the changed-code selection (`--changed-file`,
/// `--diff-base`), and finally `--shard`.
#[derive(Clone, Debug, Default)]
pub(crate) struct EmptySelection {
    pub(crate) root: PathBuf,
    /// Rendered active filters, for example `--operator AUTH-009`.
    pub(crate) filters: Vec<String>,
    /// Active discovery filters (package, operator, file, function, exclusions).
    pub(crate) discovery_filters: Vec<String>,
    /// Active changed-code selection flags.
    pub(crate) changed_filters: Vec<String>,
    /// Rendered `--shard INDEX/TOTAL`, when active.
    pub(crate) shard: Option<String>,
    pub(crate) files_scanned: usize,
    pub(crate) unparsable_files: Vec<PathBuf>,
    /// Mutants found before operator/file/function filters and suppression.
    pub(crate) candidates: usize,
    pub(crate) after_discovery: usize,
    pub(crate) after_changed: usize,
}

const MAX_LISTED_FILES: usize = 5;

/// Explains why a selection is empty, as ordered `(level, message)` pairs.
///
/// A valid empty selection is reported with `note:`. Conditions that could
/// instead indicate a discovery problem (no Rust sources at all, or sources
/// that failed to parse) are reported with `warning:` so scripts and humans can
/// tell the two apart. The result is a pure function of its input.
pub(crate) fn explain_empty_selection(selection: &EmptySelection) -> Vec<(Level, String)> {
    let mut lines = Vec::new();

    if selection.files_scanned == 0 && selection.unparsable_files.is_empty() {
        lines.push((
            Level::Warning,
            format!(
                "no Rust source files were found under {}; check the project path, --package, and --exclude-path",
                selection.root.display()
            ),
        ));
    }

    if !selection.unparsable_files.is_empty() {
        let shown = selection
            .unparsable_files
            .iter()
            .take(MAX_LISTED_FILES)
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let extra = selection
            .unparsable_files
            .len()
            .saturating_sub(MAX_LISTED_FILES);
        let more = if extra > 0 {
            format!(" (and {extra} more)")
        } else {
            String::new()
        };
        lines.push((
            Level::Warning,
            format!(
                "{} Rust source file(s) could not be parsed and were skipped: {shown}{more}",
                selection.unparsable_files.len()
            ),
        ));
    }

    let reason = if selection.files_scanned == 0 {
        None
    } else if selection.candidates == 0 {
        Some(format!(
            "no supported Soroban mutation sites were found in {} scanned Rust source file(s); this is a valid empty result, not a discovery failure",
            selection.files_scanned
        ))
    } else if selection.after_discovery == 0 && selection.discovery_filters.is_empty() {
        Some(format!(
            "all {} candidate mutant(s) were suppressed as equivalent; see the suppression summary",
            selection.candidates
        ))
    } else if selection.after_discovery == 0 {
        Some(format!(
            "{} candidate mutant(s) were discovered but none matched the discovery filters: {}",
            selection.candidates,
            selection.discovery_filters.join(", ")
        ))
    } else if selection.after_changed == 0 {
        Some(format!(
            "{} mutant(s) matched the discovery filters but none are in the changed-code selection: {}",
            selection.after_discovery,
            selection.changed_filters.join(", ")
        ))
    } else {
        selection.shard.as_ref().map(|shard| {
            format!(
                "{} mutant(s) were selected before sharding but {shard} contains none of them",
                selection.after_changed
            )
        })
    };
    if let Some(reason) = reason {
        lines.push((Level::Note, reason));
    }

    if selection.filters.is_empty() {
        lines.push((Level::Note, "no selection filters were active".to_owned()));
    } else {
        lines.push((
            Level::Note,
            format!("active selection filters: {}", selection.filters.join(", ")),
        ));
    }
    lines
}

/// Writes the empty-selection explanation to stderr.
pub(crate) fn report_empty_selection(selection: &EmptySelection) {
    for (level, message) in explain_empty_selection(selection) {
        emit(level, &message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> EmptySelection {
        EmptySelection {
            root: PathBuf::from("/project"),
            files_scanned: 3,
            ..EmptySelection::default()
        }
    }

    #[test]
    fn prefixes_are_lowercase_and_stable() {
        assert_eq!(
            format_line(Level::Error, "cannot read x"),
            "error: cannot read x"
        );
        assert_eq!(format_line(Level::Warning, "w"), "warning: w");
        assert_eq!(format_line(Level::Note, "n"), "note: n");
    }

    #[test]
    fn multi_line_messages_are_prefixed_on_every_line() {
        let mut out = Vec::new();
        emit_to(&mut out, Level::Warning, "first\nsecond").unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "warning: first\nwarning: second\n"
        );
    }

    #[test]
    fn no_candidates_is_a_valid_empty_result() {
        let lines = explain_empty_selection(&base());
        assert!(lines.iter().all(|(level, _)| *level != Level::Warning));
        assert!(lines[0]
            .1
            .contains("valid empty result, not a discovery failure"));
        assert_eq!(lines.last().unwrap().1, "no selection filters were active");
    }

    #[test]
    fn missing_sources_and_parse_failures_are_warnings() {
        let none = EmptySelection {
            files_scanned: 0,
            ..base()
        };
        let lines = explain_empty_selection(&none);
        assert_eq!(lines[0].0, Level::Warning);
        assert!(lines[0]
            .1
            .contains("no Rust source files were found under /project"));

        let broken = EmptySelection {
            files_scanned: 1,
            unparsable_files: (0..7)
                .map(|i| PathBuf::from(format!("src/f{i}.rs")))
                .collect(),
            ..base()
        };
        let lines = explain_empty_selection(&broken);
        assert_eq!(lines[0].0, Level::Warning);
        assert!(lines[0]
            .1
            .contains("7 Rust source file(s) could not be parsed"));
        assert!(lines[0].1.ends_with("src/f4.rs (and 2 more)"));
    }

    #[test]
    fn names_the_stage_that_emptied_the_selection() {
        let discovery = EmptySelection {
            candidates: 4,
            discovery_filters: vec!["--operator AUTH-009".into()],
            filters: vec!["--operator AUTH-009".into()],
            ..base()
        };
        let lines = explain_empty_selection(&discovery);
        assert!(lines[0].1.contains("4 candidate mutant(s) were discovered but none matched the discovery filters: --operator AUTH-009"));
        assert_eq!(
            lines.last().unwrap().1,
            "active selection filters: --operator AUTH-009"
        );

        let changed = EmptySelection {
            candidates: 4,
            after_discovery: 2,
            changed_filters: vec!["--diff-base main".into()],
            filters: vec!["--diff-base main".into()],
            ..base()
        };
        assert!(explain_empty_selection(&changed)[0]
            .1
            .contains("none are in the changed-code selection: --diff-base main"));

        let shard = EmptySelection {
            candidates: 4,
            after_discovery: 2,
            after_changed: 2,
            shard: Some("--shard 3/4".into()),
            filters: vec!["--shard 3/4".into()],
            ..base()
        };
        assert!(explain_empty_selection(&shard)[0]
            .1
            .contains("2 mutant(s) were selected before sharding but --shard 3/4 contains none"));
    }

    #[test]
    fn fully_suppressed_candidates_are_explained() {
        let lines = explain_empty_selection(&EmptySelection {
            candidates: 2,
            ..base()
        });
        assert!(lines[0]
            .1
            .contains("all 2 candidate mutant(s) were suppressed"));
    }
}
