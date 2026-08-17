//! Codex batch runner — executes review batches via the Codex CLI.
//!
//! Spawns `codex exec` subprocesses with the review prompt,
//! captures stdout/stderr, and handles timeouts and stall detection.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::runner::{BatchRunner, RunnerOpts};
use crate::types::{BatchResult, BatchStatus};

/// Codex CLI batch runner.
pub struct CodexRunner {
    /// Path to the codex binary (default: "codex").
    pub codex_bin: String,
    /// Default model for codex.
    pub default_model: String,
}

impl Default for CodexRunner {
    fn default() -> Self {
        Self {
            codex_bin: "codex".to_string(),
            default_model: "gpt-5.3-codex".to_string(),
        }
    }
}

impl BatchRunner for CodexRunner {
    async fn execute(&self, prompt: &str, opts: &RunnerOpts) -> BatchResult {
        let model = opts.model.as_deref().unwrap_or(&self.default_model);

        let mut cmd = Command::new(&self.codex_bin);
        cmd.arg("exec")
            .arg("--full-auto")
            .arg("-m")
            .arg(model)
            .arg("-c")
            .arg("model_reasoning_effort=\"high\"");

        // Set working directory if specified
        if let Some(ref cwd) = opts.cwd {
            cmd.arg("-C").arg(cwd);
        }

        cmd.arg(prompt);

        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        let timeout = std::time::Duration::from_secs(opts.timeout_secs);
        let stall_timeout = std::time::Duration::from_secs(opts.stall_kill_secs);

        // Spawn the process
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return BatchResult {
                    index: 0,
                    status: BatchStatus::ProcessError,
                    payload: None,
                    raw_output: format!("Failed to spawn codex: {e}"),
                    elapsed_secs: 0.0,
                };
            }
        };

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let mut stdout_lines = Vec::new();
        let mut stderr_lines = Vec::new();

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let start = std::time::Instant::now();
        let mut last_output = std::time::Instant::now();

        // Read output with timeout and stall detection
        loop {
            if start.elapsed() > timeout {
                let _ = child.kill().await;
                return BatchResult {
                    index: 0,
                    status: BatchStatus::Timeout,
                    payload: None,
                    raw_output: stdout_lines.join("\n"),
                    elapsed_secs: start.elapsed().as_secs_f64(),
                };
            }

            if last_output.elapsed() > stall_timeout {
                let _ = child.kill().await;
                return BatchResult {
                    index: 0,
                    status: BatchStatus::Timeout,
                    payload: None,
                    raw_output: format!(
                        "Stalled after {}s of no output\n{}",
                        opts.stall_kill_secs,
                        stdout_lines.join("\n")
                    ),
                    elapsed_secs: start.elapsed().as_secs_f64(),
                };
            }

            tokio::select! {
                line = stdout_reader.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            last_output = std::time::Instant::now();
                            stdout_lines.push(l);
                        }
                        Ok(None) => break, // EOF
                        Err(e) => {
                            stderr_lines.push(format!("stdout read error: {e}"));
                            break;
                        }
                    }
                }
                line = stderr_reader.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            last_output = std::time::Instant::now();
                            stderr_lines.push(l);
                        }
                        Ok(None) => {} // stderr EOF, continue reading stdout
                        Err(_) => {}
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                    // Check if child has exited
                    if let Ok(Some(_status)) = child.try_wait() {
                        // Drain remaining output
                        while let Ok(Some(l)) = stdout_reader.next_line().await {
                            stdout_lines.push(l);
                        }
                        break;
                    }
                }
            }
        }

        // Wait for process to finish
        let exit_status = match child.wait().await {
            Ok(s) => s,
            Err(e) => {
                return BatchResult {
                    index: 0,
                    status: BatchStatus::ProcessError,
                    payload: None,
                    raw_output: format!("Wait error: {e}\n{}", stdout_lines.join("\n")),
                    elapsed_secs: start.elapsed().as_secs_f64(),
                };
            }
        };

        let raw_output = stdout_lines.join("\n");

        if !exit_status.success() {
            return BatchResult {
                index: 0,
                status: BatchStatus::ProcessError,
                payload: None,
                raw_output: format!(
                    "Exit code: {:?}\nstdout:\n{}\nstderr:\n{}",
                    exit_status.code(),
                    raw_output,
                    stderr_lines.join("\n"),
                ),
                elapsed_secs: start.elapsed().as_secs_f64(),
            };
        }

        BatchResult {
            index: 0,
            status: BatchStatus::Success,
            payload: None, // Parsing happens in result_parser
            raw_output,
            elapsed_secs: start.elapsed().as_secs_f64(),
        }
    }

    fn name(&self) -> &str {
        "codex"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_runner_defaults() {
        let runner = CodexRunner::default();
        assert_eq!(runner.codex_bin, "codex");
        assert_eq!(runner.name(), "codex");
    }
}
