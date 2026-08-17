//! AI debt signal detector.
//!
//! Detects patterns indicating AI-generated code that may need human review:
//! - comment_ratio > 0.3 (over-commented, typical of AI output)
//! - log_density > 3.0 (excessive logging per function)
//! - guard_density > 2.0 (excessive defensive guards)

use super::{ContextSignal, SignalSeverity, SignalType};

/// Thresholds for AI debt detection.
const COMMENT_RATIO_THRESHOLD: f64 = 0.3;
const LOG_DENSITY_THRESHOLD: f64 = 3.0;
const GUARD_DENSITY_THRESHOLD: f64 = 2.0;
/// Minimum lines for a file to be analyzed.
const MIN_LINES: usize = 30;

/// Detect AI debt patterns in file contents.
pub fn detect(file_contents: &[(String, String)]) -> Vec<ContextSignal> {
    let mut high_comment_files = Vec::new();
    let mut high_log_files = Vec::new();
    let mut high_guard_files = Vec::new();

    for (path, content) in file_contents {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() < MIN_LINES {
            continue;
        }

        let non_blank: Vec<&&str> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
        if non_blank.is_empty() {
            continue;
        }

        // Comment ratio
        let comment_lines = non_blank
            .iter()
            .filter(|l| {
                let t = l.trim();
                t.starts_with('#')
                    || t.starts_with("//")
                    || t.starts_with("/*")
                    || t.starts_with('*')
                    || t.starts_with("\"\"\"")
                    || t.starts_with("'''")
            })
            .count();
        let comment_ratio = comment_lines as f64 / non_blank.len() as f64;
        if comment_ratio > COMMENT_RATIO_THRESHOLD {
            high_comment_files.push(path.clone());
        }

        // Count functions (rough heuristic)
        let func_count = non_blank
            .iter()
            .filter(|l| {
                let t = l.trim();
                t.starts_with("def ")
                    || t.starts_with("fn ")
                    || t.starts_with("func ")
                    || t.starts_with("function ")
                    || t.starts_with("async def ")
                    || t.starts_with("async fn ")
            })
            .count();

        let effective_funcs = func_count.max(1);

        // Log density
        let log_lines = non_blank
            .iter()
            .filter(|l| {
                let t = l.trim();
                t.contains("console.log")
                    || t.contains("print(")
                    || t.contains("println!")
                    || t.contains("logging.")
                    || t.contains("logger.")
                    || t.contains("log.")
                    || t.contains("tracing::")
            })
            .count();
        let log_density = log_lines as f64 / effective_funcs as f64;
        if log_density > LOG_DENSITY_THRESHOLD {
            high_log_files.push(path.clone());
        }

        // Guard density
        let guard_lines = non_blank
            .iter()
            .filter(|l| {
                let t = l.trim();
                t.starts_with("if not ")
                    || t.starts_with("if !")
                    || t.contains("is None")
                    || t.contains("is_none()")
                    || t.contains("== null")
                    || t.contains("=== null")
                    || t.contains("=== undefined")
                    || (t.starts_with("if ") && t.contains("return"))
            })
            .count();
        let guard_density = guard_lines as f64 / effective_funcs as f64;
        if guard_density > GUARD_DENSITY_THRESHOLD {
            high_guard_files.push(path.clone());
        }
    }

    let mut signals = Vec::new();

    if high_comment_files.len() >= 3 {
        signals.push(ContextSignal {
            signal_type: SignalType::AiDebt,
            severity: SignalSeverity::Medium,
            message: format!(
                "{} files have comment ratio > {:.0}% — possible AI-generated boilerplate",
                high_comment_files.len(),
                COMMENT_RATIO_THRESHOLD * 100.0,
            ),
            files: high_comment_files,
            detail: serde_json::json!({"pattern": "high_comment_ratio"}),
        });
    }

    if high_log_files.len() >= 3 {
        signals.push(ContextSignal {
            signal_type: SignalType::AiDebt,
            severity: SignalSeverity::Low,
            message: format!(
                "{} files have excessive logging density — review for production readiness",
                high_log_files.len(),
            ),
            files: high_log_files,
            detail: serde_json::json!({"pattern": "high_log_density"}),
        });
    }

    if high_guard_files.len() >= 3 {
        signals.push(ContextSignal {
            signal_type: SignalType::AiDebt,
            severity: SignalSeverity::Low,
            message: format!(
                "{} files have excessive guard clauses — consider trusting internal code more",
                high_guard_files.len(),
            ),
            files: high_guard_files,
            detail: serde_json::json!({"pattern": "high_guard_density"}),
        });
    }

    signals
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_with_comments(comment_pct: f64) -> String {
        let total = 60;
        let comment_count = (total as f64 * comment_pct) as usize;
        let code_count = total - comment_count;
        let mut lines = vec!["# This is a comment explaining the next line"; comment_count];
        for i in 0..code_count {
            lines.push(if i == 0 {
                "def process(x):"
            } else {
                "    x = x + 1"
            });
        }
        lines.join("\n")
    }

    #[test]
    fn detects_high_comment_ratio() {
        let files: Vec<(String, String)> = (0..5)
            .map(|i| (format!("src/mod_{i}.py"), file_with_comments(0.4)))
            .collect();

        let signals = detect(&files);
        assert!(signals.iter().any(|s| s
            .detail
            .get("pattern")
            .and_then(|v| v.as_str())
            .is_some_and(|p| p == "high_comment_ratio")));
    }

    #[test]
    fn no_signal_below_threshold() {
        let files: Vec<(String, String)> = (0..5)
            .map(|i| (format!("src/mod_{i}.py"), file_with_comments(0.1)))
            .collect();

        let signals = detect(&files);
        assert!(signals.is_empty());
    }

    #[test]
    fn needs_3_files_minimum() {
        let files = vec![
            ("src/a.py".into(), file_with_comments(0.5)),
            ("src/b.py".into(), file_with_comments(0.5)),
        ];

        let signals = detect(&files);
        assert!(signals.is_empty());
    }
}
