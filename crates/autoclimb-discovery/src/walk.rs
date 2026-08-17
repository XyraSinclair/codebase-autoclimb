use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Configuration for file discovery.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// File extensions to include (e.g. ["py"]).
    pub extensions: Vec<String>,
    /// Directory names to always skip.
    pub skip_dirs: Vec<String>,
    /// Glob patterns to exclude (substring match on relative path).
    pub exclude_patterns: Vec<String>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        DiscoveryConfig {
            extensions: Vec::new(),
            skip_dirs: vec![
                ".git".into(),
                "__pycache__".into(),
                "node_modules".into(),
                ".tox".into(),
                ".mypy_cache".into(),
                ".pytest_cache".into(),
                ".venv".into(),
                "venv".into(),
                ".eggs".into(),
                "dist".into(),
                "build".into(),
                ".autoclimb".into(),
            ],
            exclude_patterns: Vec::new(),
        }
    }
}

/// Discover source files under `root` matching the config.
///
/// Returns relative paths (forward-slash separated) sorted alphabetically.
pub fn find_source_files(root: &Path, config: &DiscoveryConfig) -> Vec<String> {
    let root = match root.canonicalize() {
        Ok(p) => p,
        Err(_) => root.to_path_buf(),
    };

    let mut files: Vec<String> = Vec::new();

    for entry in WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy();
                !config.skip_dirs.iter().any(|d| d == name.as_ref())
            } else {
                true
            }
        })
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();

        // Extension filter
        if !config.extensions.is_empty() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !config.extensions.iter().any(|e| e == ext) {
                continue;
            }
        }

        // Relative path
        let rel = match path.strip_prefix(&root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");

        // Exclude patterns (substring match)
        if config
            .exclude_patterns
            .iter()
            .any(|p| matches_exclusion(&rel_str, p))
        {
            continue;
        }

        files.push(rel_str);
    }

    files.sort();
    files
}

/// Check if a path matches an exclusion pattern (substring or glob-like).
pub fn matches_exclusion(path: &str, pattern: &str) -> bool {
    // Simple substring match, matching Python behavior
    if pattern.contains('*') {
        // Basic glob: convert to simple check
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            let (prefix, suffix) = (parts[0], parts[1]);
            if prefix.is_empty() {
                return path.ends_with(suffix);
            }
            if suffix.is_empty() {
                return path.starts_with(prefix);
            }
            return path.contains(prefix) && path.ends_with(suffix);
        }
        // Multi-glob: substring fallback
        return path.contains(pattern.trim_matches('*'));
    }
    path.contains(pattern)
}

/// Convert an absolute path to a relative path string.
pub fn to_relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Resolve a relative path against a root.
pub fn resolve_path(rel: &str, root: &Path) -> PathBuf {
    root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn matches_exclusion_substring() {
        assert!(matches_exclusion("src/vendor/lib.py", "vendor"));
        assert!(!matches_exclusion("src/main.py", "vendor"));
    }

    #[test]
    fn matches_exclusion_glob() {
        assert!(matches_exclusion("src/test_foo.py", "*.py"));
        assert!(!matches_exclusion("src/test_foo.rs", "*.py"));
    }

    #[test]
    fn find_source_files_basic() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create some files
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.py"), "# main").unwrap();
        fs::write(root.join("src/utils.py"), "# utils").unwrap();
        fs::write(root.join("src/readme.txt"), "readme").unwrap();
        fs::create_dir_all(root.join("__pycache__")).unwrap();
        fs::write(root.join("__pycache__/main.pyc"), "bytecode").unwrap();

        let config = DiscoveryConfig {
            extensions: vec!["py".into()],
            ..Default::default()
        };

        let files = find_source_files(root, &config);
        assert_eq!(files, vec!["src/main.py", "src/utils.py"]);
    }
}
