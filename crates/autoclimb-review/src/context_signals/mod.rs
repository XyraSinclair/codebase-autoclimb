//! Context signals: enrich review prompts with codebase-level patterns.
//!
//! 3 signal detectors:
//! - AI debt: detects AI-generated code patterns (high comment ratios, excessive logging, guard clauses)
//! - Auth: detects authentication/authorization coverage gaps
//! - Migration: detects in-progress migrations and deprecation patterns

pub mod ai_debt;
pub mod auth;
pub mod migration;

use serde::{Deserialize, Serialize};

/// A context signal detected in the codebase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSignal {
    pub signal_type: SignalType,
    pub severity: SignalSeverity,
    pub message: String,
    pub files: Vec<String>,
    pub detail: serde_json::Value,
}

/// Type of context signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalType {
    AiDebt,
    AuthCoverage,
    Migration,
}

/// Severity of a context signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalSeverity {
    High,
    Medium,
    Low,
}

/// Collect all context signals from file contents.
pub fn collect_signals(file_contents: &[(String, String)]) -> Vec<ContextSignal> {
    let mut signals = Vec::new();
    signals.extend(ai_debt::detect(file_contents));
    signals.extend(auth::detect(file_contents));
    signals.extend(migration::detect(file_contents));
    signals
}
