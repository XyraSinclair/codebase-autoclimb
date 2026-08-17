use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ── Phase ────────────────────────────────────────────────

/// Project health phase, determined from scan history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    FirstScan,
    Regression,
    Stagnation,
    EarlyMomentum,
    MiddleGrind,
    Refinement,
    Maintenance,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::FirstScan => "First scan",
            Phase::Regression => "Regression",
            Phase::Stagnation => "Stagnation",
            Phase::EarlyMomentum => "Early momentum",
            Phase::MiddleGrind => "Middle grind",
            Phase::Refinement => "Refinement",
            Phase::Maintenance => "Maintenance",
        }
    }
}

// ── Action types ─────────────────────────────────────────

/// Priority ordering for action types (lower = do first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrativeActionType {
    AutoFix = 0,
    Reorganize = 1,
    Refactor = 2,
    ManualFix = 3,
    DebtReview = 4,
}

impl NarrativeActionType {
    pub fn label(self) -> &'static str {
        match self {
            NarrativeActionType::AutoFix => "auto-fix",
            NarrativeActionType::Reorganize => "reorganize",
            NarrativeActionType::Refactor => "refactor",
            NarrativeActionType::ManualFix => "manual fix",
            NarrativeActionType::DebtReview => "debt review",
        }
    }
}

// ── ActionItem ───────────────────────────────────────────

/// A single recommended action for the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub action_type: NarrativeActionType,
    pub finding_id: String,
    pub file: String,
    pub detector: String,
    pub summary: String,
    pub impact: f64,
}

// ── Lane ─────────────────────────────────────────────────

/// A parallelizable work lane (group of actions with no file overlap).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    Cleanup,
    Restructure,
    Refactor,
    Debt,
}

impl Lane {
    pub fn label(self) -> &'static str {
        match self {
            Lane::Cleanup => "cleanup",
            Lane::Restructure => "restructure",
            Lane::Refactor => "refactor",
            Lane::Debt => "debt",
        }
    }
}

// ── Strategy ─────────────────────────────────────────────

/// Strategic recommendation for the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyResult {
    pub fixer_leverage: f64,
    pub lanes: Vec<LaneInfo>,
    pub parallelizable: bool,
    pub recommendation: String,
}

/// Info about a single work lane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaneInfo {
    pub lane: Lane,
    pub files: Vec<String>,
    pub action_count: usize,
}

// ── Dimension analysis ───────────────────────────────────

/// Per-dimension narrative analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionAnalysis {
    pub name: String,
    pub score: f64,
    pub change: f64,
    pub issues: u64,
    pub checks: u64,
    pub status: DimensionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionStatus {
    Improving,
    Stable,
    Declining,
    New,
}

// ── Debt analysis ────────────────────────────────────────

/// Technical debt summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebtAnalysis {
    pub total_open: u64,
    pub wontfix_count: u64,
    pub chronic_count: u64,
    pub oldest_open_days: u64,
}

// ── Milestone ────────────────────────────────────────────

/// Notable achievement detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Milestone {
    Crossed90Strict,
    Crossed80Strict,
    AllT1T2Cleared,
    ZeroOpenFindings,
}

impl Milestone {
    pub fn message(self) -> &'static str {
        match self {
            Milestone::Crossed90Strict => "Crossed 90% strict score",
            Milestone::Crossed80Strict => "Crossed 80% strict score",
            Milestone::AllT1T2Cleared => "All T1 and T2 findings cleared",
            Milestone::ZeroOpenFindings => "Zero open findings",
        }
    }
}

// ── Reminder ─────────────────────────────────────────────

/// Reminder types that can be shown to the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderType {
    RunReview,
    CheckFixedFindings,
    AddressWontfix,
    ReduceSuppressed,
    ReviewChronic,
    UpdatePlan,
    CheckCoverage,
    AddressRegression,
    ClearAutoFix,
    ReviewSecurity,
    CheckDuplicates,
    AddressSmells,
    CheckStructural,
    ReviewUnused,
    AddressNaming,
    CheckTestHealth,
    ReviewCycles,
    AddressCoupling,
    CheckExports,
}

/// Tracked reminder with decay counter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderEntry {
    pub reminder_type: ReminderType,
    pub count: u32,
    pub last_shown: String,
}

/// Maximum consecutive occurrences before suppressing a reminder.
pub const REMINDER_DECAY_THRESHOLD: u32 = 3;

// ── Risk flag ────────────────────────────────────────────

/// A risk flag highlighting a concern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFlag {
    pub severity: RiskSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskSeverity {
    High,
    Medium,
    Low,
}

// ── NarrativeResult ──────────────────────────────────────

/// The complete narrative output from the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeResult {
    pub phase: Phase,
    pub headline: String,
    pub dimensions: Vec<DimensionAnalysis>,
    pub actions: Vec<ActionItem>,
    pub strategy: StrategyResult,
    pub debt: DebtAnalysis,
    pub milestones: Vec<Milestone>,
    pub primary_action: Option<ActionItem>,
    pub why_now: Option<String>,
    pub risk_flags: Vec<RiskFlag>,
    pub strict_target: f64,
    pub reminders: Vec<ReminderEntry>,
}

// ── NarrativeInput ───────────────────────────────────────

/// Input bundle for the narrative engine.
pub struct NarrativeInput<'a> {
    pub findings: &'a BTreeMap<String, autoclimb_types::finding::Finding>,
    pub potentials: &'a BTreeMap<String, u64>,
    pub dimension_scores: &'a BTreeMap<String, autoclimb_types::scoring::DimensionScoreEntry>,
    pub strict_score: f64,
    pub overall_score: f64,
    pub scan_count: u32,
    pub scan_history: &'a [autoclimb_types::scoring::ScanHistoryEntry],
    pub prev_strict_score: Option<f64>,
    pub prev_dimension_scores:
        Option<&'a BTreeMap<String, autoclimb_types::scoring::DimensionScoreEntry>>,
}
