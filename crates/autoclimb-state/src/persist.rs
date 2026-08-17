use std::fs;
use std::io::Write;
use std::path::Path;

use fs2::FileExt;

use autoclimb_types::state::StateModel;

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Load state from a JSON file.
///
/// Post-deserialize: canonicalizes legacy status values.
pub fn load_state(path: &Path) -> Result<StateModel, StateError> {
    let contents = fs::read_to_string(path)?;
    let mut state: StateModel = serde_json::from_str(&contents)?;
    state.canonicalize_findings();
    Ok(state)
}

/// Save state to a JSON file with atomic write.
///
/// Writes to a PID-unique temp file first, then renames for atomicity.
/// Uses pretty-print JSON with sorted keys (BTreeMap guarantees ordering).
pub fn save_state(state: &StateModel, path: &Path) -> Result<(), StateError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(state)?;

    // Atomic write: write to PID-unique .tmp, then rename
    let tmp_ext = format!("json.{}.tmp", std::process::id());
    let tmp_path = path.with_extension(tmp_ext);
    let mut file = fs::File::create(&tmp_path)?;
    file.write_all(json.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(&tmp_path, path)?;

    Ok(())
}

/// Load state or create empty if file doesn't exist.
pub fn load_or_create(path: &Path) -> Result<StateModel, StateError> {
    if path.exists() {
        load_state(path)
    } else {
        Ok(StateModel::empty())
    }
}

/// Execute a function with an exclusive file lock on state.
///
/// Prevents parallel agents from clobbering each other's state:
/// 1. Acquires exclusive flock on `<path>.lock`
/// 2. Loads current state (or creates empty)
/// 3. Calls `f(&mut state)`
/// 4. Saves updated state atomically
/// 5. Releases lock
pub fn with_locked_state<F>(path: &Path, f: F) -> Result<StateModel, StateError>
where
    F: FnOnce(&mut StateModel),
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let lock_path = path.with_extension("json.lock");
    let lock_file = fs::File::create(&lock_path)?;
    lock_file.lock_exclusive()?;

    let result = (|| {
        let mut state = load_or_create(path)?;
        f(&mut state);
        save_state(&state, path)?;
        Ok(state)
    })();

    // Always release lock, even on error
    let _ = lock_file.unlock();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");

        let state = StateModel::empty();
        save_state(&state, &path).unwrap();

        let loaded = load_state(&path).unwrap();
        assert_eq!(loaded.version, state.version);
        assert_eq!(loaded.scan_count, 0);
    }

    #[test]
    fn load_or_create_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");

        let state = load_or_create(&path).unwrap();
        assert_eq!(state.version, 1);
    }

    #[test]
    fn atomic_write_creates_parent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sub").join("dir").join("state.json");

        let state = StateModel::empty();
        save_state(&state, &path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn with_locked_state_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("locked_state.json");

        let state = with_locked_state(&path, |s| {
            s.scan_count = 42;
        })
        .unwrap();

        assert_eq!(state.scan_count, 42);

        // Verify persisted
        let loaded = load_state(&path).unwrap();
        assert_eq!(loaded.scan_count, 42);
    }

    #[test]
    fn unique_tmp_filename_uses_pid() {
        // Verify save_state doesn't leave stale tmp files from other PIDs
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");

        let state = StateModel::empty();
        save_state(&state, &path).unwrap();

        // The PID-specific tmp should have been renamed away
        let tmp_path = path.with_extension(format!("json.{}.tmp", std::process::id()));
        assert!(
            !tmp_path.exists(),
            "Temp file should be cleaned up by rename"
        );
        assert!(path.exists());
    }
}
