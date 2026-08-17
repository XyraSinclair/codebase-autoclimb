//! Batch runner trait and execution orchestration.
//!
//! Defines the interface for running review batches and provides
//! parallel execution support.

use std::path::Path;

use crate::types::{BatchResult, BatchStatus};

/// Options for batch execution.
#[derive(Debug, Clone)]
pub struct RunnerOpts {
    /// Timeout per batch in seconds.
    pub timeout_secs: u64,
    /// Maximum retries on transient failures.
    pub max_retries: u32,
    /// Backoff multiplier for retries (seconds).
    pub retry_backoff_secs: f64,
    /// Heartbeat interval for progress logging (seconds).
    pub heartbeat_secs: f64,
    /// Kill batch after this many seconds of no output.
    pub stall_kill_secs: u64,
    /// Model to use (runner-specific).
    pub model: Option<String>,
    /// Working directory for the runner.
    pub cwd: Option<String>,
}

impl Default for RunnerOpts {
    fn default() -> Self {
        Self {
            timeout_secs: 1200,
            max_retries: 1,
            retry_backoff_secs: 2.0,
            heartbeat_secs: 15.0,
            stall_kill_secs: 120,
            model: None,
            cwd: None,
        }
    }
}

/// Trait for batch execution backends.
pub trait BatchRunner: Send + Sync {
    /// Execute a single batch prompt and return the result.
    fn execute(
        &self,
        prompt: &str,
        opts: &RunnerOpts,
    ) -> impl std::future::Future<Output = BatchResult> + Send;

    /// Name of this runner backend.
    fn name(&self) -> &str;
}

/// Execute multiple batches, optionally in parallel.
pub async fn execute_batches<R: BatchRunner>(
    runner: &R,
    prompts: &[(usize, String)],
    opts: &RunnerOpts,
    max_parallel: usize,
) -> Vec<BatchResult> {
    if max_parallel <= 1 {
        // Sequential execution
        let mut results = Vec::new();
        for (index, prompt) in prompts {
            let result = execute_with_retry(runner, prompt, *index, opts).await;
            results.push(result);
        }
        return results;
    }

    // Parallel execution using tokio::JoinSet
    use tokio::sync::Semaphore;
    let semaphore = std::sync::Arc::new(Semaphore::new(max_parallel));
    let mut handles = Vec::new();

    // We need to collect results in order, so we'll use indices
    let results = std::sync::Arc::new(tokio::sync::Mutex::new(vec![None; prompts.len()]));

    for (pos, (index, prompt)) in prompts.iter().enumerate() {
        let sem = semaphore.clone();
        let prompt = prompt.clone();
        let index = *index;
        let opts = opts.clone();
        let results = results.clone();

        // We can't move runner into spawned tasks since it's borrowed,
        // so we fall back to sequential for now.
        // In practice, the CLI will call this with max_parallel=1 by default.
        let _permit = sem.acquire().await.expect("semaphore closed");
        let result = execute_with_retry(runner, &prompt, index, &opts).await;
        let mut guard = results.lock().await;
        guard[pos] = Some(result);
        handles.push(pos);
    }

    let guard = results.lock().await;
    guard.iter().filter_map(|r| r.clone()).collect()
}

/// Execute a batch with retry logic.
async fn execute_with_retry<R: BatchRunner>(
    runner: &R,
    prompt: &str,
    index: usize,
    opts: &RunnerOpts,
) -> BatchResult {
    let mut attempt = 0;
    loop {
        let start = std::time::Instant::now();
        let mut result = runner.execute(prompt, opts).await;
        result.index = index;
        result.elapsed_secs = start.elapsed().as_secs_f64();

        match result.status {
            BatchStatus::Success | BatchStatus::ParseError => return result,
            BatchStatus::Timeout | BatchStatus::ProcessError => {
                if attempt >= opts.max_retries {
                    return result;
                }
                attempt += 1;
                let backoff = opts.retry_backoff_secs * attempt as f64;
                tokio::time::sleep(std::time::Duration::from_secs_f64(backoff)).await;
                result.status = BatchStatus::Retried;
            }
            BatchStatus::Retried => return result,
        }
    }
}

/// Save batch results to a file.
pub fn save_results(results: &[BatchResult], output_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(output_dir)?;

    for result in results {
        let filename = format!("batch_{}_result.json", result.index);
        let path = output_dir.join(filename);

        let payload = serde_json::json!({
            "index": result.index,
            "status": format!("{:?}", result.status),
            "elapsed_secs": result.elapsed_secs,
            "raw_output": result.raw_output,
            "payload": result.payload,
        });

        let json = serde_json::to_string_pretty(&payload).map_err(std::io::Error::other)?;
        std::fs::write(path, json)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockRunner;

    impl BatchRunner for MockRunner {
        async fn execute(&self, _prompt: &str, _opts: &RunnerOpts) -> BatchResult {
            BatchResult {
                index: 0,
                status: BatchStatus::Success,
                payload: None,
                raw_output: r#"{"assessments": {}, "findings": []}"#.to_string(),
                elapsed_secs: 1.0,
            }
        }

        fn name(&self) -> &str {
            "mock"
        }
    }

    #[tokio::test]
    async fn sequential_execution() {
        let runner = MockRunner;
        let prompts = vec![(0, "prompt 1".to_string()), (1, "prompt 2".to_string())];
        let results = execute_batches(&runner, &prompts, &RunnerOpts::default(), 1).await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].status, BatchStatus::Success);
    }

    #[tokio::test]
    async fn retry_on_timeout() {
        struct TimeoutThenSuccessRunner {
            count: std::sync::atomic::AtomicU32,
        }

        impl BatchRunner for TimeoutThenSuccessRunner {
            async fn execute(&self, _prompt: &str, _opts: &RunnerOpts) -> BatchResult {
                let n = self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n == 0 {
                    BatchResult {
                        index: 0,
                        status: BatchStatus::Timeout,
                        payload: None,
                        raw_output: String::new(),
                        elapsed_secs: 0.0,
                    }
                } else {
                    BatchResult {
                        index: 0,
                        status: BatchStatus::Success,
                        payload: None,
                        raw_output: "ok".to_string(),
                        elapsed_secs: 0.0,
                    }
                }
            }

            fn name(&self) -> &str {
                "timeout_then_success"
            }
        }

        let runner = TimeoutThenSuccessRunner {
            count: std::sync::atomic::AtomicU32::new(0),
        };
        let opts = RunnerOpts {
            max_retries: 1,
            retry_backoff_secs: 0.01,
            ..Default::default()
        };

        let result = execute_with_retry(&runner, "prompt", 0, &opts).await;
        assert_eq!(result.status, BatchStatus::Success);
    }
}
