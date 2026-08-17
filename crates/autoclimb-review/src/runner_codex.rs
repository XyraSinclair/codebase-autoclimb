//! Codex batch runner — executes review batches via the Codex CLI.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::runner::{BatchRunner, RunnerOpts};
use crate::types::{BatchResult, BatchStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodexExit {
    Success,
    Timeout,
    Stall,
    Error(Option<i32>),
}

pub(crate) struct CodexCapture {
    pub exit: CodexExit,
    pub stdout: String,
    pub stderr: String,
    pub elapsed_secs: f64,
}

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

impl CodexRunner {
    pub(crate) async fn execute_capture(&self, prompt: &str, opts: &RunnerOpts) -> CodexCapture {
        let model = opts.model.as_deref().unwrap_or(&self.default_model);
        let mut command = Command::new(&self.codex_bin);
        command
            .arg("exec")
            .arg("--full-auto")
            .arg("--ephemeral")
            .arg("-m")
            .arg(model)
            .arg("-c")
            .arg("model_reasoning_effort=\"high\"");
        if let Some(cwd) = &opts.cwd {
            command.arg("-C").arg(cwd);
        }
        command
            .arg(prompt)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        let start = std::time::Instant::now();
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => return capture_error(start, format!("Failed to spawn codex: {error}")),
        };
        let mut stdout = BufReader::new(child.stdout.take().expect("stdout piped")).lines();
        let mut stderr = BufReader::new(child.stderr.take().expect("stderr piped")).lines();
        let mut stdout_lines = Vec::new();
        let mut stderr_lines = Vec::new();
        let mut stdout_open = true;
        let mut stderr_open = true;
        let mut last_output = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(opts.timeout_secs);
        let stall = std::time::Duration::from_secs(opts.stall_kill_secs);

        let exit = loop {
            let forced = if start.elapsed() > timeout {
                Some(CodexExit::Timeout)
            } else if last_output.elapsed() > stall {
                Some(CodexExit::Stall)
            } else {
                None
            };
            if let Some(exit) = forced {
                let _ = child.kill().await;
                break exit;
            }
            if !stdout_open && !stderr_open {
                break match child.wait().await {
                    Ok(status) if status.success() => CodexExit::Success,
                    Ok(status) => CodexExit::Error(status.code()),
                    Err(error) => {
                        stderr_lines.push(format!("Wait error: {error}"));
                        CodexExit::Error(None)
                    }
                };
            }

            tokio::select! {
                line = stdout.next_line(), if stdout_open => match line {
                    Ok(Some(line)) => { last_output = std::time::Instant::now(); stdout_lines.push(line); }
                    Ok(None) => stdout_open = false,
                    Err(error) => { stderr_lines.push(format!("stdout read error: {error}")); stdout_open = false; }
                },
                line = stderr.next_line(), if stderr_open => match line {
                    Ok(Some(line)) => { last_output = std::time::Instant::now(); stderr_lines.push(line); }
                    Ok(None) => stderr_open = false,
                    Err(error) => { stderr_lines.push(format!("stderr read error: {error}")); stderr_open = false; }
                },
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                    match child.try_wait() {
                        Ok(Some(status)) => break if status.success() { CodexExit::Success } else { CodexExit::Error(status.code()) },
                        Ok(None) => {}
                        Err(error) => { stderr_lines.push(format!("Wait error: {error}")); break CodexExit::Error(None); }
                    }
                }
            }
        };

        while let Ok(Some(line)) = stdout.next_line().await {
            stdout_lines.push(line);
        }
        while let Ok(Some(line)) = stderr.next_line().await {
            stderr_lines.push(line);
        }
        CodexCapture {
            exit,
            stdout: stdout_lines.join("\n"),
            stderr: stderr_lines.join("\n"),
            elapsed_secs: start.elapsed().as_secs_f64(),
        }
    }
}

impl BatchRunner for CodexRunner {
    async fn execute(&self, prompt: &str, opts: &RunnerOpts) -> BatchResult {
        let capture = self.execute_capture(prompt, opts).await;
        let (status, raw_output) = match capture.exit {
            CodexExit::Success => (BatchStatus::Success, capture.stdout),
            CodexExit::Timeout => (BatchStatus::Timeout, capture.stdout),
            CodexExit::Stall => (
                BatchStatus::Timeout,
                format!(
                    "Stalled after {}s of no output\n{}",
                    opts.stall_kill_secs, capture.stdout
                ),
            ),
            CodexExit::Error(code) => (
                BatchStatus::ProcessError,
                format!(
                    "Exit code: {code:?}\nstdout:\n{}\nstderr:\n{}",
                    capture.stdout, capture.stderr
                ),
            ),
        };
        BatchResult {
            index: 0,
            status,
            payload: None,
            raw_output,
            elapsed_secs: capture.elapsed_secs,
        }
    }

    fn name(&self) -> &str {
        "codex"
    }
}

fn capture_error(start: std::time::Instant, message: String) -> CodexCapture {
    CodexCapture {
        exit: CodexExit::Error(None),
        stdout: String::new(),
        stderr: message,
        elapsed_secs: start.elapsed().as_secs_f64(),
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
