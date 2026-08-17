use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use regex::Regex;

use autoclimb_types::enums::{Confidence, Status, Tier, Zone};
use autoclimb_types::finding::Finding;

use crate::context::ScanContext;
use crate::phase::{DetectorPhase, PhaseOutput};

/// Security rule type.
#[derive(Debug, Clone, Copy)]
enum RuleType {
    HardcodedSecret,
    SecretVarName,
    InsecureRandom,
    WeakCrypto,
    SensitiveLogging,
}

struct SecurityRule {
    rule_type: RuleType,
    name: &'static str,
    pattern: Regex,
    tier: Tier,
    base_confidence: Confidence,
}

fn security_rules() -> Vec<SecurityRule> {
    vec![
        // Hardcoded secrets — format patterns
        SecurityRule {
            rule_type: RuleType::HardcodedSecret,
            name: "hardcoded_api_key",
            pattern: Regex::new(r#"(?i)(api[_-]?key|apikey)\s*[:=]\s*["'][a-zA-Z0-9_\-]{20,}"#)
                .unwrap(),
            tier: Tier::QuickFix,
            base_confidence: Confidence::High,
        },
        SecurityRule {
            rule_type: RuleType::HardcodedSecret,
            name: "hardcoded_token",
            pattern: Regex::new(
                r#"(?i)(token|bearer|jwt)\s*[:=]\s*["'][a-zA-Z0-9_\-.]{20,}"#,
            )
            .unwrap(),
            tier: Tier::QuickFix,
            base_confidence: Confidence::High,
        },
        SecurityRule {
            rule_type: RuleType::HardcodedSecret,
            name: "hardcoded_password",
            pattern: Regex::new(r#"(?i)(password|passwd|pwd)\s*[:=]\s*["'][^"']{8,}"#).unwrap(),
            tier: Tier::QuickFix,
            base_confidence: Confidence::Medium,
        },
        // Secret variable names assigned string literals
        SecurityRule {
            rule_type: RuleType::SecretVarName,
            name: "secret_variable",
            pattern: Regex::new(
                r#"(?i)(secret|private[_-]?key|aws[_-]?secret)\s*[:=]\s*["'][^"']+["']"#,
            )
            .unwrap(),
            tier: Tier::QuickFix,
            base_confidence: Confidence::High,
        },
        // Insecure randomness
        SecurityRule {
            rule_type: RuleType::InsecureRandom,
            name: "insecure_random",
            pattern: Regex::new(r"\brandom\.(random|randint|choice|randrange)\b").unwrap(),
            tier: Tier::Judgment,
            base_confidence: Confidence::Low,
        },
        SecurityRule {
            rule_type: RuleType::InsecureRandom,
            name: "math_random",
            pattern: Regex::new(r"\bMath\.random\(\)").unwrap(),
            tier: Tier::Judgment,
            base_confidence: Confidence::Low,
        },
        // Weak crypto / TLS
        SecurityRule {
            rule_type: RuleType::WeakCrypto,
            name: "weak_hash_md5",
            pattern: Regex::new(r"(?i)\b(md5|sha1)\s*\(").unwrap(),
            tier: Tier::Judgment,
            base_confidence: Confidence::Medium,
        },
        SecurityRule {
            rule_type: RuleType::WeakCrypto,
            name: "ssl_verify_disabled",
            pattern: Regex::new(r"(?i)(verify\s*=\s*False|CERT_NONE|SSL_VERIFY_NONE)").unwrap(),
            tier: Tier::QuickFix,
            base_confidence: Confidence::High,
        },
        // Sensitive logging
        SecurityRule {
            rule_type: RuleType::SensitiveLogging,
            name: "logging_sensitive",
            pattern: Regex::new(
                r#"(?i)(log|print|console\.log|logger)\s*\(.*\b(password|secret|token|api[_-]?key)\b"#,
            )
            .unwrap(),
            tier: Tier::QuickFix,
            base_confidence: Confidence::Medium,
        },
        // SQL injection — generic string formatting in SQL
        SecurityRule {
            rule_type: RuleType::WeakCrypto,
            name: "sql_format_string",
            pattern: Regex::new(
                r#"(?i)(execute|query|raw)\s*\(\s*(f"|f'|"[^"]*%|"[^"]*\{|`[^`]*\$\{)"#,
            )
            .unwrap(),
            tier: Tier::QuickFix,
            base_confidence: Confidence::High,
        },
        // eval() usage
        SecurityRule {
            rule_type: RuleType::WeakCrypto,
            name: "eval_usage",
            pattern: Regex::new(r"\beval\s*\(").unwrap(),
            tier: Tier::QuickFix,
            base_confidence: Confidence::High,
        },
        // Subprocess shell injection
        SecurityRule {
            rule_type: RuleType::WeakCrypto,
            name: "shell_injection",
            pattern: Regex::new(
                r"(?i)(subprocess|os\.system|os\.popen|exec|shell_exec)\s*\(",
            )
            .unwrap(),
            tier: Tier::Judgment,
            base_confidence: Confidence::Low,
        },
        // Pickle deserialization
        SecurityRule {
            rule_type: RuleType::WeakCrypto,
            name: "pickle_load",
            pattern: Regex::new(r"\bpickle\.(load|loads)\s*\(").unwrap(),
            tier: Tier::Judgment,
            base_confidence: Confidence::Medium,
        },
        // YAML unsafe load
        SecurityRule {
            rule_type: RuleType::WeakCrypto,
            name: "yaml_unsafe_load",
            pattern: Regex::new(r"\byaml\.(load|unsafe_load)\s*\(").unwrap(),
            tier: Tier::QuickFix,
            base_confidence: Confidence::Medium,
        },
    ]
}

/// Detects security issues via regex-based line scanning.
///
/// 5 rule types: hardcoded secrets, secret variable names, insecure randomness,
/// weak crypto/TLS, sensitive logging. Zone-aware: lower confidence for test files.
pub struct SecurityDetector;

impl DetectorPhase for SecurityDetector {
    fn label(&self) -> &str {
        "security scanning"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let rules = security_rules();
        let mut findings = Vec::new();
        let all_files = &ctx.files;

        for file in all_files {
            let path = root.join(file);
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let zone = ctx.zone_map.get(file);
            let is_test = zone == Zone::Test;

            for (line_num, line) in content.lines().enumerate() {
                // Skip comment lines (basic heuristic)
                let trimmed = line.trim();
                if trimmed.starts_with('#') || trimmed.starts_with("//") || trimmed.starts_with('*')
                {
                    continue;
                }

                for rule in &rules {
                    if !rule.pattern.is_match(line) {
                        continue;
                    }

                    // Lower confidence for test files
                    let confidence = if is_test {
                        Confidence::Low
                    } else {
                        rule.base_confidence
                    };

                    let summary = format!(
                        "Security: {} at line {} — {}",
                        rule.name,
                        line_num + 1,
                        rule_type_label(rule.rule_type),
                    );

                    let detail = serde_json::json!({
                        "rule": rule.name,
                        "rule_type": rule_type_label(rule.rule_type),
                        "line": line_num + 1,
                        "zone": zone.to_string(),
                    });

                    let finding_id = format!("security::{file}::{}_{}", rule.name, line_num + 1);
                    let now = autoclimb_types::newtypes::Timestamp::now();

                    findings.push(Finding {
                        id: finding_id,
                        detector: "security".into(),
                        file: file.clone(),
                        tier: rule.tier,
                        confidence,
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
                        zone: Some(zone.to_string()),
                        extra: BTreeMap::new(),
                    });
                }
            }
        }

        let potential = all_files.len() as u64;
        let mut potentials = BTreeMap::new();
        potentials.insert("security".into(), potential);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

fn rule_type_label(rt: RuleType) -> &'static str {
    match rt {
        RuleType::HardcodedSecret => "hardcoded secret",
        RuleType::SecretVarName => "secret variable name",
        RuleType::InsecureRandom => "insecure randomness",
        RuleType::WeakCrypto => "weak crypto/TLS",
        RuleType::SensitiveLogging => "sensitive logging",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_discovery::zones::{ZoneMap, ZoneRule};
    use std::collections::BTreeSet;

    fn make_context(files: Vec<String>, zone: Zone) -> ScanContext {
        // Build zone map using exact basename patterns for non-production zones
        let rules = if zone != Zone::Production {
            vec![ZoneRule {
                zone,
                // Use basename patterns (exact match) for each file
                patterns: files
                    .iter()
                    .map(|f| f.rsplit('/').next().unwrap_or(f).to_string())
                    .collect(),
            }]
        } else {
            vec![]
        };
        let zone_map = ZoneMap::new(&files, &rules);
        ScanContext {
            lang_name: "python".into(),
            files,
            dep_graph: None,
            zone_map,
            exclusions: vec![],
            entry_patterns: vec![],
            barrel_names: BTreeSet::new(),
            large_threshold: 300,
            complexity_threshold: 20,
        }
    }

    #[test]
    fn detects_hardcoded_api_key() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("config.py"),
            r#"API_KEY = "example_api_token_abcdefghijklmnopqrstuvwxyz1234567890""#,
        )
        .unwrap();

        let ctx = make_context(vec!["config.py".into()], Zone::Production);
        let detector = SecurityDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(!output.findings.is_empty());
        assert!(output.findings[0].summary.contains("hardcoded"));
    }

    #[test]
    fn detects_ssl_verify_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("client.py"), "requests.get(url, verify=False)\n").unwrap();

        let ctx = make_context(vec!["client.py".into()], Zone::Production);
        let detector = SecurityDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(!output.findings.is_empty());
    }

    #[test]
    fn lower_confidence_in_tests() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("test_config.py"),
            r#"API_KEY = "example_test_token_abcdefghijklmnopqrstuvwxyz1234567890""#,
        )
        .unwrap();

        let ctx = make_context(vec!["test_config.py".into()], Zone::Test);
        let detector = SecurityDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(!output.findings.is_empty());
        assert_eq!(output.findings[0].confidence, Confidence::Low);
    }

    #[test]
    fn skips_comments() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("safe.py"),
            "# API_KEY = \"example_api_token_abcdefghijklmnopqrstuvwxyz1234567890\"\n",
        )
        .unwrap();

        let ctx = make_context(vec!["safe.py".into()], Zone::Production);
        let detector = SecurityDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(output.findings.is_empty());
    }

    #[test]
    fn security_rules_compile() {
        let rules = security_rules();
        assert!(rules.len() >= 14);
        for r in &rules {
            let _ = r.pattern.is_match("test");
        }
    }
}
