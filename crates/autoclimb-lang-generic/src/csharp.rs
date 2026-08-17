//! C#-specific security detector.
//!
//! Detects C#-specific security patterns beyond the generic security detector:
//! - SQL injection via string concatenation in SqlCommand
//! - BinaryFormatter/SoapFormatter deserialization
//! - Disabled TLS certificate validation
//! - Insecure RNG in security-sensitive contexts

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use regex::Regex;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};

struct CSharpSecurityRule {
    name: &'static str,
    pattern: Regex,
    tier: Tier,
    confidence: Confidence,
    summary_template: &'static str,
}

fn csharp_security_rules() -> Vec<CSharpSecurityRule> {
    vec![
        // SQL injection: SqlCommand with string interpolation/concatenation
        CSharpSecurityRule {
            name: "sql_injection",
            pattern: Regex::new(r#"(?i)new\s+SqlCommand\s*\(\s*(\$"|"[^"]*"\s*\+|string\.Format)"#)
                .unwrap(),
            tier: Tier::QuickFix,
            confidence: Confidence::High,
            summary_template: "SQL injection risk: SqlCommand with string concatenation",
        },
        CSharpSecurityRule {
            name: "sql_injection_exec",
            pattern: Regex::new(r#"(?i)\.CommandText\s*=\s*(\$"|"[^"]*"\s*\+|string\.Format)"#)
                .unwrap(),
            tier: Tier::QuickFix,
            confidence: Confidence::High,
            summary_template: "SQL injection risk: CommandText with string concatenation",
        },
        // Dangerous deserialization
        CSharpSecurityRule {
            name: "binary_formatter",
            pattern: Regex::new(r"(?i)\b(BinaryFormatter|SoapFormatter)\b").unwrap(),
            tier: Tier::QuickFix,
            confidence: Confidence::High,
            summary_template:
                "Dangerous deserialization: BinaryFormatter/SoapFormatter is exploitable",
        },
        CSharpSecurityRule {
            name: "javascript_serializer",
            pattern: Regex::new(r"\bJavaScriptSerializer\b").unwrap(),
            tier: Tier::Judgment,
            confidence: Confidence::Medium,
            summary_template: "Unsafe deserialization: JavaScriptSerializer can be exploitable",
        },
        // Disabled TLS verification
        CSharpSecurityRule {
            name: "tls_bypass",
            pattern: Regex::new(r"(?i)ServerCertificateValidationCallback\s*[+=]").unwrap(),
            tier: Tier::QuickFix,
            confidence: Confidence::High,
            summary_template: "TLS bypass: ServerCertificateValidationCallback override",
        },
        // Insecure RNG in security contexts
        CSharpSecurityRule {
            name: "insecure_rng",
            pattern: Regex::new(
                r"(?i)new\s+Random\(\).*(?:token|password|secret|key|salt|nonce|iv)",
            )
            .unwrap(),
            tier: Tier::QuickFix,
            confidence: Confidence::Medium,
            summary_template: "Insecure RNG: System.Random used for security-sensitive value",
        },
        // Hardcoded connection strings with credentials
        CSharpSecurityRule {
            name: "hardcoded_connstr",
            pattern: Regex::new(r#"(?i)(Password|Pwd)\s*=\s*[^;"]+"#).unwrap(),
            tier: Tier::QuickFix,
            confidence: Confidence::Medium,
            summary_template: "Hardcoded credentials in connection string",
        },
    ]
}

/// Detects C#-specific security issues.
pub struct CSharpSecurityDetector;

impl DetectorPhase for CSharpSecurityDetector {
    fn label(&self) -> &str {
        "csharp security"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let rules = csharp_security_rules();
        let mut findings = Vec::new();
        let now = autoclimb_types::newtypes::Timestamp::now().0;

        for file in &ctx.files {
            if !file.ends_with(".cs") {
                continue;
            }

            let path = root.join(file);
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let zone = ctx.zone_map.get(file);
            let is_test = zone.is_scoring_excluded();

            for (line_num, line) in content.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with("//") || trimmed.starts_with('*') {
                    continue;
                }

                for rule in &rules {
                    if !rule.pattern.is_match(line) {
                        continue;
                    }

                    let confidence = if is_test {
                        Confidence::Low
                    } else {
                        rule.confidence
                    };

                    findings.push(Finding {
                        id: format!("csharp_security::{file}::{}_{}", rule.name, line_num + 1),
                        detector: "csharp_security".into(),
                        file: file.clone(),
                        tier: rule.tier,
                        confidence,
                        summary: format!("{} at line {}", rule.summary_template, line_num + 1),
                        detail: serde_json::json!({
                            "rule": rule.name,
                            "line": line_num + 1,
                        }),
                        status: Status::Open,
                        note: None,
                        first_seen: now.clone(),
                        last_seen: now.clone(),
                        resolved_at: None,
                        reopen_count: 0,
                        suppressed: false,
                        suppressed_at: None,
                        suppression_pattern: None,
                        resolution_attestation: None,
                        lang: Some("csharp".into()),
                        zone: Some(zone.to_string()),
                        extra: BTreeMap::new(),
                    });
                }
            }
        }

        let total = ctx.files.iter().filter(|f| f.ends_with(".cs")).count() as u64;
        let mut potentials = BTreeMap::new();
        potentials.insert("csharp_security".into(), total);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_discovery::zones::ZoneMap;
    use std::collections::BTreeSet;

    fn make_ctx(files: Vec<String>) -> ScanContext {
        let zone_map = ZoneMap::new(&files, &[]);
        ScanContext {
            lang_name: "csharp".into(),
            files,
            dep_graph: None,
            zone_map,
            exclusions: vec![],
            entry_patterns: vec!["Program".into()],
            barrel_names: BTreeSet::new(),
            large_threshold: 400,
            complexity_threshold: 25,
        }
    }

    #[test]
    fn detects_sql_injection() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("Repo.cs"),
            "var cmd = new SqlCommand($\"SELECT * FROM Users WHERE Id = {id}\");\n",
        )
        .unwrap();

        let ctx = make_ctx(vec!["Repo.cs".into()]);
        let detector = CSharpSecurityDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(!output.findings.is_empty());
        assert!(output.findings[0].detail["rule"] == "sql_injection");
    }

    #[test]
    fn detects_binary_formatter() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("Serializer.cs"),
            "var formatter = new BinaryFormatter();\nformatter.Deserialize(stream);\n",
        )
        .unwrap();

        let ctx = make_ctx(vec!["Serializer.cs".into()]);
        let detector = CSharpSecurityDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(!output.findings.is_empty());
    }

    #[test]
    fn detects_tls_bypass() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("Client.cs"),
            "ServicePointManager.ServerCertificateValidationCallback += (s, c, ch, e) => true;\n",
        )
        .unwrap();

        let ctx = make_ctx(vec!["Client.cs".into()]);
        let detector = CSharpSecurityDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(!output.findings.is_empty());
        assert!(output.findings[0].detail["rule"] == "tls_bypass");
    }

    #[test]
    fn skips_non_cs_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("script.py"), "x = 1\n").unwrap();

        let ctx = make_ctx(vec!["script.py".into()]);
        let detector = CSharpSecurityDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(output.findings.is_empty());
    }

    #[test]
    fn rules_compile() {
        let rules = csharp_security_rules();
        assert!(rules.len() >= 7);
    }
}
