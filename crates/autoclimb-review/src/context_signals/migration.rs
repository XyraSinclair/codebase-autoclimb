//! Migration signal detector.
//!
//! Detects in-progress migrations and deprecation patterns:
//! - Deprecation markers (@deprecated, #[deprecated])
//! - Migration TODOs (TODO: migrate, FIXME: port)
//! - Pattern pairs indicating incomplete migrations (e.g., old + new APIs coexisting)

use super::{ContextSignal, SignalSeverity, SignalType};

/// Deprecation marker patterns.
const DEPRECATION_PATTERNS: &[&str] = &[
    "@deprecated",
    "#[deprecated",
    "DeprecationWarning",
    "@Deprecated",
    "// deprecated",
    "# deprecated",
    "DEPRECATED",
    "mark_deprecated",
];

/// Migration TODO patterns.
const MIGRATION_TODO_PATTERNS: &[&str] = &[
    "TODO: migrate",
    "TODO: port",
    "FIXME: migrate",
    "FIXME: port",
    "TODO: convert",
    "TODO: replace",
    "MIGRATION:",
    "MIGRATE:",
    "# LEGACY",
    "// LEGACY",
];

/// Pattern pairs: (old pattern, new pattern) indicating incomplete migration.
const PATTERN_PAIRS: &[(&str, &str)] = &[
    ("require(", "import "),              // CommonJS → ESM
    ("var ", "const "),                   // var → const/let
    ("class ", "function "),              // class → functional (React)
    (".then(", "async "),                 // Promise chains → async/await
    ("unittest", "pytest"),               // unittest → pytest
    ("jQuery", "document.querySelector"), // jQuery → vanilla JS
];

/// Detect migration patterns.
pub fn detect(file_contents: &[(String, String)]) -> Vec<ContextSignal> {
    let mut deprecated_files = Vec::new();
    let mut migration_todo_files = Vec::new();
    let mut mixed_pattern_files: Vec<(String, String)> = Vec::new();

    for (path, content) in file_contents {
        // Deprecation markers
        let has_deprecation = DEPRECATION_PATTERNS.iter().any(|p| content.contains(p));
        if has_deprecation {
            deprecated_files.push(path.clone());
        }

        // Migration TODOs
        let has_migration_todo = MIGRATION_TODO_PATTERNS.iter().any(|p| content.contains(p));
        if has_migration_todo {
            migration_todo_files.push(path.clone());
        }

        // Pattern pairs
        for (old, new) in PATTERN_PAIRS {
            if content.contains(old) && content.contains(new) {
                mixed_pattern_files.push((path.clone(), format!("{old} + {new}")));
                break;
            }
        }
    }

    let mut signals = Vec::new();

    if deprecated_files.len() >= 3 {
        signals.push(ContextSignal {
            signal_type: SignalType::Migration,
            severity: SignalSeverity::Medium,
            message: format!(
                "{} files contain deprecation markers — consider completing the migration",
                deprecated_files.len(),
            ),
            files: deprecated_files,
            detail: serde_json::json!({"pattern": "deprecation_markers"}),
        });
    }

    if migration_todo_files.len() >= 2 {
        signals.push(ContextSignal {
            signal_type: SignalType::Migration,
            severity: SignalSeverity::Medium,
            message: format!(
                "{} files have migration TODOs — track progress",
                migration_todo_files.len(),
            ),
            files: migration_todo_files,
            detail: serde_json::json!({"pattern": "migration_todos"}),
        });
    }

    if mixed_pattern_files.len() >= 3 {
        let files: Vec<String> = mixed_pattern_files.iter().map(|(f, _)| f.clone()).collect();
        let patterns: Vec<String> = mixed_pattern_files.iter().map(|(_, p)| p.clone()).collect();

        signals.push(ContextSignal {
            signal_type: SignalType::Migration,
            severity: SignalSeverity::Low,
            message: format!(
                "{} files mix old and new patterns — incomplete migration",
                files.len(),
            ),
            files,
            detail: serde_json::json!({"pattern": "mixed_patterns", "examples": patterns}),
        });
    }

    signals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_deprecation_markers() {
        let files: Vec<(String, String)> = (0..4)
            .map(|i| {
                (
                    format!("src/old_{i}.py"),
                    "@deprecated\ndef old_func():\n    pass".into(),
                )
            })
            .collect();

        let signals = detect(&files);
        assert!(signals.iter().any(|s| s
            .detail
            .get("pattern")
            .and_then(|v| v.as_str())
            .is_some_and(|p| p == "deprecation_markers")));
    }

    #[test]
    fn detects_migration_todos() {
        let files = vec![
            (
                "src/a.py".into(),
                "# TODO: migrate to new API\ndef old_way(): pass".into(),
            ),
            (
                "src/b.py".into(),
                "# TODO: port to async\ndef sync_way(): pass".into(),
            ),
        ];

        let signals = detect(&files);
        assert!(signals.iter().any(|s| s
            .detail
            .get("pattern")
            .and_then(|v| v.as_str())
            .is_some_and(|p| p == "migration_todos")));
    }

    #[test]
    fn no_signal_below_threshold() {
        let files = vec![("src/a.py".into(), "def clean_func():\n    return 42".into())];

        let signals = detect(&files);
        assert!(signals.is_empty());
    }
}
