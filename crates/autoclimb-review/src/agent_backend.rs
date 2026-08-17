use crate::runner::RunnerOpts;
use crate::runner_codex::{CodexCapture, CodexExit, CodexRunner};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static TRANSCRIPT_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[rustfmt::skip]
#[derive(Debug, Clone)]
pub struct Brief { pub text: String, pub hash: String }

#[rustfmt::skip]
impl Brief {
    pub fn new(text: impl Into<String>) -> Self { let text = text.into(); let hash = sha256(&text); Self { text, hash } }
}

#[rustfmt::skip]
#[derive(Debug, Clone, Copy)]
pub struct AttemptBudget { pub wall_secs: u64, pub max_attempts: u32 }

#[rustfmt::skip]
#[derive(Debug, Clone)]
pub struct AttemptRecord {
    pub backend: String, pub brief_hash: String, pub started_at: String, pub ended_at: String,
    pub exit: String, pub transcript_path: PathBuf, pub transcript_hash: String,
}

pub trait AgentBackend {
    fn run(&self, brief: &Brief, lane_path: &Path, budget: &AttemptBudget) -> AttemptRecord;
}

#[derive(Default)]
pub struct CodexBackend {
    pub runner: CodexRunner,
}

impl AgentBackend for CodexBackend {
    fn run(&self, brief: &Brief, lane_path: &Path, budget: &AttemptBudget) -> AttemptRecord {
        let started_at = timestamp();
        let brief_hash = sha256(&brief.text);
        let captures = if brief_hash != brief.hash {
            vec![CodexCapture {
                exit: CodexExit::Error(None),
                stdout: String::new(),
                stderr: "brief hash does not match brief text".to_owned(),
                elapsed_secs: 0.0,
            }]
        } else if budget.max_attempts == 0 || budget.wall_secs == 0 {
            vec![CodexCapture {
                exit: CodexExit::Error(None),
                stdout: String::new(),
                stderr: "attempt budget permits no execution".to_owned(),
                elapsed_secs: 0.0,
            }]
        } else {
            std::thread::scope(|scope| {
                scope
                    .spawn(|| {
                        let runtime = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .expect("tokio runtime");
                        let started = std::time::Instant::now();
                        let mut captures = Vec::new();
                        for _ in 0..budget.max_attempts {
                            let remaining =
                                budget.wall_secs.saturating_sub(started.elapsed().as_secs());
                            if remaining == 0 {
                                captures.push(CodexCapture {
                                    exit: CodexExit::Timeout,
                                    stdout: String::new(),
                                    stderr: "wall-time budget exhausted".to_owned(),
                                    elapsed_secs: started.elapsed().as_secs_f64(),
                                });
                                break;
                            }
                            let opts = RunnerOpts {
                                timeout_secs: remaining,
                                stall_kill_secs: remaining
                                    .min(RunnerOpts::default().stall_kill_secs),
                                cwd: Some(lane_path.to_string_lossy().into_owned()),
                                ..RunnerOpts::default()
                            };
                            let capture =
                                runtime.block_on(self.runner.execute_capture(&brief.text, &opts));
                            let done = capture.exit == CodexExit::Success;
                            captures.push(capture);
                            if done {
                                break;
                            }
                        }
                        captures
                    })
                    .join()
                    .unwrap_or_else(|_| {
                        vec![CodexCapture {
                            exit: CodexExit::Error(None),
                            stdout: String::new(),
                            stderr: "codex runner thread panicked".to_owned(),
                            elapsed_secs: 0.0,
                        }]
                    })
            })
        };

        let transcript = captures
            .iter()
            .enumerate()
            .map(|(index, capture)| {
                format!(
                    "attempt {} ({:.3}s):\nstdout:\n{}\n\nstderr:\n{}\n",
                    index + 1,
                    capture.elapsed_secs,
                    capture.stdout,
                    capture.stderr
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let transcript_path = lane_path.join(".autoclimb-tmp").join(format!(
            "transcript-{}.txt",
            TRANSCRIPT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let mut exit = exit_name(captures.last().expect("at least one capture").exit);
        if let Some(parent) = transcript_path.parent() {
            if std::fs::create_dir_all(parent)
                .and_then(|()| std::fs::write(&transcript_path, &transcript))
                .is_err()
            {
                exit = "error:-1".to_owned();
            }
        }
        AttemptRecord {
            backend: "codex".to_owned(),
            brief_hash,
            started_at,
            ended_at: timestamp(),
            exit,
            transcript_path,
            transcript_hash: sha256(&transcript),
        }
    }
}

fn exit_name(exit: CodexExit) -> String {
    match exit {
        CodexExit::Success => "success".to_owned(),
        CodexExit::Timeout => "timeout".to_owned(),
        CodexExit::Stall => "stall".to_owned(),
        CodexExit::Error(code) => format!("error:{}", code.unwrap_or(-1)),
    }
}

fn sha256(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

fn timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before Unix epoch")
        .as_secs()
        .to_string()
}
