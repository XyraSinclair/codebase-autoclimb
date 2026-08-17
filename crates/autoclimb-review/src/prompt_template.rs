//! Batch prompt template renderer.
//!
//! Renders the full prompt for a single review batch, including system prompt,
//! dimension descriptions, feedback contract, batch context, evidence focus,
//! historical context, and output schema.
//!
//! Ported from Python: app/commands/review/batch_prompt_template.py

use std::collections::BTreeMap;
use std::path::Path;

use crate::dimensions::DimensionRegistry;
use crate::feedback_contract;

/// Context for rendering a batch prompt.
#[derive(Debug, Clone)]
pub struct BatchContext {
    /// Batch name.
    pub name: String,
    /// Batch rationale.
    pub rationale: String,
    /// 0-indexed batch number.
    pub index: usize,
    /// Total number of batches.
    pub total: usize,
    /// Dimensions this batch should assess.
    pub dimensions: Vec<String>,
    /// Seed files to read.
    pub files_to_read: Vec<String>,
    /// Historical issues for retrospective context.
    pub historical_issues: Vec<HistoricalIssue>,
    /// Mechanical concern signals from detectors.
    pub concern_signals: Vec<ConcernSignal>,
    /// Holistic context JSON (opaque, passed to reviewer).
    pub holistic_context_json: Option<String>,
}

/// A historical issue for retrospective context.
#[derive(Debug, Clone)]
pub struct HistoricalIssue {
    pub status: String,
    pub summary: String,
    pub note: Option<String>,
}

/// A mechanical concern signal from detectors.
#[derive(Debug, Clone)]
pub struct ConcernSignal {
    pub file: String,
    pub concern_type: String,
    pub summary: String,
    pub question: String,
    pub evidence: Vec<String>,
}

/// Workflow-integrity dimensions that trigger extra guidance.
const WORKFLOW_INTEGRITY_DIMS: &[&str] = &[
    "cross_module_architecture",
    "high_level_elegance",
    "mid_level_elegance",
    "design_coherence",
    "initialization_coupling",
];

/// Maximum concern signals to show in prompt.
const MAX_CONCERN_SIGNALS: usize = 8;

/// Render a complete batch prompt.
pub fn render_batch_prompt(
    batch: &BatchContext,
    repo_root: &Path,
    packet_path: &Path,
    registry: &DimensionRegistry,
) -> String {
    let mut sections = vec![
        render_metadata(batch, repo_root, packet_path),
        render_system_prompt(),
        render_dimension_descriptions(batch, registry),
        render_evidence_note(),
        render_seed_files(batch),
    ];

    // 6. Historical focus (conditional)
    if !batch.historical_issues.is_empty() {
        sections.push(render_historical_focus(batch));
    }

    // 7. Mechanical concern signals (conditional)
    if !batch.concern_signals.is_empty() {
        sections.push(render_concern_signals(batch));
    }

    // 8. Task requirements
    sections.push(render_task_requirements(batch));

    // 9. Dimension-specific evidence focus
    sections.push(render_evidence_focus(batch, registry));

    // 10. Workflow integrity checks (conditional)
    if batch
        .dimensions
        .iter()
        .any(|d| WORKFLOW_INTEGRITY_DIMS.contains(&d.as_str()))
    {
        sections.push(render_workflow_integrity());
    }

    // 11. Holistic context JSON (conditional)
    if let Some(ref ctx) = batch.holistic_context_json {
        sections.push(format!("## Holistic Context\n\n```json\n{ctx}\n```"));
    }

    // 12. Output schema
    sections.push(render_output_schema(batch));

    sections.join("\n\n---\n\n")
}

fn render_metadata(batch: &BatchContext, repo_root: &Path, packet_path: &Path) -> String {
    let dims_str = if batch.dimensions.is_empty() {
        "(none)".to_string()
    } else {
        batch.dimensions.join(", ")
    };

    format!(
        "You are a focused subagent reviewer for a single holistic investigation batch.\n\n\
         Repository root: {}\n\
         Blind packet path: {}\n\
         Batch: {} of {} — {}\n\
         Dimensions: {}\n\
         Rationale: {}",
        repo_root.display(),
        packet_path.display(),
        batch.index + 1,
        batch.total,
        batch.name,
        dims_str,
        batch.rationale,
    )
}

fn render_system_prompt() -> String {
    format!(
        "## Review Guidelines\n\n{}\n\n{}\n\n{}",
        feedback_contract::GLOBAL_REVIEW_CONTRACT,
        feedback_contract::SCORING_BANDS,
        feedback_contract::CONFIDENCE_CALIBRATION,
    )
}

fn render_dimension_descriptions(batch: &BatchContext, registry: &DimensionRegistry) -> String {
    let mut lines = vec!["## Dimension Descriptions".to_string()];

    for dim_key in &batch.dimensions {
        if let Some(def) = registry.get(dim_key) {
            lines.push(format!("\n### {}\n", def.display_name));
            lines.push(def.description.to_string());
            lines.push(format!("\n**Look for:**\n{}", def.look_for));
            lines.push(format!("\n**Skip:**\n{}", def.skip));
        }
    }

    lines.join("\n")
}

fn render_evidence_note() -> String {
    "## Scan Evidence\n\n\
     The holistic_context.scan_evidence section (if present) contains objective \
     signals from mechanical detectors. Use these as investigation starting points:\n\
     - complexity_hotspots: files with high cyclomatic/cognitive complexity\n\
     - error_hotspots: files with concentrated exception handling findings\n\
     - signal_density: files where multiple detectors fired\n\
     - boundary_violations: import paths crossing architectural boundaries"
        .to_string()
}

fn render_seed_files(batch: &BatchContext) -> String {
    let mut lines = vec!["## Seed Files (start here)".to_string()];
    for f in &batch.files_to_read {
        lines.push(format!("- `{f}`"));
    }
    if batch.files_to_read.is_empty() {
        lines.push("(no specific seed files — explore from project root)".to_string());
    }
    lines.join("\n")
}

fn render_historical_focus(batch: &BatchContext) -> String {
    let mut lines = vec![
        "## Historical Issue Focus".to_string(),
        String::new(),
        "Check whether each issue still exists in the current code. \
         If fixed, confirm. If still present, include as a finding."
            .to_string(),
        String::new(),
    ];

    for issue in &batch.historical_issues {
        let note = issue
            .note
            .as_deref()
            .map(|n| format!(" (note: {n})"))
            .unwrap_or_default();
        lines.push(format!("- [{}] {}{}", issue.status, issue.summary, note));
    }

    lines.join("\n")
}

fn render_concern_signals(batch: &BatchContext) -> String {
    let mut lines = vec![
        "## Mechanical Concern Signals".to_string(),
        String::new(),
        "Treat each as a hypothesis: confirm or refute with direct code evidence.".to_string(),
        String::new(),
    ];

    let show_count = batch.concern_signals.len().min(MAX_CONCERN_SIGNALS);
    for signal in batch.concern_signals.iter().take(show_count) {
        lines.push(format!("### {} — {}", signal.file, signal.concern_type));
        lines.push(signal.summary.clone());
        lines.push(format!("**Question:** {}", signal.question));
        for (i, ev) in signal.evidence.iter().take(2).enumerate() {
            lines.push(format!("  Evidence {}: {ev}", i + 1));
        }
        lines.push(String::new());
    }

    let remaining = batch
        .concern_signals
        .len()
        .saturating_sub(MAX_CONCERN_SIGNALS);
    if remaining > 0 {
        lines.push(format!("(+{remaining} more concern signals)"));
    }

    lines.join("\n")
}

fn render_task_requirements(batch: &BatchContext) -> String {
    let findings_cap =
        feedback_contract::max_batch_findings_for_dimension_count(batch.dimensions.len());

    format!(
        "## Task Requirements\n\n\
         1. Read all seed files and explore related files as needed.\n\
         2. Assess each dimension listed above on a 0-100 scale (one decimal place).\n\
         3. For scores below {low}: include at least one finding for that dimension.\n\
         4. For scores below {feedback}: include explicit feedback (finding or dimension_notes).\n\
         5. For scores at or above {high}: include `issues_preventing_higher_score` in dimension_notes.\n\
         6. Produce 0-{cap} findings total. Focus on the most impactful issues.\n\
         7. Each finding must include: dimension, identifier, summary, related_files, evidence, suggestion.\n\
         8. Use confidence: high/medium/low based on calibration guidance above.\n\
         9. Use impact_scope: local/module/subsystem/codebase.\n\
         10. Use fix_scope: single_edit/multi_file_refactor/architectural_change.\n\
         11. Group related findings with root_cause_cluster when they share a root cause.\n\
         12. Do not report positive observations as findings.\n\
         13. Return exactly one JSON object matching the output schema below.",
        low = feedback_contract::LOW_SCORE_FINDING_THRESHOLD,
        feedback = feedback_contract::ASSESSMENT_FEEDBACK_THRESHOLD,
        high = feedback_contract::HIGH_SCORE_ISSUES_NOTE_THRESHOLD,
        cap = findings_cap,
    )
}

fn render_evidence_focus(batch: &BatchContext, registry: &DimensionRegistry) -> String {
    let mut lines = vec!["## Dimension-Specific Evidence Focus".to_string()];
    let mut has_content = false;

    for dim_key in &batch.dimensions {
        if let Some(def) = registry.get(dim_key) {
            if !def.evidence_focus.is_empty() {
                lines.push(format!(
                    "\n**{}:** {}",
                    def.display_name, def.evidence_focus
                ));
                has_content = true;
            }
        }
    }

    if !has_content {
        return String::new();
    }

    lines.join("\n")
}

fn render_workflow_integrity() -> String {
    "## Workflow Integrity Checks\n\n\
     When reviewing orchestration/queue/review flows, explicitly look for \
     loop-prone patterns and blind spots:\n\
     - repeated stale/reopen churn without clear exit criteria or gating\n\
     - packet/batch data being generated but dropped before prompt execution\n\
     - ranking/triage logic that can starve target-improving work\n\
     - reruns happening before existing open review work is drained\n\n\
     If found, propose concrete guardrails and where to implement them."
        .to_string()
}

fn render_output_schema(batch: &BatchContext) -> String {
    let has_abstraction = batch.dimensions.iter().any(|d| d == "abstraction_fitness");

    let sub_axes_note = if has_abstraction {
        ",\n         \"sub_axes\": {\n           \
         \"abstraction_leverage\": \"0-100\",\n           \
         \"indirection_cost\": \"0-100\",\n           \
         \"interface_honesty\": \"0-100\",\n           \
         \"delegation_density\": \"0-100\",\n           \
         \"definition_directness\": \"0-100\",\n           \
         \"type_discipline\": \"0-100\"\n         }"
    } else {
        ""
    };

    format!(
        r#"## Output Schema

Return exactly one JSON object:

```json
{{
  "batch": "{batch_name}",
  "batch_index": {batch_index},
  "assessments": {{
    "<dimension>": "<0-100 with one decimal>"
  }},
  "dimension_notes": {{
    "<dimension>": {{
      "evidence": ["specific observations"],
      "impact_scope": "local|module|subsystem|codebase",
      "fix_scope": "single_edit|multi_file_refactor|architectural_change",
      "confidence": "high|medium|low",
      "issues_preventing_higher_score": "required when score > 85.0"{sub_axes}
    }}
  }},
  "findings": [{{
    "dimension": "string",
    "identifier": "short_id",
    "summary": "one-line defect summary",
    "related_files": ["relative/path"],
    "evidence": ["code observation"],
    "suggestion": "concrete fix",
    "confidence": "high|medium|low",
    "impact_scope": "local|module|subsystem|codebase",
    "fix_scope": "single_edit|multi_file_refactor|architectural_change",
    "root_cause_cluster": "optional_cluster_name"
  }}],
  "retrospective": {{
    "root_causes": ["optional hypotheses"],
    "likely_symptoms": ["optional identifiers"],
    "possible_false_positives": ["optional mis-scoped concepts"]
  }}
}}
```

Scope enums:
- impact_scope: "local" | "module" | "subsystem" | "codebase"
- fix_scope: "single_edit" | "multi_file_refactor" | "architectural_change""#,
        batch_name = batch.name,
        batch_index = batch.index,
        sub_axes = sub_axes_note,
    )
}

/// Per-language prompt overrides for specific dimensions.
/// Returns additional guidance text to append, or empty string.
pub fn language_override(lang: &str, dimension: &str) -> &'static str {
    match (lang, dimension) {
        ("python", "abstraction_fitness") => {
            "Python override: favor direct modules, explicit domain APIs, \
             and bounded packages over indirection and generic helper surfaces. \
             Look for functions that only forward args/kwargs without policy, \
             protocol/base-class abstractions with one implementation, \
             cross-module wrapper chains, project-wide generic helper reliance, \
             over-broad dict/config/context parameter bags. \
             Skip Django/FastAPI/SQLAlchemy framework boundaries."
        }
        ("typescript", "abstraction_fitness") => {
            "TypeScript override: type and interface layers should improve \
             safety and design clarity, not add ceremony or pass-through indirection. \
             Look for components that only forward props without behavior, \
             interfaces/types with one implementation, large option objects passed through layers, \
             generic helpers with one concrete type, cross-feature wrapper chains, \
             one-implementation interface ecosystems. \
             Skip React/Next.js framework composition patterns."
        }
        _ => "",
    }
}

/// Build a mapping of per-language evidence focus overrides.
pub fn language_evidence_overrides(lang: &str) -> BTreeMap<&'static str, &'static str> {
    let mut overrides = BTreeMap::new();
    let override_text = language_override(lang, "abstraction_fitness");
    if !override_text.is_empty() {
        overrides.insert("abstraction_fitness", override_text);
    }
    overrides
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_batch() -> BatchContext {
        BatchContext {
            name: "Core modules".to_string(),
            rationale: "Review core module quality".to_string(),
            index: 0,
            total: 3,
            dimensions: vec![
                "naming_quality".to_string(),
                "logic_clarity".to_string(),
                "design_coherence".to_string(),
            ],
            files_to_read: vec!["src/core.py".to_string(), "src/utils.py".to_string()],
            historical_issues: vec![],
            concern_signals: vec![],
            holistic_context_json: None,
        }
    }

    #[test]
    fn render_basic_prompt() {
        let reg = DimensionRegistry::new();
        let batch = test_batch();
        let prompt = render_batch_prompt(
            &batch,
            Path::new("/repo"),
            Path::new("/repo/.autoclimb/packet.json"),
            &reg,
        );

        assert!(prompt.contains("Batch: 1 of 3"));
        assert!(prompt.contains("Core modules"));
        assert!(prompt.contains("naming_quality"));
        assert!(prompt.contains("GLOBAL REVIEW CONTRACT"));
        assert!(prompt.contains("src/core.py"));
        assert!(prompt.contains("Output Schema"));
    }

    #[test]
    fn render_with_historical_issues() {
        let reg = DimensionRegistry::new();
        let mut batch = test_batch();
        batch.historical_issues = vec![HistoricalIssue {
            status: "open".to_string(),
            summary: "God class in core.py".to_string(),
            note: Some("Needs splitting".to_string()),
        }];

        let prompt = render_batch_prompt(
            &batch,
            Path::new("/repo"),
            Path::new("/repo/.autoclimb/packet.json"),
            &reg,
        );
        assert!(prompt.contains("Historical Issue Focus"));
        assert!(prompt.contains("God class in core.py"));
        assert!(prompt.contains("Needs splitting"));
    }

    #[test]
    fn render_with_concern_signals() {
        let reg = DimensionRegistry::new();
        let mut batch = test_batch();
        batch.concern_signals = vec![ConcernSignal {
            file: "src/core.py".to_string(),
            concern_type: "complexity".to_string(),
            summary: "High cyclomatic complexity".to_string(),
            question: "Should this be split?".to_string(),
            evidence: vec!["CC=25".to_string()],
        }];

        let prompt = render_batch_prompt(
            &batch,
            Path::new("/repo"),
            Path::new("/repo/.autoclimb/packet.json"),
            &reg,
        );
        assert!(prompt.contains("Mechanical Concern Signals"));
        assert!(prompt.contains("Should this be split?"));
    }

    #[test]
    fn render_includes_workflow_integrity() {
        let reg = DimensionRegistry::new();
        let batch = test_batch(); // has design_coherence
        let prompt = render_batch_prompt(
            &batch,
            Path::new("/repo"),
            Path::new("/repo/.autoclimb/packet.json"),
            &reg,
        );
        assert!(prompt.contains("Workflow Integrity Checks"));
    }

    #[test]
    fn render_with_abstraction_shows_sub_axes() {
        let reg = DimensionRegistry::new();
        let mut batch = test_batch();
        batch.dimensions.push("abstraction_fitness".to_string());

        let prompt = render_batch_prompt(
            &batch,
            Path::new("/repo"),
            Path::new("/repo/.autoclimb/packet.json"),
            &reg,
        );
        assert!(prompt.contains("abstraction_leverage"));
        assert!(prompt.contains("delegation_density"));
    }

    #[test]
    fn python_language_override() {
        let text = language_override("python", "abstraction_fitness");
        assert!(text.contains("Python override"));
    }

    #[test]
    fn unknown_language_no_override() {
        let text = language_override("rust", "abstraction_fitness");
        assert!(text.is_empty());
    }

    #[test]
    fn findings_cap_scales_with_dimensions() {
        let reg = DimensionRegistry::new();
        let mut batch = test_batch();
        // 3 dimensions → cap = max(10, 3) = 10
        let prompt = render_batch_prompt(
            &batch,
            Path::new("/repo"),
            Path::new("/repo/.autoclimb/packet.json"),
            &reg,
        );
        assert!(prompt.contains("0-10 findings"));

        // 15 dimensions → cap = max(10, 15) = 15
        batch.dimensions = reg
            .default_keys()
            .iter()
            .take(15)
            .map(|s| s.to_string())
            .collect();
        let prompt = render_batch_prompt(
            &batch,
            Path::new("/repo"),
            Path::new("/repo/.autoclimb/packet.json"),
            &reg,
        );
        assert!(prompt.contains("0-15 findings"));
    }
}
