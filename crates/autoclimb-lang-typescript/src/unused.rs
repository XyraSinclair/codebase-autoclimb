//! TypeScript unused code detector.
//!
//! Detects unused imports and variables via regex analysis.
//! (Full implementation would use tsc TS6133/TS6192 when available.)

use std::collections::BTreeMap;
use std::path::Path;

use regex::Regex;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};
use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

/// TypeScript unused imports/variables detector.
pub struct TypeScriptUnusedDetector;

impl DetectorPhase for TypeScriptUnusedDetector {
    fn label(&self) -> &str {
        "TypeScript unused"
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
            findings.extend(detect_unused_imports(&source, file, &now));
        }

        Ok(PhaseOutput {
            findings,
            potentials: BTreeMap::from([("ts_unused".into(), files_scanned)]),
        })
    }
}

/// Detected import entry.
struct ImportEntry {
    names: Vec<String>,
    line: usize,
    is_type_only: bool,
}

/// Detect unused imports in TypeScript source.
fn detect_unused_imports(source: &str, file: &str, now: &str) -> Vec<Finding> {
    let imports = collect_imports(source);
    let mut findings = Vec::new();

    for import in &imports {
        for name in &import.names {
            // Skip React (used implicitly in JSX)
            if name == "React" && (file.ends_with(".tsx") || file.ends_with(".jsx")) {
                continue;
            }

            // Check if name appears elsewhere in the source (beyond the import line)
            if !is_name_used_after(source, name, import.line) {
                let summary = if import.is_type_only {
                    format!("Unused type import: {name}")
                } else {
                    format!("Unused import: {name}")
                };

                findings.push(Finding {
                    id: format!("ts_unused::{file}::{name}"),
                    detector: "ts_unused".into(),
                    file: file.to_string(),
                    tier: Tier::AutoFix,
                    confidence: Confidence::Medium,
                    summary,
                    detail: serde_json::json!({
                        "import": name,
                        "line": import.line,
                        "type_only": import.is_type_only,
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

fn collect_imports(source: &str) -> Vec<ImportEntry> {
    let named_re = Regex::new(r#"import\s+(?:type\s+)?\{\s*([^}]+)\}\s+from\s+['""]"#).unwrap();

    let default_re = Regex::new(r#"import\s+(?:type\s+)?(\w+)\s+from\s+['""]"#).unwrap();

    let type_re = Regex::new(r"import\s+type\b").unwrap();

    let mut imports = Vec::new();

    for (line_idx, line) in source.lines().enumerate() {
        let is_type = type_re.is_match(line);

        // Named imports: import { A, B, C } from '...'
        if let Some(cap) = named_re.captures(line) {
            let names: Vec<String> = cap[1]
                .split(',')
                .map(|s| {
                    let s = s.trim();
                    // Handle "X as Y" aliases
                    if let Some(alias_part) = s.split(" as ").nth(1) {
                        alias_part.trim().to_string()
                    } else {
                        // Handle "type X" prefixes within named imports
                        s.strip_prefix("type ").unwrap_or(s).trim().to_string()
                    }
                })
                .filter(|s| !s.is_empty())
                .collect();

            imports.push(ImportEntry {
                names,
                line: line_idx + 1,
                is_type_only: is_type,
            });
        }
        // Default imports: import X from '...'
        else if let Some(cap) = default_re.captures(line) {
            let name = cap[1].to_string();
            imports.push(ImportEntry {
                names: vec![name],
                line: line_idx + 1,
                is_type_only: is_type,
            });
        }
    }

    imports
}

/// Check if a name is used in the source after the given line index.
fn is_name_used_after(source: &str, name: &str, import_line: usize) -> bool {
    let word_re = Regex::new(&format!(r"\b{}\b", regex::escape(name))).unwrap();

    for (line_idx, line) in source.lines().enumerate() {
        if line_idx < import_line {
            continue;
        }
        // Skip comments
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('*') {
            continue;
        }
        if word_re.is_match(line) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_unused_import() {
        let source = "import { useState } from 'react';\nconst x = 1;\n";
        let findings = detect_unused_imports(source, "test.ts", "2025-01-01");
        assert!(findings.iter().any(|f| f.summary.contains("useState")));
    }

    #[test]
    fn used_import_clean() {
        let source = "import { useState } from 'react';\nconst [x, setX] = useState(0);\n";
        let findings = detect_unused_imports(source, "test.ts", "2025-01-01");
        assert!(findings.is_empty());
    }

    #[test]
    fn skip_react_in_tsx() {
        let source = "import React from 'react';\nconst App = () => <div />;\n";
        let findings = detect_unused_imports(source, "App.tsx", "2025-01-01");
        assert!(findings.is_empty());
    }

    #[test]
    fn detect_unused_type_import() {
        let source = "import type { User } from './types';\nconst x = 1;\n";
        let findings = detect_unused_imports(source, "test.ts", "2025-01-01");
        assert!(findings.iter().any(|f| f.summary.contains("type import")));
    }

    #[test]
    fn handle_aliased_imports() {
        let source = "import { foo as bar } from './utils';\nconst x = bar();\n";
        let findings = detect_unused_imports(source, "test.ts", "2025-01-01");
        assert!(findings.is_empty()); // bar is used
    }

    #[test]
    fn multiple_named_imports() {
        let source = "import { used, unused } from './utils';\nconst x = used();\n";
        let findings = detect_unused_imports(source, "test.ts", "2025-01-01");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].summary.contains("unused"));
    }
}
