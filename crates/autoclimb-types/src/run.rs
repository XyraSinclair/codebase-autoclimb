use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[rustfmt::skip]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    pub seen: u64,
    pub total: u64,
    #[serde(default)] pub excluded: Vec<(String, String)>,
}

#[rustfmt::skip]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub repo_id: String,
    pub head: String,
    pub tree: String,
    pub dirty_digest: String,
    pub file_universe: Coverage,
    #[serde(default)] pub tool_versions: BTreeMap<String, String>,
    pub taken_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactKind {
    Observation,
    Contradiction,
    Unknown,
}

#[rustfmt::skip]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    pub id: String,
    pub snapshot: String,
    pub subject: String,
    pub predicate: String,
    pub value: serde_json::Value,
    pub kind: FactKind,
    #[serde(default)] pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub denominator: Option<Coverage>,
}

#[rustfmt::skip]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branch { pub id: String, pub description: String }

#[rustfmt::skip]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DecisionStatus {
    Open,
    Answered { chosen: String, raw_text: String },
    Superseded,
}

#[rustfmt::skip]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    #[serde(default)] pub gates: Vec<Branch>,
    #[serde(default)] pub evidence: Vec<String>,
    pub why_not_inferable: String,
    pub recommendation: String,
    #[serde(default)] pub consequences: Vec<String>,
    pub default_if_delegated: String,
    pub status: DecisionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskClass {
    R0,
    R1,
    R2,
    R3,
    R4,
    R5,
}

#[rustfmt::skip]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget { pub attempts: u32, pub wall_secs: u64, pub subprocesses: u32 }

#[rustfmt::skip]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSet {
    pub purpose: String,
    #[serde(default)] pub non_goals: Vec<String>,
    #[serde(default)] pub allowed_paths: Vec<String>,
    #[serde(default)] pub protected_paths: Vec<String>,
    #[serde(default)] pub compatibility: BTreeMap<String, String>,
    #[serde(default)] pub verifier_commands: Vec<String>,
    pub risk_ceiling: RiskClass,
    pub budget: Budget,
    #[serde(default, skip_serializing_if = "String::is_empty")] pub hash: String,
}

impl RuleSet {
    pub fn canonical_hash(&self) -> String {
        let mut canonical = self.clone();
        canonical.hash.clear();
        let json = serde_json::to_string(&canonical).expect("RuleSet serialization cannot fail");
        hex::encode(Sha256::digest(json.as_bytes()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    Planned,
    Lane,
    Verifying,
    Verified,
    Landed,
    Discarded,
}

#[rustfmt::skip]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub id: String,
    pub thesis: String,
    pub lineage: String,
    pub base: Snapshot,
    pub brief: String,
    pub brief_hash: String,
    #[serde(default)] pub write_set: Vec<String>,
    pub risk_class: RiskClass,
    #[serde(default)] pub predicted: Vec<Fact>,
    pub status: ChangeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Passed,
    Failed,
    Inconclusive,
}

#[rustfmt::skip]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelResult { pub command: String, pub verdict: Verdict, pub output_digest: String }

#[rustfmt::skip]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verification {
    pub change: String,
    pub base_tree: String,
    pub result_tree: String,
    pub patch_hash: String,
    pub ruleset_hash: String,
    #[serde(default)] pub levels_run: Vec<LevelResult>,
    #[serde(default)] pub behaviour_changed: Vec<String>,
    #[serde(default)] pub behaviour_preserved: Vec<String>,
    #[serde(default)] pub residual_uncertainty: Vec<String>,
    #[serde(default)] pub realized: Vec<Fact>,
    pub rollback: String,
}
