//! TypeScript code smell detectors.
//!
//! Detects patterns like empty catch blocks, any types, console usage,
//! magic numbers, async without await, and other common issues.

use std::collections::BTreeMap;
use std::path::Path;

use regex::Regex;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};
use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

/// TypeScript smell detector.
pub struct TypeScriptSmellsDetector;

impl DetectorPhase for TypeScriptSmellsDetector {
    fn label(&self) -> &str {
        "TypeScript smells"
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
            findings.extend(detect_smells(&source, file, &now));
        }

        Ok(PhaseOutput {
            findings,
            potentials: BTreeMap::from([("ts_smells".into(), files_scanned)]),
        })
    }
}

/// Pattern-based smell check definition.
struct SmellCheck {
    id: &'static str,
    pattern: Regex,
    summary: &'static str,
    tier: Tier,
    confidence: Confidence,
}

fn smell_checks() -> Vec<SmellCheck> {
    vec![
        SmellCheck {
            id: "empty_catch",
            pattern: Regex::new(r"catch\s*\([^)]*\)\s*\{\s*\}").unwrap(),
            summary: "Empty catch block swallows errors silently",
            tier: Tier::QuickFix,
            confidence: Confidence::High,
        },
        SmellCheck {
            id: "any_type",
            pattern: Regex::new(r":\s*any\b").unwrap(),
            summary: "Explicit `any` type defeats type safety",
            tier: Tier::Judgment,
            confidence: Confidence::High,
        },
        SmellCheck {
            id: "ts_ignore",
            pattern: Regex::new(r"@ts-ignore|@ts-nocheck").unwrap(),
            summary: "TypeScript error suppression bypasses type checking",
            tier: Tier::Judgment,
            confidence: Confidence::High,
        },
        SmellCheck {
            id: "non_null_assert",
            pattern: Regex::new(r"\w+!\.\w+").unwrap(),
            summary: "Non-null assertion operator (!) masks potential null errors",
            tier: Tier::Judgment,
            confidence: Confidence::Medium,
        },
        SmellCheck {
            id: "todo_fixme",
            pattern: Regex::new(r"(?i)//\s*(TODO|FIXME|HACK|XXX)\b").unwrap(),
            summary: "Unresolved TODO/FIXME comment",
            tier: Tier::Judgment,
            confidence: Confidence::Medium,
        },
        SmellCheck {
            id: "magic_number",
            pattern: Regex::new(r"(?:if|while|for|===|!==|>|<|>=|<=)\s*(?:\d{3,}|0x[0-9a-f]{3,})")
                .unwrap(),
            summary: "Magic number in condition — extract to named constant",
            tier: Tier::Judgment,
            confidence: Confidence::Low,
        },
    ]
}

fn detect_smells(source: &str, file: &str, now: &str) -> Vec<Finding> {
    let checks = smell_checks();
    let mut findings = Vec::new();

    for check in &checks {
        let mut match_count = 0;
        let mut first_line = 0;

        for (line_idx, line) in source.lines().enumerate() {
            // Skip comments for most checks, but allow todo_fixme and ts_ignore
            let trimmed = line.trim();
            if (trimmed.starts_with("//") || trimmed.starts_with('*'))
                && check.id != "todo_fixme"
                && check.id != "ts_ignore"
            {
                continue;
            }

            if check.pattern.is_match(line) {
                if match_count == 0 {
                    first_line = line_idx + 1;
                }
                match_count += 1;
            }
        }

        if match_count > 0 {
            findings.push(Finding {
                id: format!("ts_smells::{file}::{}", check.id),
                detector: "ts_smells".into(),
                file: file.to_string(),
                tier: check.tier,
                confidence: check.confidence,
                summary: if match_count > 1 {
                    format!("{} ({} occurrences)", check.summary, match_count)
                } else {
                    check.summary.to_string()
                },
                detail: serde_json::json!({
                    "smell": check.id,
                    "count": match_count,
                    "first_line": first_line,
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
        }
    }

    // Multi-line checks
    findings.extend(detect_async_no_await(source, file, now));
    findings.extend(detect_nested_closures(source, file, now));

    findings
}

/// Detect async functions that never use await.
fn detect_async_no_await(source: &str, file: &str, now: &str) -> Vec<Finding> {
    let async_re = Regex::new(r"async\s+(?:function\s+(\w+)|\(|(\w+)\s*=>)").unwrap();
    let await_re = Regex::new(r"\bawait\b").unwrap();

    let mut findings = Vec::new();

    // Simple heuristic: find async keyword lines and check if await appears nearby
    for (line_idx, line) in source.lines().enumerate() {
        if let Some(cap) = async_re.captures(line) {
            let name = cap
                .get(1)
                .or(cap.get(2))
                .map(|m| m.as_str())
                .unwrap_or("anonymous");

            // Check next 50 lines for await
            let scope_end = (line_idx + 50).min(source.lines().count());
            let scope: String = source
                .lines()
                .skip(line_idx)
                .take(scope_end - line_idx)
                .collect::<Vec<_>>()
                .join("\n");

            if !await_re.is_match(&scope) {
                findings.push(Finding {
                    id: format!("ts_smells::{file}::async_no_await::{name}"),
                    detector: "ts_smells".into(),
                    file: file.to_string(),
                    tier: Tier::QuickFix,
                    confidence: Confidence::Medium,
                    summary: format!("Async function `{name}` never uses await"),
                    detail: serde_json::json!({
                        "smell": "async_no_await",
                        "function": name,
                        "line": line_idx + 1,
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
            }
        }
    }

    findings
}

/// Detect deeply nested closures (3+ levels).
fn detect_nested_closures(source: &str, file: &str, now: &str) -> Vec<Finding> {
    let arrow_re = Regex::new(r"=>\s*\{").unwrap();
    let mut max_depth = 0;
    let mut depth = 0;

    for line in source.lines() {
        let arrows = arrow_re.find_iter(line).count();
        depth += arrows;
        // Simple heuristic: closing braces reduce depth
        let closes = line.matches('}').count();
        depth = depth.saturating_sub(closes);
        max_depth = max_depth.max(depth);
    }

    if max_depth >= 3 {
        vec![Finding {
            id: format!("ts_smells::{file}::nested_closures"),
            detector: "ts_smells".into(),
            file: file.to_string(),
            tier: Tier::Judgment,
            confidence: Confidence::Medium,
            summary: format!(
                "Deeply nested closures ({max_depth} levels) — extract to named functions"
            ),
            detail: serde_json::json!({
                "smell": "nested_closures",
                "max_depth": max_depth,
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
        }]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_empty_catch() {
        let source = "try { foo() } catch (e) { }";
        let findings = detect_smells(source, "test.ts", "2025-01-01");
        assert!(findings.iter().any(|f| f.id.contains("empty_catch")));
    }

    #[test]
    fn detect_any_type() {
        let source = "function foo(x: any): void { }";
        let findings = detect_smells(source, "test.ts", "2025-01-01");
        assert!(findings.iter().any(|f| f.id.contains("any_type")));
    }

    #[test]
    fn detect_ts_ignore() {
        let source = "// @ts-ignore\nconst x = 1;";
        let findings = detect_smells(source, "test.ts", "2025-01-01");
        assert!(findings.iter().any(|f| f.id.contains("ts_ignore")));
    }

    #[test]
    fn detect_todo() {
        let source = "// TODO: fix this later";
        let findings = detect_smells(source, "test.ts", "2025-01-01");
        assert!(findings.iter().any(|f| f.id.contains("todo_fixme")));
    }

    #[test]
    fn clean_code_no_smells() {
        let source = "export function add(a: number, b: number): number { return a + b; }";
        let findings = detect_smells(source, "test.ts", "2025-01-01");
        assert!(findings.is_empty());
    }

    #[test]
    fn async_no_await_detected() {
        let source = "async function fetchData() {\n  return 42;\n}";
        let findings = detect_async_no_await(source, "test.ts", "2025-01-01");
        assert!(findings.iter().any(|f| f.id.contains("async_no_await")));
    }

    #[test]
    fn async_with_await_clean() {
        let source =
            "async function fetchData() {\n  const data = await fetch('/api');\n  return data;\n}";
        let findings = detect_async_no_await(source, "test.ts", "2025-01-01");
        assert!(findings.is_empty());
    }
}
