use globset::{Glob, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use thiserror::Error;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

#[rustfmt::skip]
#[derive(Debug)]
pub struct Lane { pub path: PathBuf, pub base_head: String, pub base_tree: String, repo: PathBuf }

#[rustfmt::skip]
#[derive(Debug, Error)]
pub enum LaneError {
    #[error("{0}")] Message(String),
    #[error(transparent)] Io(#[from] std::io::Error),
}

#[rustfmt::skip]
#[derive(Debug, Error)]
pub enum LaneViolation {
    #[error("cannot enforce lane paths: {0}")] Invalid(String),
    #[error("lane path violations: outside write set {outside:?}, protected {protected:?}")]
    Paths { outside: Vec<String>, protected: Vec<String> },
}

impl Lane {
    pub fn create(repo: &Path, base_head: &str, lanes_dir: &Path) -> Result<Self, LaneError> {
        let actual = git(repo, &["rev-parse", "HEAD"])?;
        if actual.trim() != base_head {
            return Err(message(format!(
                "repository HEAD is {}, expected {base_head}",
                actual.trim()
            )));
        }
        let dirty = status_paths(repo)?;
        if !dirty.is_empty() {
            return Err(message(format!(
                "repository is dirty: {}",
                dirty.join(", ")
            )));
        }

        let base_tree = git(repo, &["rev-parse", &format!("{base_head}^{{tree}}")])?
            .trim()
            .to_owned();
        let short = git(repo, &["rev-parse", "--short", base_head])?;
        std::fs::create_dir_all(lanes_dir)?;
        let path = lanes_dir.join(format!(
            "lane-{}-{}-{}",
            short.trim(),
            std::process::id(),
            next_id()
        ));
        git(
            repo,
            &[
                "worktree",
                "add",
                "--detach",
                &path.to_string_lossy(),
                base_head,
            ],
        )?;
        let tmp = path.join(".autoclimb-tmp");
        std::fs::create_dir_all(&tmp)?;
        std::fs::write(tmp.join(".gitignore"), "*\n")?;
        Ok(Self {
            path,
            base_head: base_head.to_owned(),
            base_tree,
            repo: repo.to_owned(),
        })
    }

    #[rustfmt::skip]
    pub fn recorded(repo: &Path, path: PathBuf, base_head: String, base_tree: String) -> Self { Self { path, base_head, base_tree, repo: repo.to_owned() } }

    pub fn diff_paths(&self) -> Result<Vec<String>, LaneError> {
        status_paths(&self.path)
    }

    pub fn enforce(&self, write_set: &[String], protected: &[String]) -> Result<(), LaneViolation> {
        let write_set = globs(write_set)?;
        let protected = globs(protected)?;
        let paths = self
            .diff_paths()
            .map_err(|error| LaneViolation::Invalid(error.to_string()))?;
        let outside = paths
            .iter()
            .filter(|path| !write_set.is_match(path))
            .cloned()
            .collect::<Vec<_>>();
        let blocked = paths
            .into_iter()
            .filter(|path| protected.is_match(path))
            .collect::<Vec<_>>();
        if outside.is_empty() && blocked.is_empty() {
            Ok(())
        } else {
            Err(LaneViolation::Paths {
                outside,
                protected: blocked,
            })
        }
    }

    pub fn result_tree(&self) -> Result<String, LaneError> {
        let index = self.path.join(".autoclimb-tmp").join(format!(
            "index-{}-{}",
            std::process::id(),
            next_id()
        ));
        let result = (|| {
            git_index(&self.path, &index, &["read-tree", &self.base_head])?;
            git_index(&self.path, &index, &["add", "-A"])?;
            Ok(git_index(&self.path, &index, &["write-tree"])?
                .trim()
                .to_owned())
        })();
        let _ = std::fs::remove_file(index);
        result
    }

    pub fn patch(&self) -> Result<String, LaneError> {
        self.diff_tree(&[])
    }
    pub fn patch_stat(&self) -> Result<String, LaneError> {
        self.diff_tree(&["--stat"])
    }

    pub fn patch_sha256(&self) -> Result<String, LaneError> {
        Ok(hex::encode(Sha256::digest(self.patch()?.as_bytes())))
    }

    pub fn remove(self) -> Result<(), LaneError> {
        let paths = self.diff_paths()?;
        if paths.is_empty() {
            self.remove_inner()
        } else {
            Err(message(format!("lane has changes: {}", paths.join(", "))))
        }
    }

    pub fn remove_discarding(self) -> Result<(), LaneError> {
        self.remove_inner()
    }

    fn remove_inner(self) -> Result<(), LaneError> {
        git(
            &self.repo,
            &[
                "worktree",
                "remove",
                "--force",
                &self.path.to_string_lossy(),
            ],
        )?;
        git(&self.repo, &["worktree", "prune"])?;
        Ok(())
    }

    #[rustfmt::skip]
    fn diff_tree(&self, options: &[&str]) -> Result<String, LaneError> { let result_tree = self.result_tree()?; let mut args = vec!["diff"]; args.extend_from_slice(options); args.extend([self.base_tree.as_str(), result_tree.as_str()]); git(&self.path, &args) }
}

fn globs(patterns: &[String]) -> Result<GlobSet, LaneViolation> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern).map_err(|error| {
            LaneViolation::Invalid(format!("invalid glob {pattern:?}: {error}"))
        })?);
    }
    builder
        .build()
        .map_err(|error| LaneViolation::Invalid(error.to_string()))
}

fn status_paths(repo: &Path) -> Result<Vec<String>, LaneError> {
    let bytes = run(
        repo,
        &["status", "--porcelain", "-z", "--untracked-files=all"],
        None,
    )?;
    let mut fields = bytes
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty());
    let mut paths = BTreeSet::new();
    while let Some(field) = fields.next() {
        if field.len() < 4 {
            continue;
        }
        paths.insert(String::from_utf8_lossy(&field[3..]).into_owned());
        if field[0] == b'R' || field[0] == b'C' || field[1] == b'R' || field[1] == b'C' {
            if let Some(source) = fields.next() {
                paths.insert(String::from_utf8_lossy(source).into_owned());
            }
        }
    }
    Ok(paths.into_iter().collect())
}

fn git(repo: &Path, args: &[&str]) -> Result<String, LaneError> {
    String::from_utf8(run(repo, args, None)?)
        .map_err(|error| message(format!("git output was not UTF-8: {error}")))
}

fn git_index(repo: &Path, index: &Path, args: &[&str]) -> Result<String, LaneError> {
    String::from_utf8(run(repo, args, Some(index))?)
        .map_err(|error| message(format!("git output was not UTF-8: {error}")))
}

fn run(repo: &Path, args: &[&str], index: Option<&Path>) -> Result<Vec<u8>, LaneError> {
    let mut command = Command::new("git");
    command.arg("-C").arg(repo).args(args);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let output = command.output()?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(message(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn next_id() -> usize {
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn message(text: String) -> LaneError {
    LaneError::Message(text)
}
