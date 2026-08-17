//! Review packet preparation.
//!
//! Creates immutable review packets that drive batch execution.
//! Once created, a packet captures a snapshot of scan state and
//! review configuration for reproducible review sessions.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use autoclimb_types::state::StateModel;

use crate::context;
use crate::context::budget;
use crate::context::mechanical;
use crate::context::selection::{self, SelectionConfig};
use crate::dimensions::selection as dim_selection;
use crate::dimensions::DimensionRegistry;
use crate::prompt_template::BatchContext;

/// An immutable review packet — snapshot of state + review config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewPacket {
    /// Packet version.
    pub version: u32,
    /// When the packet was created.
    pub created: String,
    /// Investigation batches.
    pub batches: Vec<BatchSpec>,
    /// Score snapshot at packet creation time.
    pub score_snapshot: ScoreSnapshot,
    /// Next recommended command.
    pub next_command: Option<String>,
}

/// Specification for a single review batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSpec {
    /// Batch name.
    pub name: String,
    /// Why this batch exists.
    pub rationale: String,
    /// Files the reviewer should start with.
    pub files_to_read: Vec<String>,
    /// Dimensions to assess in this batch.
    pub dimensions: Vec<String>,
    /// Concern signals for this batch.
    pub concern_signals: Vec<ConcernSignalSpec>,
    /// Evidence focus hints.
    pub evidence_focus: BTreeMap<String, String>,
    /// Holistic context JSON.
    pub holistic_context: Option<serde_json::Value>,
}

/// Concern signal in a batch spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcernSignalSpec {
    pub file: String,
    pub concern_type: String,
    pub summary: String,
    pub question: String,
    pub evidence: Vec<String>,
}

/// Score snapshot at packet creation time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreSnapshot {
    pub overall: f64,
    pub objective: f64,
    pub strict: f64,
    pub verified_strict: f64,
}

/// Options for packet preparation.
#[derive(Debug, Clone)]
pub struct PrepareOptions {
    /// Explicit dimension list (None = defaults).
    pub dimensions: Option<Vec<String>>,
    /// Maximum files per batch.
    pub max_files_per_batch: usize,
    /// Whether to include historical issue context.
    pub retrospective: bool,
    /// Language for per-language guidance.
    pub lang: Option<String>,
}

impl Default for PrepareOptions {
    fn default() -> Self {
        Self {
            dimensions: None,
            max_files_per_batch: 15,
            retrospective: false,
            lang: None,
        }
    }
}

/// Prepare a holistic review packet from scan state.
pub fn prepare_review_packet(
    state: &StateModel,
    _project_root: &Path,
    opts: &PrepareOptions,
) -> ReviewPacket {
    let registry = DimensionRegistry::new();

    // Select dimensions
    let dimensions = dim_selection::select_dimensions(&registry, opts.dimensions.as_deref());

    // Select files for review
    let selection_config = SelectionConfig {
        max_files_per_batch: opts.max_files_per_batch,
        ..SelectionConfig::default()
    };
    let candidates = selection::select_review_files(&state.findings, &selection_config);

    // Group files into batches
    let file_groups = selection::group_into_batches(&candidates, opts.max_files_per_batch);

    // Build holistic context
    let mut holistic_ctx =
        context::build_holistic_context(&state.findings, state.dimension_scores.as_ref());
    budget::trim_to_budget(&mut holistic_ctx, budget::DEFAULT_BUDGET_CHARS);
    let ctx_json = serde_json::to_value(&holistic_ctx).ok();

    // Synthesize concern signals
    let concern_signals = mechanical::synthesize_concern_signals(&state.findings, 8);

    // Build batch specs
    let batches: Vec<BatchSpec> = if file_groups.is_empty() {
        // Even with no files, create one batch for the dimensions
        vec![BatchSpec {
            name: "Full review".to_string(),
            rationale: "Holistic review of project quality".to_string(),
            files_to_read: Vec::new(),
            dimensions: dimensions.clone(),
            concern_signals: concern_signals
                .iter()
                .map(|s| ConcernSignalSpec {
                    file: s.file.clone(),
                    concern_type: s.concern_type.clone(),
                    summary: s.summary.clone(),
                    question: s.question.clone(),
                    evidence: s.evidence.clone(),
                })
                .collect(),
            evidence_focus: BTreeMap::new(),
            holistic_context: ctx_json.clone(),
        }]
    } else {
        file_groups
            .iter()
            .enumerate()
            .map(|(i, files)| {
                // Distribute concern signals to relevant batches
                let batch_signals: Vec<ConcernSignalSpec> = concern_signals
                    .iter()
                    .filter(|s| files.contains(&s.file))
                    .map(|s| ConcernSignalSpec {
                        file: s.file.clone(),
                        concern_type: s.concern_type.clone(),
                        summary: s.summary.clone(),
                        question: s.question.clone(),
                        evidence: s.evidence.clone(),
                    })
                    .collect();

                BatchSpec {
                    name: format!("Batch {}", i + 1),
                    rationale: format!("Review {} files with highest finding density", files.len()),
                    files_to_read: files.clone(),
                    dimensions: dimensions.clone(),
                    concern_signals: batch_signals,
                    evidence_focus: BTreeMap::new(),
                    holistic_context: if i == 0 { ctx_json.clone() } else { None },
                }
            })
            .collect()
    };

    let now = autoclimb_types::newtypes::Timestamp::now();

    ReviewPacket {
        version: 1,
        created: now.0,
        batches,
        score_snapshot: ScoreSnapshot {
            overall: state.overall_score,
            objective: state.objective_score,
            strict: state.strict_score,
            verified_strict: state.verified_strict_score,
        },
        next_command: None,
    }
}

/// Convert a BatchSpec to a BatchContext for prompt rendering.
pub fn batch_spec_to_context(spec: &BatchSpec, index: usize, total: usize) -> BatchContext {
    BatchContext {
        name: spec.name.clone(),
        rationale: spec.rationale.clone(),
        index,
        total,
        dimensions: spec.dimensions.clone(),
        files_to_read: spec.files_to_read.clone(),
        historical_issues: Vec::new(),
        concern_signals: spec
            .concern_signals
            .iter()
            .map(|s| crate::prompt_template::ConcernSignal {
                file: s.file.clone(),
                concern_type: s.concern_type.clone(),
                summary: s.summary.clone(),
                question: s.question.clone(),
                evidence: s.evidence.clone(),
            })
            .collect(),
        holistic_context_json: spec
            .holistic_context
            .as_ref()
            .and_then(|v| serde_json::to_string_pretty(v).ok()),
    }
}

/// Generate a blind variant of the packet (scores redacted).
pub fn make_blind_packet(packet: &ReviewPacket) -> ReviewPacket {
    let mut blind = packet.clone();
    blind.score_snapshot = ScoreSnapshot {
        overall: 0.0,
        objective: 0.0,
        strict: 0.0,
        verified_strict: 0.0,
    };
    blind
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::finding::Finding;

    #[test]
    fn prepare_empty_state() {
        let state = StateModel::empty();
        let packet = prepare_review_packet(&state, Path::new("/repo"), &PrepareOptions::default());
        assert_eq!(packet.version, 1);
        assert!(!packet.batches.is_empty());
        assert!(!packet.batches[0].dimensions.is_empty());
    }

    #[test]
    fn prepare_with_findings() {
        let mut state = StateModel::empty();
        for i in 0..5 {
            let f = Finding {
                id: format!("smells::src/f{i}.py"),
                detector: "smells".to_string(),
                file: format!("src/f{i}.py"),
                tier: autoclimb_types::enums::Tier::Judgment,
                confidence: autoclimb_types::enums::Confidence::High,
                summary: "test".to_string(),
                detail: serde_json::json!({}),
                status: autoclimb_types::enums::Status::Open,
                note: None,
                first_seen: String::new(),
                last_seen: String::new(),
                resolved_at: None,
                reopen_count: 0,
                suppressed: false,
                suppressed_at: None,
                suppression_pattern: None,
                resolution_attestation: None,
                lang: None,
                zone: None,
                extra: BTreeMap::new(),
            };
            state.findings.insert(f.id.clone(), f);
        }

        let packet = prepare_review_packet(&state, Path::new("/repo"), &PrepareOptions::default());
        assert!(!packet.batches.is_empty());
        // Should have files in the first batch
        assert!(!packet.batches[0].files_to_read.is_empty());
    }

    #[test]
    fn blind_packet_redacts_scores() {
        let state = StateModel::empty();
        let packet = prepare_review_packet(&state, Path::new("/repo"), &PrepareOptions::default());
        let blind = make_blind_packet(&packet);
        assert_eq!(blind.score_snapshot.overall, 0.0);
        assert_eq!(blind.score_snapshot.strict, 0.0);
    }

    #[test]
    fn batch_spec_converts_to_context() {
        let spec = BatchSpec {
            name: "Test batch".to_string(),
            rationale: "test".to_string(),
            files_to_read: vec!["src/a.py".to_string()],
            dimensions: vec!["naming_quality".to_string()],
            concern_signals: Vec::new(),
            evidence_focus: BTreeMap::new(),
            holistic_context: None,
        };

        let ctx = batch_spec_to_context(&spec, 0, 1);
        assert_eq!(ctx.name, "Test batch");
        assert_eq!(ctx.index, 0);
        assert_eq!(ctx.total, 1);
    }

    #[test]
    fn packet_serializes() {
        let state = StateModel::empty();
        let packet = prepare_review_packet(&state, Path::new("/repo"), &PrepareOptions::default());
        let json = serde_json::to_string_pretty(&packet).unwrap();
        assert!(json.contains("batches"));
        assert!(json.contains("score_snapshot"));
    }
}
