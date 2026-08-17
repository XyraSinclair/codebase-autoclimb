use serde::{Deserialize, Serialize};
use std::fmt;

/// Finding ID: "detector::path::key"
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FindingId(pub String);

impl FindingId {
    pub fn new(detector: &str, file: &str, key: &str) -> Self {
        if key.is_empty() {
            FindingId(format!("{detector}::{file}"))
        } else {
            FindingId(format!("{detector}::{file}::{key}"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FindingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for FindingId {
    fn from(s: String) -> Self {
        FindingId(s)
    }
}

impl From<&str> for FindingId {
    fn from(s: &str) -> Self {
        FindingId(s.to_owned())
    }
}

/// Detector name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DetectorName(pub String);

impl DetectorName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DetectorName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for DetectorName {
    fn from(s: &str) -> Self {
        DetectorName(s.to_owned())
    }
}

impl From<String> for DetectorName {
    fn from(s: String) -> Self {
        DetectorName(s)
    }
}

/// Relative file path (always forward-slash, relative to project root).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RelPath(pub String);

impl RelPath {
    pub fn new(path: &str) -> Self {
        RelPath(path.replace('\\', "/"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the basename (filename component).
    pub fn basename(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    /// Check if path starts with a given prefix.
    pub fn starts_with(&self, prefix: &str) -> bool {
        self.0.starts_with(prefix)
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for RelPath {
    fn from(s: &str) -> Self {
        RelPath::new(s)
    }
}

impl From<String> for RelPath {
    fn from(s: String) -> Self {
        RelPath(s.replace('\\', "/"))
    }
}

/// ISO 8601 UTC timestamp string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub String);

impl Timestamp {
    pub fn now() -> Self {
        Timestamp(
            chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S+00:00")
                .to_string(),
        )
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for Timestamp {
    fn from(s: &str) -> Self {
        Timestamp(s.to_owned())
    }
}

impl From<String> for Timestamp {
    fn from(s: String) -> Self {
        Timestamp(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_id_format() {
        let id = FindingId::new("cycles", "src/foo.py", "cycle_1");
        assert_eq!(id.as_str(), "cycles::src/foo.py::cycle_1");

        let id2 = FindingId::new("structural", "src/big.py", "");
        assert_eq!(id2.as_str(), "structural::src/big.py");
    }

    #[test]
    fn relpath_normalizes_backslash() {
        let p = RelPath::new("src\\foo\\bar.py");
        assert_eq!(p.as_str(), "src/foo/bar.py");
        assert_eq!(p.basename(), "bar.py");
    }

    #[test]
    fn timestamp_now_is_iso() {
        let ts = Timestamp::now();
        assert!(ts.as_str().contains('T'));
        assert!(ts.as_str().contains("+00:00"));
    }
}
