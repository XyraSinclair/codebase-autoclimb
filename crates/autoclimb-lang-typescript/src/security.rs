//! TypeScript security detectors.
//!
//! Detects eval usage, XSS vectors, hardcoded secrets, innerHTML,
//! dangerouslySetInnerHTML, open redirects, and insecure crypto.

use std::collections::BTreeMap;
use std::path::Path;

use regex::Regex;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};
use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

/// TypeScript-specific security detector (complements the shared SecurityDetector).
pub struct TypeScriptSecurityDetector;

impl DetectorPhase for TypeScriptSecurityDetector {
    fn label(&self) -> &str {
        "TypeScript security"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let now = autoclimb_types::newtypes::Timestamp::now().0;
        let mut findings = Vec::new();
        let mut files_scanned = 0u64;

        for file in ctx.production_files() {
            let full = root.join(file);
            let source = match std::fs::read_to_string(&full) {
                Ok(s) => s,
                Err(_) => continue,
            };
            files_scanned += 1;
            findings.extend(detect_ts_security(&source, file, &now));
        }

        Ok(PhaseOutput {
            findings,
            potentials: BTreeMap::from([("ts_security".into(), files_scanned)]),
        })
    }
}

struct SecurityCheck {
    id: &'static str,
    pattern: Regex,
    summary: &'static str,
    tier: Tier,
    confidence: Confidence,
}

fn security_checks() -> Vec<SecurityCheck> {
    vec![
        SecurityCheck {
            id: "eval_usage",
            pattern: Regex::new(r#"\beval\s*\("#).unwrap(),
            summary: "eval() usage — potential code injection vulnerability",
            tier: Tier::QuickFix,
            confidence: Confidence::High,
        },
        SecurityCheck {
            id: "innerhtml",
            pattern: Regex::new(r"\.innerHTML\s*=").unwrap(),
            summary: "Direct innerHTML assignment — XSS vulnerability",
            tier: Tier::QuickFix,
            confidence: Confidence::High,
        },
        SecurityCheck {
            id: "dangerously_set_innerhtml",
            pattern: Regex::new(r"dangerouslySetInnerHTML").unwrap(),
            summary: "dangerouslySetInnerHTML — ensure input is sanitized",
            tier: Tier::Judgment,
            confidence: Confidence::High,
        },
        SecurityCheck {
            id: "hardcoded_secret",
            pattern: Regex::new(
                r#"(?i)(?:api[_-]?key|secret|token|password|auth)\s*[:=]\s*['"][^'"]{8,}['"]"#,
            )
            .unwrap(),
            summary: "Hardcoded secret or API key — use environment variables",
            tier: Tier::AutoFix,
            confidence: Confidence::Medium,
        },
        SecurityCheck {
            id: "open_redirect",
            pattern: Regex::new(r#"window\.location\s*=\s*[^'";\n]*(?:req|params|query|input)"#)
                .unwrap(),
            summary: "Possible open redirect — user input in window.location",
            tier: Tier::QuickFix,
            confidence: Confidence::Medium,
        },
        SecurityCheck {
            id: "document_write",
            pattern: Regex::new(r#"document\.write\s*\("#).unwrap(),
            summary: "document.write() — XSS risk, use DOM APIs instead",
            tier: Tier::QuickFix,
            confidence: Confidence::High,
        },
        SecurityCheck {
            id: "new_function",
            pattern: Regex::new(r#"new\s+Function\s*\("#).unwrap(),
            summary: "new Function() -- dynamic code execution risk",
            tier: Tier::QuickFix,
            confidence: Confidence::High,
        },
    ]
}

fn detect_ts_security(source: &str, file: &str, now: &str) -> Vec<Finding> {
    let checks = security_checks();
    let mut findings = Vec::new();

    for check in &checks {
        for (line_idx, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            // Skip comments
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }

            if check.pattern.is_match(line) {
                findings.push(Finding {
                    id: format!("ts_security::{file}::{}::{}", check.id, line_idx + 1),
                    detector: "ts_security".into(),
                    file: file.to_string(),
                    tier: check.tier,
                    confidence: check.confidence,
                    summary: check.summary.to_string(),
                    detail: serde_json::json!({
                        "check": check.id,
                        "line": line_idx + 1,
                        "content": trimmed.chars().take(100).collect::<String>(),
                    }),
                    status: Status::Open,
                    note: None,
                    first_seen: now.to_string(),
                    last_seen: now.to_string(),
                    resolved_at: None,
                    reopen_count: 0,
                    suppressed: false,
                    suppressed_at: None,
                    suppression_pattern: None,
                    resolution_attestation: None,
                    lang: Some("typescript".into()),
                    zone: None,
                    extra: BTreeMap::new(),
                });
                break; // One finding per check per file
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_eval() {
        let source = "const result = eval(userInput);";
        let findings = detect_ts_security(source, "test.ts", "2025-01-01");
        assert!(findings.iter().any(|f| f.id.contains("eval_usage")));
    }

    #[test]
    fn detect_innerhtml() {
        let source = "element.innerHTML = userContent;";
        let findings = detect_ts_security(source, "test.ts", "2025-01-01");
        assert!(findings.iter().any(|f| f.id.contains("innerhtml")));
    }

    #[test]
    fn detect_dangerous_react() {
        let source = "<div dangerouslySetInnerHTML={{__html: content}} />";
        let findings = detect_ts_security(source, "test.tsx", "2025-01-01");
        assert!(findings.iter().any(|f| f.id.contains("dangerously")));
    }

    #[test]
    fn detect_hardcoded_key() {
        let source = r#"const API_KEY = "sk-1234567890abcdefghij";"#;
        let findings = detect_ts_security(source, "test.ts", "2025-01-01");
        assert!(findings.iter().any(|f| f.id.contains("hardcoded_secret")));
    }

    #[test]
    fn skip_comments() {
        let source = "// eval(dangerous)";
        let findings = detect_ts_security(source, "test.ts", "2025-01-01");
        assert!(findings.is_empty());
    }

    #[test]
    fn clean_code_no_findings() {
        let source = "const x = document.getElementById('app');\nx.textContent = 'hello';";
        let findings = detect_ts_security(source, "test.ts", "2025-01-01");
        assert!(findings.is_empty());
    }
}
