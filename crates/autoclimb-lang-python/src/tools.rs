//! External tool adapters for Python: ruff and bandit.

use autoclimb_detectors::tool_runner::{OutputFormat, ToolSpec};
use autoclimb_types::enums::Tier;

/// Create a ToolSpec for ruff (Python linter).
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
        timeout_secs: 120,
        detector_name: "ruff".into(),
        tier: Tier::AutoFix,
        fix_cmd: Some(vec![
            "ruff".into(),
            "check".into(),
            "--fix".into(),
            ".".into(),
        ]),
    }
}

/// Create a ToolSpec for bandit (Python security scanner).
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
        tier: Tier::QuickFix,
        fix_cmd: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruff_spec_has_json_format() {
        let spec = ruff_spec();
        assert_eq!(spec.label, "ruff");
        assert!(matches!(spec.format, OutputFormat::Json));
    }

    #[test]
    fn bandit_spec_is_tier_2() {
        let spec = bandit_spec();
        assert_eq!(spec.tier, Tier::QuickFix);
    }
}
