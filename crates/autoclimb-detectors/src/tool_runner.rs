//! External tool runner for invoking linters and static analysis tools.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

/// Specification for an external tool to run.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub label: String,
    pub cmd: Vec<String>,
    pub format: OutputFormat,
    pub timeout_secs: u32,
    pub detector_name: String,
    pub tier: Tier,
    pub fix_cmd: Option<Vec<String>>,
}

/// Output format of the external tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// JSON array of objects with `file`, `line`, `message` fields.
    Json,
    /// ESLint-style: `file:line:col: message`
    Eslint,
    /// Cargo-style JSON diagnostics (one JSON object per line).
    Cargo,
    /// GNU-style: `file:line: severity: message`
    Gnu,
    /// RuboCop JSON format.
    Rubocop,
    /// golangci-lint JSON format.
    Golangci,
}

/// Status of a tool run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Ok,
    Empty,
    Error,
    NotFound,
    Timeout,
}

/// Result from running an external tool.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub status: ToolStatus,
    pub findings: Vec<Finding>,
    pub raw_output: String,
}

/// Run an external tool and parse its output into findings.
pub fn run_tool(spec: &ToolSpec, root: &Path, _files: &[String]) -> ToolResult {
    if spec.cmd.is_empty() {
        return ToolResult {
            status: ToolStatus::Error,
            findings: Vec::new(),
            raw_output: "Empty command".into(),
        };
    }

    let program = &spec.cmd[0];
    let args = &spec.cmd[1..];

    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(root);

    // Capture output with timeout
    let output = match run_with_timeout(&mut cmd, Duration::from_secs(spec.timeout_secs as u64)) {
        Ok(o) => o,
        Err(ToolError::NotFound) => {
            return ToolResult {
                status: ToolStatus::NotFound,
                findings: Vec::new(),
                raw_output: format!("Tool not found: {program}"),
            };
        }
        Err(ToolError::Timeout) => {
            return ToolResult {
                status: ToolStatus::Timeout,
                findings: Vec::new(),
                raw_output: format!("Timeout after {}s", spec.timeout_secs),
            };
        }
        Err(ToolError::Other(msg)) => {
            return ToolResult {
                status: ToolStatus::Error,
                findings: Vec::new(),
                raw_output: msg,
            };
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let raw = if stderr.is_empty() {
        stdout.clone()
    } else {
        format!("{stdout}\n{stderr}")
    };

    if stdout.trim().is_empty() {
        return ToolResult {
            status: ToolStatus::Empty,
            findings: Vec::new(),
            raw_output: raw,
        };
    }

    let findings = parse_output(&stdout, spec);

    ToolResult {
        status: if findings.is_empty() {
            ToolStatus::Empty
        } else {
            ToolStatus::Ok
        },
        findings,
        raw_output: raw,
    }
}

enum ToolError {
    NotFound,
    Timeout,
    Other(String),
}

fn run_with_timeout(
    cmd: &mut Command,
    timeout: Duration,
) -> Result<std::process::Output, ToolError> {
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ToolError::NotFound
            } else {
                ToolError::Other(e.to_string())
            }
        })?;

    // Simple timeout via thread + wait
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                return child
                    .wait_with_output()
                    .map_err(|e| ToolError::Other(e.to_string()));
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    return Err(ToolError::Timeout);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(ToolError::Other(e.to_string())),
        }
    }
}

fn parse_output(stdout: &str, spec: &ToolSpec) -> Vec<Finding> {
    match spec.format {
        OutputFormat::Json => parse_json_output(stdout, spec),
        OutputFormat::Eslint => parse_eslint_output(stdout, spec),
        OutputFormat::Gnu => parse_gnu_output(stdout, spec),
        OutputFormat::Cargo => parse_cargo_output(stdout, spec),
        OutputFormat::Rubocop => parse_rubocop_output(stdout, spec),
        OutputFormat::Golangci => parse_golangci_output(stdout, spec),
    }
}

/// Parse JSON array output: `[{"file": "...", "line": N, "message": "..."}]`
fn parse_json_output(stdout: &str, spec: &ToolSpec) -> Vec<Finding> {
    let arr: Vec<serde_json::Value> = match serde_json::from_str(stdout) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    arr.iter()
        .filter_map(|v| {
            let file = v.get("file")?.as_str()?.to_string();
            let line = v.get("line").and_then(|l| l.as_u64()).unwrap_or(0) as u32;
            let message = v.get("message")?.as_str()?.to_string();
            Some(make_tool_finding(spec, &file, line, &message))
        })
        .collect()
}

/// Parse ESLint-style output: `filepath:line:col: message`
fn parse_eslint_output(stdout: &str, spec: &ToolSpec) -> Vec<Finding> {
    let re = regex::Regex::new(r"^(.+?):(\d+):\d+:\s+(.+)$").unwrap();
    stdout
        .lines()
        .filter_map(|line| {
            let caps = re.captures(line)?;
            let file = caps.get(1)?.as_str().to_string();
            let lineno: u32 = caps.get(2)?.as_str().parse().ok()?;
            let message = caps.get(3)?.as_str().to_string();
            Some(make_tool_finding(spec, &file, lineno, &message))
        })
        .collect()
}

/// Parse GNU-style output: `file:line: severity: message`
fn parse_gnu_output(stdout: &str, spec: &ToolSpec) -> Vec<Finding> {
    let re = regex::Regex::new(r"^(.+?):(\d+):\s*\w+:\s*(.+)$").unwrap();
    stdout
        .lines()
        .filter_map(|line| {
            let caps = re.captures(line)?;
            let file = caps.get(1)?.as_str().to_string();
            let lineno: u32 = caps.get(2)?.as_str().parse().ok()?;
            let message = caps.get(3)?.as_str().to_string();
            Some(make_tool_finding(spec, &file, lineno, &message))
        })
        .collect()
}

/// Parse Cargo JSON diagnostics (one JSON per line).
fn parse_cargo_output(stdout: &str, spec: &ToolSpec) -> Vec<Finding> {
    stdout
        .lines()
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            let msg = v.get("message")?;
            let message = msg.get("message")?.as_str()?.to_string();
            let spans = msg.get("spans")?.as_array()?;
            let span = spans.first()?;
            let file = span.get("file_name")?.as_str()?.to_string();
            let lineno = span.get("line_start").and_then(|l| l.as_u64()).unwrap_or(0) as u32;
            Some(make_tool_finding(spec, &file, lineno, &message))
        })
        .collect()
}

/// Parse RuboCop JSON output.
fn parse_rubocop_output(stdout: &str, spec: &ToolSpec) -> Vec<Finding> {
    let root: serde_json::Value = match serde_json::from_str(stdout) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let files = match root.get("files").and_then(|f| f.as_array()) {
        Some(f) => f,
        None => return Vec::new(),
    };
    let mut findings = Vec::new();
    for file_entry in files {
        let file = match file_entry.get("path").and_then(|p| p.as_str()) {
            Some(f) => f.to_string(),
            None => continue,
        };
        let offenses = match file_entry.get("offenses").and_then(|o| o.as_array()) {
            Some(o) => o,
            None => continue,
        };
        for offense in offenses {
            let message = offense
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or_default()
                .to_string();
            let lineno = offense
                .get("location")
                .and_then(|l| l.get("start_line"))
                .and_then(|l| l.as_u64())
                .unwrap_or(0) as u32;
            findings.push(make_tool_finding(spec, &file, lineno, &message));
        }
    }
    findings
}

/// Parse golangci-lint JSON output.
fn parse_golangci_output(stdout: &str, spec: &ToolSpec) -> Vec<Finding> {
    let root: serde_json::Value = match serde_json::from_str(stdout) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let issues = match root.get("Issues").and_then(|i| i.as_array()) {
        Some(i) => i,
        None => return Vec::new(),
    };
    issues
        .iter()
        .filter_map(|issue| {
            let file = issue.get("Pos")?.get("Filename")?.as_str()?.to_string();
            let lineno = issue
                .get("Pos")
                .and_then(|p| p.get("Line"))
                .and_then(|l| l.as_u64())
                .unwrap_or(0) as u32;
            let message = issue.get("Text")?.as_str()?.to_string();
            Some(make_tool_finding(spec, &file, lineno, &message))
        })
        .collect()
}

fn make_tool_finding(spec: &ToolSpec, file: &str, line: u32, message: &str) -> Finding {
    let finding_id = format!("{}::{file}::{line}", spec.detector_name);
    let now = autoclimb_types::newtypes::Timestamp::now();
    Finding {
        id: finding_id,
        detector: spec.detector_name.clone(),
        file: file.to_string(),
        tier: spec.tier,
        confidence: Confidence::High,
        summary: message.to_string(),
        detail: serde_json::json!({
            "tool": spec.label,
            "line": line,
        }),
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
        lang: None,
        zone: None,
        extra: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_spec() -> ToolSpec {
        ToolSpec {
            label: "test-tool".into(),
            cmd: vec!["echo".into(), "hello".into()],
            format: OutputFormat::Json,
            timeout_secs: 10,
            detector_name: "test_tool".into(),
            tier: Tier::Judgment,
            fix_cmd: None,
        }
    }

    #[test]
    fn parse_json_array() {
        let spec = test_spec();
        let json = r#"[{"file": "src/main.py", "line": 10, "message": "unused import"}]"#;
        let findings = parse_json_output(json, &spec);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "src/main.py");
        assert!(findings[0].summary.contains("unused import"));
    }

    #[test]
    fn parse_eslint_lines() {
        let spec = ToolSpec {
            format: OutputFormat::Eslint,
            ..test_spec()
        };
        let output =
            "src/app.ts:42:5: 'x' is defined but never used\nsrc/app.ts:50:1: Missing semicolon\n";
        let findings = parse_eslint_output(output, &spec);
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn parse_gnu_lines() {
        let spec = ToolSpec {
            format: OutputFormat::Gnu,
            ..test_spec()
        };
        let output = "main.c:10: warning: unused variable 'x'\n";
        let findings = parse_gnu_output(output, &spec);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "main.c");
    }

    #[test]
    fn parse_rubocop_json() {
        let spec = ToolSpec {
            format: OutputFormat::Rubocop,
            ..test_spec()
        };
        let json = r#"{"files":[{"path":"app.rb","offenses":[{"message":"Line is too long","location":{"start_line":5}}]}]}"#;
        let findings = parse_rubocop_output(json, &spec);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "app.rb");
    }

    #[test]
    fn parse_golangci_json() {
        let spec = ToolSpec {
            format: OutputFormat::Golangci,
            ..test_spec()
        };
        let json = r#"{"Issues":[{"Text":"exported function Foo","Pos":{"Filename":"main.go","Line":10}}]}"#;
        let findings = parse_golangci_output(json, &spec);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "main.go");
    }

    #[test]
    fn tool_not_found() {
        let spec = ToolSpec {
            cmd: vec!["nonexistent_tool_xyz_12345".into()],
            ..test_spec()
        };
        let result = run_tool(&spec, Path::new("."), &[]);
        assert_eq!(result.status, ToolStatus::NotFound);
    }

    #[test]
    fn tool_empty_output() {
        let spec = ToolSpec {
            cmd: vec!["true".into()],
            ..test_spec()
        };
        let result = run_tool(&spec, Path::new("."), &[]);
        assert_eq!(result.status, ToolStatus::Empty);
    }
}
