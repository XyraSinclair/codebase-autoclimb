//! Pre-built external tool specifications for common linters and analyzers.

use autoclimb_detectors::tool_runner::{OutputFormat, ToolSpec};
use autoclimb_types::enums::Tier;

/// Ruff (Python linter) — fast replacement for flake8/pylint.
pub fn ruff_spec() -> ToolSpec {
    ToolSpec {
        label: "ruff".into(),
        cmd: vec![
            "ruff".into(),
            "check".into(),
            "--output-format".into(),
            "json".into(),
            ".".into(),
        ],
        format: OutputFormat::Json,
        timeout_secs: 60,
        detector_name: "ruff".into(),
        tier: Tier::QuickFix,
        fix_cmd: Some(vec![
            "ruff".into(),
            "check".into(),
            "--fix".into(),
            ".".into(),
        ]),
    }
}

/// Bandit (Python security linter).
pub fn bandit_spec() -> ToolSpec {
    ToolSpec {
        label: "bandit".into(),
        cmd: vec![
            "bandit".into(),
            "-r".into(),
            "-f".into(),
            "json".into(),
            ".".into(),
        ],
        format: OutputFormat::Json,
        timeout_secs: 120,
        detector_name: "bandit".into(),
        tier: Tier::Judgment,
        fix_cmd: None,
    }
}

/// golangci-lint (Go meta-linter).
pub fn golangci_lint_spec() -> ToolSpec {
    ToolSpec {
        label: "golangci-lint".into(),
        cmd: vec![
            "golangci-lint".into(),
            "run".into(),
            "--out-format".into(),
            "json".into(),
            "./...".into(),
        ],
        format: OutputFormat::Golangci,
        timeout_secs: 120,
        detector_name: "golangci_lint".into(),
        tier: Tier::QuickFix,
        fix_cmd: None,
    }
}

/// ESLint (JavaScript/TypeScript linter).
pub fn eslint_spec() -> ToolSpec {
    ToolSpec {
        label: "eslint".into(),
        cmd: vec![
            "npx".into(),
            "eslint".into(),
            "--format".into(),
            "unix".into(),
            ".".into(),
        ],
        format: OutputFormat::Eslint,
        timeout_secs: 120,
        detector_name: "eslint".into(),
        tier: Tier::QuickFix,
        fix_cmd: Some(vec![
            "npx".into(),
            "eslint".into(),
            "--fix".into(),
            ".".into(),
        ]),
    }
}

/// RuboCop (Ruby linter).
pub fn rubocop_spec() -> ToolSpec {
    ToolSpec {
        label: "rubocop".into(),
        cmd: vec!["rubocop".into(), "--format".into(), "json".into()],
        format: OutputFormat::Rubocop,
        timeout_secs: 120,
        detector_name: "rubocop".into(),
        tier: Tier::QuickFix,
        fix_cmd: Some(vec!["rubocop".into(), "--auto-correct".into()]),
    }
}

/// Clippy (Rust linter).
pub fn clippy_spec() -> ToolSpec {
    ToolSpec {
        label: "clippy".into(),
        cmd: vec![
            "cargo".into(),
            "clippy".into(),
            "--message-format=json".into(),
            "--".into(),
            "-D".into(),
            "warnings".into(),
        ],
        format: OutputFormat::Cargo,
        timeout_secs: 300,
        detector_name: "clippy".into(),
        tier: Tier::QuickFix,
        fix_cmd: Some(vec![
            "cargo".into(),
            "clippy".into(),
            "--fix".into(),
            "--allow-dirty".into(),
        ]),
    }
}

/// dart analyze (Dart static analysis).
pub fn dart_analyze_spec() -> ToolSpec {
    ToolSpec {
        label: "dart-analyze".into(),
        cmd: vec![
            "dart".into(),
            "analyze".into(),
            "--format".into(),
            "json".into(),
        ],
        format: OutputFormat::Json,
        timeout_secs: 120,
        detector_name: "dart_analyze".into(),
        tier: Tier::QuickFix,
        fix_cmd: Some(vec!["dart".into(), "fix".into(), "--apply".into()]),
    }
}

/// Get recommended tool specs for a language.
pub fn tools_for_language(lang: &str) -> Vec<ToolSpec> {
    match lang {
        "python" => vec![ruff_spec(), bandit_spec()],
        "go" => vec![golangci_lint_spec()],
        "typescript" | "javascript" => vec![eslint_spec()],
        "ruby" => vec![rubocop_spec()],
        "rust" => vec![clippy_spec()],
        "dart" => vec![dart_analyze_spec()],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_specs_have_commands() {
        let specs = vec![
            ruff_spec(),
            bandit_spec(),
            golangci_lint_spec(),
            eslint_spec(),
            rubocop_spec(),
            clippy_spec(),
            dart_analyze_spec(),
        ];
        for spec in &specs {
            assert!(!spec.cmd.is_empty());
            assert!(!spec.label.is_empty());
            assert!(!spec.detector_name.is_empty());
        }
    }

    #[test]
    fn tools_for_known_languages() {
        assert!(!tools_for_language("python").is_empty());
        assert!(!tools_for_language("go").is_empty());
        assert!(!tools_for_language("typescript").is_empty());
        assert!(tools_for_language("unknown_lang").is_empty());
    }
}
