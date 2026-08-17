//! Batch execution engine for LLM review.
//!
//! Runs review prompts as subprocesses with timeout, heartbeat
//! monitoring, and stall detection. Uses tokio for async subprocess
//! management.

use std::path::Path;
use std::time::Instant;

use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::types::{BatchPrompt, BatchResult, BatchStatus, Provenance, ReviewPayload, ReviewScope};

/// Configuration for batch execution.
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Timeout per batch in seconds.
    pub timeout_secs: u32,
    /// Maximum retry attempts.
    pub max_retries: u32,
    /// Backoff multiplier for retries.
    pub retry_backoff_secs: f64,
    /// Maximum parallel batches.
    pub max_parallel: u32,
    /// Heartbeat interval in seconds.
    pub heartbeat_secs: f64,
    /// Kill after this many seconds of no output.
    pub stall_kill_secs: u32,
    /// Command template for the review runner.
    pub runner_cmd: Vec<String>,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 1200,
            max_retries: 1,
            retry_backoff_secs: 2.0,
            max_parallel: 3,
            heartbeat_secs: 15.0,
            stall_kill_secs: 120,
            runner_cmd: Vec::new(),
        }
    }
}

/// Run a single batch prompt as a subprocess.
pub async fn run_batch(prompt: &BatchPrompt, config: &BatchConfig, root: &Path) -> BatchResult {
    let start = Instant::now();

    for attempt in 0..=config.max_retries {
        if attempt > 0 {
            let backoff = Duration::from_secs_f64(config.retry_backoff_secs * attempt as f64);
            tokio::time::sleep(backoff).await;
        }

        let result = run_single_attempt(prompt, config, root).await;

        match result.status {
            BatchStatus::Success => return result,
            BatchStatus::Timeout | BatchStatus::ProcessError if attempt < config.max_retries => {
                continue;
            }
            _ => {
                return BatchResult {
                    elapsed_secs: start.elapsed().as_secs_f64(),
                    ..result
                };
            }
        }
    }

    BatchResult {
        index: prompt.index,
        status: BatchStatus::ProcessError,
        payload: None,
        raw_output: "max retries exceeded".into(),
        elapsed_secs: start.elapsed().as_secs_f64(),
    }
}

async fn run_single_attempt(
    prompt: &BatchPrompt,
    config: &BatchConfig,
    root: &Path,
) -> BatchResult {
    let start = Instant::now();

    let cmd = if config.runner_cmd.is_empty() {
        return BatchResult {
            index: prompt.index,
            status: BatchStatus::ProcessError,
            payload: None,
            raw_output: "no runner command configured".into(),
            elapsed_secs: 0.0,
        };
    } else {
        &config.runner_cmd
    };

    let child = Command::new(&cmd[0])
        .args(&cmd[1..])
        .current_dir(root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            return BatchResult {
                index: prompt.index,
                status: BatchStatus::ProcessError,
                payload: None,
                raw_output: format!("spawn error: {e}"),
                elapsed_secs: start.elapsed().as_secs_f64(),
            };
        }
    };

    // Write prompt to stdin then drop to close it
    {
        use tokio::io::AsyncWriteExt;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(prompt.prompt.as_bytes()).await;
        }
    }

    // Read stdout concurrently with waiting
    let stdout_handle = child.stdout.take();
    let _stderr_handle = child.stderr.take();

    // Wait with timeout
    let duration = Duration::from_secs(config.timeout_secs as u64);
    match timeout(duration, child.wait()).await {
        Ok(Ok(exit_status)) => {
            // Read remaining stdout/stderr
            let stdout = read_handle(stdout_handle).await;

            if !exit_status.success() {
                return BatchResult {
                    index: prompt.index,
                    status: BatchStatus::ProcessError,
                    payload: None,
                    raw_output: stdout,
                    elapsed_secs: start.elapsed().as_secs_f64(),
                };
            }

            // Try to parse the output as ReviewPayload JSON
            match parse_review_output(&stdout, prompt) {
                Ok(payload) => BatchResult {
                    index: prompt.index,
                    status: BatchStatus::Success,
                    payload: Some(payload),
                    raw_output: stdout,
                    elapsed_secs: start.elapsed().as_secs_f64(),
                },
                Err(e) => BatchResult {
                    index: prompt.index,
                    status: BatchStatus::ParseError,
                    payload: None,
                    raw_output: format!("parse error: {e}\n\n{stdout}"),
                    elapsed_secs: start.elapsed().as_secs_f64(),
                },
            }
        }
        Ok(Err(e)) => BatchResult {
            index: prompt.index,
            status: BatchStatus::ProcessError,
            payload: None,
            raw_output: format!("process error: {e}"),
            elapsed_secs: start.elapsed().as_secs_f64(),
        },
        Err(_) => {
            let _ = child.kill().await;
            BatchResult {
                index: prompt.index,
                status: BatchStatus::Timeout,
                payload: None,
                raw_output: format!("timeout after {}s", config.timeout_secs),
                elapsed_secs: start.elapsed().as_secs_f64(),
            }
        }
    }
}

async fn read_handle(handle: Option<tokio::process::ChildStdout>) -> String {
    use tokio::io::AsyncReadExt;
    match handle {
        Some(mut h) => {
            let mut buf = Vec::new();
            let _ = h.read_to_end(&mut buf).await;
            String::from_utf8_lossy(&buf).to_string()
        }
        None => String::new(),
    }
}

/// Parse review runner output into a ReviewPayload.
fn parse_review_output(output: &str, prompt: &BatchPrompt) -> Result<ReviewPayload, String> {
    // Find JSON block in output (may be wrapped in markdown fences)
    let json_str = extract_json_block(output).unwrap_or(output);

    let payload: ReviewPayload =
        serde_json::from_str(json_str).map_err(|e| format!("JSON parse: {e}"))?;

    // Validate scope matches
    if let ReviewScope::Batch { index, total } = &payload.review_scope {
        if *index != prompt.index || *total != prompt.total {
            return Err(format!(
                "scope mismatch: expected batch {}/{}, got {}/{}",
                prompt.index, prompt.total, index, total
            ));
        }
    }

    Ok(payload)
}

/// Extract a JSON block from output that may contain markdown fences.
fn extract_json_block(output: &str) -> Option<&str> {
    // Look for ```json ... ``` blocks
    if let Some(start) = output.find("```json") {
        let json_start = start + 7;
        if let Some(end) = output[json_start..].find("```") {
            return Some(output[json_start..json_start + end].trim());
        }
    }
    // Look for plain ``` ... ``` blocks
    if let Some(start) = output.find("```\n{") {
        let json_start = start + 4;
        if let Some(end) = output[json_start..].find("\n```") {
            return Some(output[json_start..json_start + end].trim());
        }
    }
    // Try the whole output as JSON
    let trimmed = output.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(trimmed);
    }
    None
}

/// Run multiple batches with parallelism control.
pub async fn run_batches(
    prompts: Vec<BatchPrompt>,
    config: &BatchConfig,
    root: &Path,
) -> Vec<BatchResult> {
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(config.max_parallel as usize));
    let mut handles = Vec::new();

    for prompt in prompts {
        let sem = semaphore.clone();
        let cfg = config.clone();
        let root = root.to_path_buf();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            run_batch(&prompt, &cfg, &root).await
        });
        handles.push(handle);
    }

    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => results.push(BatchResult {
                index: 0,
                status: BatchStatus::ProcessError,
                payload: None,
                raw_output: format!("join error: {e}"),
                elapsed_secs: 0.0,
            }),
        }
    }

    results.sort_by_key(|r| r.index);
    results
}

/// Create a provenance record for a batch run.
pub fn batch_provenance(runner: &str, model: Option<&str>, batch_count: usize) -> Provenance {
    Provenance {
        runner: runner.to_string(),
        model: model.map(|s| s.to_string()),
        timestamp: autoclimb_types::newtypes::Timestamp::now().0,
        batch_count,
        session_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_from_markdown() {
        let output = "Some text\n```json\n{\"key\": \"value\"}\n```\nMore text";
        let json = extract_json_block(output).unwrap();
        assert_eq!(json, "{\"key\": \"value\"}");
    }

    #[test]
    fn extract_json_plain() {
        let output = "{\"key\": \"value\"}";
        let json = extract_json_block(output).unwrap();
        assert_eq!(json, "{\"key\": \"value\"}");
    }

    #[test]
    fn extract_json_none() {
        let output = "no json here";
        assert!(extract_json_block(output).is_none());
    }

    #[test]
    fn batch_config_defaults() {
        let config = BatchConfig::default();
        assert_eq!(config.timeout_secs, 1200);
        assert_eq!(config.max_parallel, 3);
    }
}
