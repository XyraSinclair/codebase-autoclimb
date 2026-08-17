use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use regex::Regex;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use crate::context::ScanContext;
use crate::phase::{DetectorPhase, PhaseOutput};

/// A pluggable signal contributing to a file's complexity score.
pub struct ComplexitySignal {
    pub name: &'static str,
    pub pattern: Option<Regex>,
    pub weight: f64,
    pub threshold: u32,
}

/// Detects high-complexity files via pluggable complexity signals.
///
/// Fires when aggregate score >= the context's complexity threshold AND
/// the file has >= 50 lines (min_loc).
pub struct ComplexityDetector {
    pub signals: Vec<ComplexitySignal>,
    pub min_loc: u32,
}

impl Default for ComplexityDetector {
    fn default() -> Self {
        Self {
            signals: default_signals(),
            min_loc: 50,
        }
    }
}

fn default_signals() -> Vec<ComplexitySignal> {
    vec![
        ComplexitySignal {
            name: "nested_control",
            pattern: Some(Regex::new(r"^(\s{12,})(if |for |while |elif )").unwrap()),
            weight: 1.5,
            threshold: 3,
        },
        ComplexitySignal {
            name: "long_functions",
            pattern: Some(Regex::new(r"^\s{0,4}(def |fn |func |function )").unwrap()),
            weight: 0.5,
            threshold: 0, // counted, weighted by function count vs loc
        },
        ComplexitySignal {
            name: "boolean_chains",
            pattern: Some(Regex::new(r"\b(and|or|&&|\|\|)\b.*\b(and|or|&&|\|\|)\b").unwrap()),
            weight: 2.0,
            threshold: 2,
        },
        ComplexitySignal {
            name: "exception_handlers",
            pattern: Some(Regex::new(r"^\s*(except|catch|rescue)\b").unwrap()),
            weight: 1.0,
            threshold: 5,
        },
    ]
}

impl DetectorPhase for ComplexityDetector {
    fn label(&self) -> &str {
        "complexity analysis"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let production = ctx.production_files();
        let mut findings = Vec::new();
        let threshold = ctx.complexity_threshold;

        for file in &production {
            let path = root.join(file);
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let loc = content.lines().count() as u32;
            if loc < self.min_loc {
                continue;
            }

            let mut total_score: f64 = 0.0;
            let mut signal_hits: Vec<(&str, u32)> = Vec::new();

            for signal in &self.signals {
                let count = match &signal.pattern {
                    Some(re) => content.lines().filter(|l| re.is_match(l)).count() as u32,
                    None => 0,
                };

                if signal.threshold > 0 && count >= signal.threshold {
                    total_score += count as f64 * signal.weight;
                    signal_hits.push((signal.name, count));
                } else if signal.threshold == 0 {
                    total_score += count as f64 * signal.weight;
                    if count > 0 {
                        signal_hits.push((signal.name, count));
                    }
                }
            }

            if total_score >= threshold as f64 {
                let summary = format!(
                    "High complexity score {:.0} (threshold: {threshold}) in {loc} lines",
                    total_score,
                );

                let detail = serde_json::json!({
                    "complexity_score": total_score,
                    "threshold": threshold,
                    "loc": loc,
                    "signals": signal_hits.iter().map(|(name, count)| {
                        serde_json::json!({"signal": name, "count": count})
                    }).collect::<Vec<_>>(),
                });

                let finding_id = format!("structural::{file}::complexity");
                let now = autoclimb_types::newtypes::Timestamp::now();

                findings.push(Finding {
                    id: finding_id,
                    detector: "structural".into(),
                    file: file.to_string(),
                    tier: Tier::Judgment,
                    confidence: Confidence::Medium,
                    summary,
                    detail,
                    status: Status::Open,
                    note: None,
                    first_seen: now.0.clone(),
                    last_seen: now.0,
                    resolved_at: None,
                    reopen_count: 0,
                    suppressed: false,
                    suppressed_at: None,
                    suppression_pattern: None,
                    resolution_attestation: None,
                    lang: Some(ctx.lang_name.clone()),
                    zone: Some(ctx.zone_map.get(file).to_string()),
                    extra: BTreeMap::new(),
                });
            }
        }

        let potential = production.len() as u64;
        let mut potentials = BTreeMap::new();
        potentials.insert("structural".into(), potential);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_signals_compile() {
        let signals = default_signals();
        assert!(!signals.is_empty());
        for s in &signals {
            if let Some(ref re) = s.pattern {
                // Just verify the regex compiled
                let _ = re.is_match("test");
            }
        }
    }

    #[test]
    fn classify_nested_control() {
        let re = Regex::new(r"^(\s{12,})(if |for |while |elif )").unwrap();
        assert!(re.is_match("            if something:"));
        assert!(!re.is_match("    if something:"));
    }

    #[test]
    fn classify_boolean_chains() {
        let re = Regex::new(r"\b(and|or|&&|\|\|)\b.*\b(and|or|&&|\|\|)\b").unwrap();
        assert!(re.is_match("if a and b or c:"));
        assert!(!re.is_match("if a and b:"));
    }
}
