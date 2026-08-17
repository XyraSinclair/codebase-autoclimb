//! Review result parsing and validation.
//!
//! Extracts JSON payloads from raw batch output, validates structure,
//! and normalizes values (score clamping, confidence normalization).

use std::collections::BTreeMap;

use autoclimb_types::enums::Confidence;

use crate::types::{
    BatchPrompt, BatchResult, BatchStatus, DimensionNote, Provenance, ReviewFinding, ReviewPayload,
    ReviewScope,
};

/// Extract a JSON object from raw batch output.
///
/// Searches for the last valid JSON object in the output text.
/// LLM output may contain markdown, commentary, etc. around the JSON.
pub fn extract_json_payload(raw_output: &str) -> Option<serde_json::Value> {
    // Try the entire output as JSON first
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw_output) {
        if v.is_object() {
            return Some(v);
        }
    }

    // Search for JSON blocks in the output
    // Strategy: find matching { } pairs, try parsing each
    let mut best: Option<serde_json::Value> = None;

    let chars: Vec<char> = raw_output.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '{' {
            // Find the matching closing brace
            let mut depth = 0;
            let mut j = i;
            let mut in_string = false;
            let mut escape = false;

            while j < chars.len() {
                let c = chars[j];

                if escape {
                    escape = false;
                } else if c == '\\' && in_string {
                    escape = true;
                } else if c == '"' {
                    in_string = !in_string;
                } else if !in_string {
                    if c == '{' {
                        depth += 1;
                    } else if c == '}' {
                        depth -= 1;
                        if depth == 0 {
                            // Try parsing this substring
                            let candidate: String = chars[i..=j].iter().collect();
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&candidate) {
                                if v.is_object() {
                                    // Prefer objects with "assessments" key
                                    if v.get("assessments").is_some() {
                                        return Some(v);
                                    }
                                    best = Some(v);
                                }
                            }
                            break;
                        }
                    }
                }
                j += 1;
            }
        }
        i += 1;
    }

    best
}

/// Validate and normalize a parsed JSON payload into a ReviewPayload.
pub fn parse_review_payload(
    json: &serde_json::Value,
    allowed_dimensions: &[String],
    runner: &str,
    batch_index: usize,
) -> Result<ReviewPayload, ParseError> {
    let obj = json.as_object().ok_or(ParseError::NotAnObject)?;

    // Parse assessments
    let assessments_val = obj
        .get("assessments")
        .ok_or(ParseError::MissingField("assessments"))?;
    let assessments = parse_assessments(assessments_val, allowed_dimensions)?;

    // Parse findings
    let empty_arr = serde_json::Value::Array(vec![]);
    let findings_val = obj.get("findings").unwrap_or(&empty_arr);
    let findings = parse_findings(findings_val, allowed_dimensions)?;

    // Parse dimension notes
    let dimension_notes = parse_dimension_notes(
        obj.get("dimension_notes")
            .unwrap_or(&serde_json::Value::Null),
    );

    // Parse reviewed files from batch context
    let reviewed_files: Vec<String> = findings
        .iter()
        .flat_map(|f| f.related_files.iter())
        .cloned()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let now = autoclimb_types::newtypes::Timestamp::now();

    Ok(ReviewPayload {
        findings,
        assessments,
        reviewed_files,
        review_scope: ReviewScope::Batch {
            index: batch_index,
            total: 0,
        },
        dimension_notes,
        provenance: Provenance {
            runner: runner.to_string(),
            model: None,
            timestamp: now.0,
            batch_count: 1,
            session_id: None,
        },
    })
}

/// Parse a successful raw batch result into a structured payload in place.
pub fn attach_parsed_payload(
    result: &mut BatchResult,
    prompt: &BatchPrompt,
    allowed_dimensions: &[String],
    runner: &str,
) -> Result<(), String> {
    if result.status != BatchStatus::Success {
        return Err("batch did not complete successfully".to_string());
    }

    if result.payload.is_some() {
        return Ok(());
    }

    let json = extract_json_payload(&result.raw_output)
        .ok_or_else(|| "No JSON payload found in runner output".to_string())?;
    let mut payload = parse_review_payload(&json, allowed_dimensions, runner, prompt.index)
        .map_err(|e| format!("invalid review payload for batch {}: {e}", prompt.index + 1))?;
    payload.review_scope = ReviewScope::Batch {
        index: prompt.index,
        total: prompt.total,
    };
    payload.provenance.batch_count = prompt.total;
    result.payload = Some(payload);
    Ok(())
}

fn parse_assessments(
    val: &serde_json::Value,
    allowed: &[String],
) -> Result<BTreeMap<String, f64>, ParseError> {
    let obj = val.as_object().ok_or(ParseError::InvalidAssessments)?;
    let mut result = BTreeMap::new();

    for (key, score_val) in obj {
        // Filter to allowed dimensions
        if !allowed.is_empty() && !allowed.iter().any(|d| d == key) {
            continue;
        }

        let score = match score_val {
            serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
            serde_json::Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
            _ => continue,
        };

        // Clamp to 0-100
        result.insert(key.clone(), score.clamp(0.0, 100.0));
    }

    Ok(result)
}

fn parse_findings(
    val: &serde_json::Value,
    allowed: &[String],
) -> Result<Vec<ReviewFinding>, ParseError> {
    let arr = match val.as_array() {
        Some(a) => a,
        None => return Ok(Vec::new()),
    };

    let mut findings = Vec::new();

    for item in arr {
        let obj = match item.as_object() {
            Some(o) => o,
            None => continue,
        };

        let dimension = obj
            .get("dimension")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Filter to allowed dimensions
        if !allowed.is_empty() && !allowed.iter().any(|d| d == &dimension) {
            continue;
        }

        let identifier = obj
            .get("identifier")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let summary = obj
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let confidence = match obj
            .get("confidence")
            .and_then(|v| v.as_str())
            .unwrap_or("medium")
        {
            "high" => Confidence::High,
            "low" => Confidence::Low,
            _ => Confidence::Medium,
        };

        let suggestion = obj
            .get("suggestion")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let related_files: Vec<String> = obj
            .get("related_files")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let evidence: Vec<String> = obj
            .get("evidence")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let impact_scope = obj
            .get("impact_scope")
            .and_then(|v| v.as_str())
            .unwrap_or("local")
            .to_string();

        let fix_scope = obj
            .get("fix_scope")
            .and_then(|v| v.as_str())
            .unwrap_or("single_edit")
            .to_string();

        let concern_verdict = obj
            .get("concern_verdict")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let concern_fingerprint = obj
            .get("concern_fingerprint")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        findings.push(ReviewFinding {
            dimension,
            identifier,
            summary,
            confidence,
            suggestion,
            related_files,
            evidence,
            impact_scope,
            fix_scope,
            concern_verdict,
            concern_fingerprint,
        });
    }

    Ok(findings)
}

fn parse_dimension_notes(val: &serde_json::Value) -> BTreeMap<String, DimensionNote> {
    let mut notes = BTreeMap::new();

    let obj = match val.as_object() {
        Some(o) => o,
        None => return notes,
    };

    for (key, note_val) in obj {
        let note_obj = match note_val.as_object() {
            Some(o) => o,
            None => continue,
        };

        let note_text = note_obj
            .get("issues_preventing_higher_score")
            .or_else(|| note_obj.get("evidence"))
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Array(a) => a
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
                _ => String::new(),
            })
            .unwrap_or_default();

        let score_adjustment = note_obj.get("score_adjustment").and_then(|v| v.as_f64());

        notes.insert(
            key.clone(),
            DimensionNote {
                dimension: key.clone(),
                note: note_text,
                score_adjustment,
            },
        );
    }

    notes
}

/// Errors during result parsing.
#[derive(Debug)]
pub enum ParseError {
    NotAnObject,
    MissingField(&'static str),
    InvalidAssessments,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::NotAnObject => write!(f, "JSON payload is not an object"),
            ParseError::MissingField(name) => write!(f, "Missing required field: {name}"),
            ParseError::InvalidAssessments => write!(f, "Invalid assessments format"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_from_pure_json() {
        let raw = r#"{"assessments": {"naming_quality": 85.0}, "findings": []}"#;
        let json = extract_json_payload(raw).unwrap();
        assert!(json.get("assessments").is_some());
    }

    #[test]
    fn extract_json_from_markdown() {
        let raw = r#"Here's my review:

```json
{"assessments": {"naming_quality": 85.0}, "findings": []}
```

That's my assessment."#;
        let json = extract_json_payload(raw).unwrap();
        assert!(json.get("assessments").is_some());
    }

    #[test]
    fn extract_json_with_surrounding_text() {
        let raw = r#"I've reviewed the code. {"assessments": {"logic_clarity": 90.0}, "findings": []} Done."#;
        let json = extract_json_payload(raw).unwrap();
        assert!(json.get("assessments").is_some());
    }

    #[test]
    fn extract_no_json() {
        let raw = "No JSON here at all.";
        assert!(extract_json_payload(raw).is_none());
    }

    #[test]
    fn parse_valid_payload() {
        let json = serde_json::json!({
            "assessments": {
                "naming_quality": 85.5,
                "logic_clarity": 92.0
            },
            "findings": [{
                "dimension": "naming_quality",
                "identifier": "generic_name_handle",
                "summary": "handle_data is too generic",
                "related_files": ["src/main.py"],
                "evidence": ["handle_data processes 5 different types"],
                "suggestion": "Rename to process_user_input",
                "confidence": "high",
                "impact_scope": "module",
                "fix_scope": "single_edit"
            }],
            "dimension_notes": {
                "naming_quality": {
                    "issues_preventing_higher_score": "Several generic names remain",
                    "evidence": ["handle_data", "do_thing"],
                    "confidence": "high"
                }
            }
        });

        let allowed = vec!["naming_quality".to_string(), "logic_clarity".to_string()];
        let payload = parse_review_payload(&json, &allowed, "codex", 0).unwrap();
        assert_eq!(payload.assessments.len(), 2);
        assert_eq!(payload.findings.len(), 1);
        assert_eq!(payload.findings[0].dimension, "naming_quality");
        assert!(!payload.dimension_notes.is_empty());
    }

    #[test]
    fn scores_clamped() {
        let json = serde_json::json!({
            "assessments": {
                "naming_quality": 150.0,
                "logic_clarity": -10.0
            },
            "findings": []
        });

        let payload = parse_review_payload(&json, &[], "codex", 0).unwrap();
        assert_eq!(payload.assessments["naming_quality"], 100.0);
        assert_eq!(payload.assessments["logic_clarity"], 0.0);
    }

    #[test]
    fn unknown_dimensions_filtered() {
        let json = serde_json::json!({
            "assessments": {
                "naming_quality": 85.0,
                "unknown_dim": 50.0
            },
            "findings": [{
                "dimension": "unknown_dim",
                "identifier": "x",
                "summary": "x",
                "confidence": "high"
            }]
        });

        let allowed = vec!["naming_quality".to_string()];
        let payload = parse_review_payload(&json, &allowed, "codex", 0).unwrap();
        assert_eq!(payload.assessments.len(), 1);
        assert!(payload.assessments.contains_key("naming_quality"));
        assert!(payload.findings.is_empty()); // unknown_dim filtered
    }

    #[test]
    fn string_scores_parsed() {
        let json = serde_json::json!({
            "assessments": {
                "naming_quality": "85.5"
            },
            "findings": []
        });

        let payload = parse_review_payload(&json, &[], "codex", 0).unwrap();
        assert!((payload.assessments["naming_quality"] - 85.5).abs() < f64::EPSILON);
    }

    #[test]
    fn attach_payload_updates_scope_and_payload() {
        let prompt = BatchPrompt {
            index: 1,
            total: 3,
            files: vec!["src/lib.rs".into()],
            prompt: "review".into(),
        };
        let mut result = BatchResult {
            index: 1,
            status: BatchStatus::Success,
            payload: None,
            raw_output: r#"{"assessments":{"naming_quality":81.0},"findings":[]}"#.into(),
            elapsed_secs: 1.0,
        };

        attach_parsed_payload(
            &mut result,
            &prompt,
            &["naming_quality".to_string()],
            "codex",
        )
        .expect("attach payload");

        let payload = result.payload.expect("payload attached");
        match payload.review_scope {
            ReviewScope::Batch { index, total } => {
                assert_eq!(index, 1);
                assert_eq!(total, 3);
            }
            _ => panic!("expected batch scope"),
        }
        assert_eq!(payload.provenance.runner, "codex");
        assert_eq!(payload.provenance.batch_count, 3);
    }

    #[test]
    fn attach_payload_rejects_non_json_output() {
        let prompt = BatchPrompt {
            index: 0,
            total: 1,
            files: vec![],
            prompt: "review".into(),
        };
        let mut result = BatchResult {
            index: 0,
            status: BatchStatus::Success,
            payload: None,
            raw_output: "not json".into(),
            elapsed_secs: 1.0,
        };

        let err = attach_parsed_payload(&mut result, &prompt, &[], "codex")
            .expect_err("non-json output should fail");
        assert!(err.contains("No JSON payload"));
    }
}
