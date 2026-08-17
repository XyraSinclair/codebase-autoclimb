use crate::enums::ActionType;

/// Static metadata for a detector, matching Python's `DetectorMeta`.
#[derive(Debug, Clone)]
pub struct DetectorMeta {
    pub name: &'static str,
    pub display: &'static str,
    pub dimension: &'static str,
    pub action_type: ActionType,
    pub guidance: &'static str,
    pub fixers: &'static [&'static str],
    pub tool: Option<&'static str>,
    pub structural: bool,
    pub needs_judgment: bool,
}

/// All registered detectors, in display order.
pub static DETECTORS: &[DetectorMeta] = &[
    // Auto-fixable
    DetectorMeta {
        name: "logs",
        display: "logs",
        dimension: "Code quality",
        action_type: ActionType::AutoFix,
        guidance: "remove debug logs",
        fixers: &["debug-logs"],
        tool: None,
        structural: false,
        needs_judgment: false,
    },
    DetectorMeta {
        name: "unused",
        display: "unused",
        dimension: "Code quality",
        action_type: ActionType::AutoFix,
        guidance: "remove unused imports and variables",
        fixers: &["unused-imports", "unused-vars", "unused-params"],
        tool: None,
        structural: false,
        needs_judgment: false,
    },
    DetectorMeta {
        name: "exports",
        display: "exports",
        dimension: "Code quality",
        action_type: ActionType::ManualFix,
        guidance: "run `knip --fix` to remove dead exports",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: false,
    },
    DetectorMeta {
        name: "deprecated",
        display: "deprecated",
        dimension: "Code quality",
        action_type: ActionType::ManualFix,
        guidance: "remove deprecated symbols or migrate callers",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: false,
    },
    DetectorMeta {
        name: "structural",
        display: "structural",
        dimension: "File health",
        action_type: ActionType::Refactor,
        guidance: "decompose large files — extract logic into focused modules",
        fixers: &[],
        tool: None,
        structural: true,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "props",
        display: "props",
        dimension: "Code quality",
        action_type: ActionType::Refactor,
        guidance: "split bloated components, extract sub-components",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "single_use",
        display: "single_use",
        dimension: "Code quality",
        action_type: ActionType::Reorganize,
        guidance: "inline or relocate with `autoclimb move`",
        fixers: &[],
        tool: Some("move"),
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "coupling",
        display: "coupling",
        dimension: "Code quality",
        action_type: ActionType::Reorganize,
        guidance: "fix boundary violations with `autoclimb move`",
        fixers: &[],
        tool: Some("move"),
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "cycles",
        display: "cycles",
        dimension: "Security",
        action_type: ActionType::Reorganize,
        guidance: "break cycles by extracting shared code or using `autoclimb move`",
        fixers: &[],
        tool: Some("move"),
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "orphaned",
        display: "orphaned",
        dimension: "Code quality",
        action_type: ActionType::Reorganize,
        guidance: "delete dead files or relocate with `autoclimb move`",
        fixers: &[],
        tool: Some("move"),
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "uncalled_functions",
        display: "uncalled functions",
        dimension: "Code quality",
        action_type: ActionType::Refactor,
        guidance: "remove dead functions or document why they're retained",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "facade",
        display: "facade",
        dimension: "Code quality",
        action_type: ActionType::Reorganize,
        guidance: "flatten re-export facades or consolidate barrel files",
        fixers: &[],
        tool: Some("move"),
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "patterns",
        display: "patterns",
        dimension: "Code quality",
        action_type: ActionType::Refactor,
        guidance: "align to single pattern across the codebase",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "naming",
        display: "naming",
        dimension: "Code quality",
        action_type: ActionType::Reorganize,
        guidance: "rename files with `autoclimb move` to fix conventions",
        fixers: &[],
        tool: Some("move"),
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "smells",
        display: "smells",
        dimension: "Code quality",
        action_type: ActionType::AutoFix,
        guidance: "fix code smells — dead useEffect, empty if chains",
        fixers: &["dead-useeffect", "empty-if-chain"],
        tool: None,
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "react",
        display: "react",
        dimension: "Code quality",
        action_type: ActionType::Refactor,
        guidance: "refactor React antipatterns (state sync, provider nesting, hook bloat)",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "dupes",
        display: "dupes",
        dimension: "Duplication",
        action_type: ActionType::Refactor,
        guidance: "extract shared utility or consolidate duplicates",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "stale_exclude",
        display: "stale exclude",
        dimension: "Code quality",
        action_type: ActionType::ManualFix,
        guidance: "remove stale exclusion or verify it's still needed",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: false,
    },
    DetectorMeta {
        name: "dict_keys",
        display: "dict keys",
        dimension: "Code quality",
        action_type: ActionType::Refactor,
        guidance: "fix dict key mismatches — dead writes are likely dead code, schema drift suggests a typo or missed rename",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "flat_dirs",
        display: "flat dirs",
        dimension: "Code quality",
        action_type: ActionType::Reorganize,
        guidance: "create subdirectories and use `autoclimb move`",
        fixers: &[],
        tool: Some("move"),
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "signature",
        display: "signature",
        dimension: "Code quality",
        action_type: ActionType::Refactor,
        guidance: "consolidate inconsistent function signatures",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "global_mutable_config",
        display: "global mutable config",
        dimension: "Code quality",
        action_type: ActionType::ManualFix,
        guidance: "refactor module-level mutable state — use explicit init functions or dependency injection",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "private_imports",
        display: "private imports",
        dimension: "Code quality",
        action_type: ActionType::ManualFix,
        guidance: "stop importing private symbols across module boundaries",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "layer_violation",
        display: "layer violation",
        dimension: "Code quality",
        action_type: ActionType::ManualFix,
        guidance: "fix architectural layer violations — move shared code to the correct layer",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "responsibility_cohesion",
        display: "responsibility cohesion",
        dimension: "Code quality",
        action_type: ActionType::Refactor,
        guidance: "split modules with too many responsibilities — extract focused sub-modules",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "boilerplate_duplication",
        display: "boilerplate duplication",
        dimension: "Duplication",
        action_type: ActionType::Refactor,
        guidance: "extract shared boilerplate into reusable helpers or base classes",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: true,
    },
    DetectorMeta {
        name: "stale_wontfix",
        display: "stale wontfix",
        dimension: "Code quality",
        action_type: ActionType::ManualFix,
        guidance: "re-evaluate old wontfix decisions — fix, document, or escalate",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: false,
    },
    DetectorMeta {
        name: "test_coverage",
        display: "test coverage",
        dimension: "Test health",
        action_type: ActionType::Refactor,
        guidance: "add tests for untested production modules — prioritize by import count",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: false,
    },
    DetectorMeta {
        name: "security",
        display: "security",
        dimension: "Security",
        action_type: ActionType::ManualFix,
        guidance: "review and fix security findings — prioritize by severity",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: false,
    },
    DetectorMeta {
        name: "concerns",
        display: "design concerns",
        dimension: "Design coherence",
        action_type: ActionType::Refactor,
        guidance: "address design concerns confirmed by subjective evaluation",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: false,
    },
    DetectorMeta {
        name: "review",
        display: "design review",
        dimension: "Test health",
        action_type: ActionType::Refactor,
        guidance: "address design quality findings from AI code review",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: false,
    },
    DetectorMeta {
        name: "subjective_review",
        display: "subjective review",
        dimension: "Test health",
        action_type: ActionType::ManualFix,
        guidance: "run `autoclimb review --prepare` to evaluate files against quality dimensions",
        fixers: &[],
        tool: None,
        structural: false,
        needs_judgment: false,
    },
];

/// Look up a detector by name.
pub fn detector_by_name(name: &str) -> Option<&'static DetectorMeta> {
    DETECTORS.iter().find(|d| d.name == name)
}

/// All detector names that need LLM judgment.
pub fn judgment_detectors() -> impl Iterator<Item = &'static str> {
    DETECTORS
        .iter()
        .filter(|d| d.needs_judgment)
        .map(|d| d.name)
}

/// Compact action type label for a dimension.
pub fn dimension_action_type(dim_name: &str) -> &'static str {
    let mut best_priority = u8::MAX;
    let mut best_label = "manual";
    for d in DETECTORS {
        if d.dimension == dim_name {
            let pri = d.action_type.priority();
            if pri < best_priority {
                best_priority = pri;
                best_label = d.action_type.label();
            }
        }
    }
    best_label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_detectors_have_unique_names() {
        let mut seen = std::collections::HashSet::new();
        for d in DETECTORS {
            assert!(seen.insert(d.name), "duplicate detector: {}", d.name);
        }
    }

    #[test]
    fn lookup_by_name() {
        let d = detector_by_name("cycles").unwrap();
        assert_eq!(d.dimension, "Security");
        assert!(d.needs_judgment);
    }

    #[test]
    fn judgment_detectors_count() {
        let count = judgment_detectors().count();
        assert!(count > 10, "expected many judgment detectors, got {count}");
    }

    #[test]
    fn dimension_action_type_works() {
        assert_eq!(dimension_action_type("Code quality"), "fix");
        assert_eq!(dimension_action_type("File health"), "refactor");
    }
}
