use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid value: {0}")]
    InvalidValue(String),
}

/// Metadata attached to an ignore pattern.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IgnoreMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_at: Option<String>,
}

/// Project-level configuration stored in `.autoclimb/config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default = "default_target_strict_score")]
    pub target_strict_score: u32,

    #[serde(default = "default_review_max_age_days")]
    pub review_max_age_days: u32,

    #[serde(default = "default_review_batch_max_files")]
    pub review_batch_max_files: u32,

    #[serde(default = "default_holistic_max_age_days")]
    pub holistic_max_age_days: u32,

    #[serde(default = "default_generate_scorecard")]
    pub generate_scorecard: bool,

    #[serde(default = "default_badge_path")]
    pub badge_path: String,

    #[serde(default)]
    pub exclude: Vec<String>,

    #[serde(default)]
    pub ignore: Vec<String>,

    #[serde(default)]
    pub ignore_metadata: BTreeMap<String, IgnoreMeta>,

    #[serde(default)]
    pub zone_overrides: BTreeMap<String, String>,

    #[serde(default)]
    pub review_dimensions: Vec<String>,

    #[serde(default)]
    pub large_files_threshold: u32,

    #[serde(default)]
    pub props_threshold: u32,

    #[serde(default = "default_finding_noise_budget")]
    pub finding_noise_budget: u32,

    #[serde(default)]
    pub finding_noise_global_budget: u32,

    #[serde(default)]
    pub needs_rescan: bool,

    #[serde(default)]
    pub languages: BTreeMap<String, serde_json::Value>,

    /// Preserve unknown fields for forward compatibility.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

fn default_target_strict_score() -> u32 {
    95
}
fn default_review_max_age_days() -> u32 {
    30
}
fn default_review_batch_max_files() -> u32 {
    80
}
fn default_holistic_max_age_days() -> u32 {
    30
}
fn default_generate_scorecard() -> bool {
    true
}
fn default_badge_path() -> String {
    "scorecard.png".into()
}
fn default_finding_noise_budget() -> u32 {
    10
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            target_strict_score: default_target_strict_score(),
            review_max_age_days: default_review_max_age_days(),
            review_batch_max_files: default_review_batch_max_files(),
            holistic_max_age_days: default_holistic_max_age_days(),
            generate_scorecard: default_generate_scorecard(),
            badge_path: default_badge_path(),
            exclude: Vec::new(),
            ignore: Vec::new(),
            ignore_metadata: BTreeMap::new(),
            zone_overrides: BTreeMap::new(),
            review_dimensions: Vec::new(),
            large_files_threshold: 0,
            props_threshold: 0,
            finding_noise_budget: default_finding_noise_budget(),
            finding_noise_global_budget: 0,
            needs_rescan: false,
            languages: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }
}

/// Return a config with all defaults.
pub fn default_config() -> ProjectConfig {
    ProjectConfig::default()
}

/// Load config from a JSON file. Missing keys get defaults.
pub fn load_config(path: &Path) -> Result<ProjectConfig, ConfigError> {
    let data = fs::read_to_string(path)?;
    let config: ProjectConfig = serde_json::from_str(&data)?;
    Ok(config)
}

/// Load config or return defaults if the file doesn't exist.
pub fn load_or_default(path: &Path) -> ProjectConfig {
    match load_config(path) {
        Ok(c) => c,
        Err(_) => default_config(),
    }
}

/// Save config to disk with atomic write (temp file + rename).
pub fn save_config(config: &ProjectConfig, path: &Path) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, json.as_bytes())?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Parse a raw CLI string into the appropriate type and set it on the config.
pub fn set_config_value(
    config: &mut ProjectConfig,
    key: &str,
    raw: &str,
) -> Result<(), ConfigError> {
    match key {
        "target_strict_score" => {
            let v: u32 = raw
                .parse()
                .map_err(|_| ConfigError::InvalidValue(format!("expected integer: {raw}")))?;
            if v > 100 {
                return Err(ConfigError::InvalidValue(
                    "target_strict_score must be 0-100".into(),
                ));
            }
            config.target_strict_score = v;
        }
        "review_max_age_days" | "holistic_max_age_days" => {
            let v = parse_age(raw)?;
            if key == "review_max_age_days" {
                config.review_max_age_days = v;
            } else {
                config.holistic_max_age_days = v;
            }
        }
        "review_batch_max_files" => {
            config.review_batch_max_files = raw
                .parse()
                .map_err(|_| ConfigError::InvalidValue(format!("expected integer: {raw}")))?;
        }
        "generate_scorecard" => {
            config.generate_scorecard = parse_bool(raw)?;
        }
        "badge_path" => {
            if raw.ends_with('/') || raw.is_empty() {
                return Err(ConfigError::InvalidValue(
                    "badge_path must be a filename".into(),
                ));
            }
            config.badge_path = raw.to_string();
        }
        "large_files_threshold" => {
            config.large_files_threshold = raw
                .parse()
                .map_err(|_| ConfigError::InvalidValue(format!("expected integer: {raw}")))?;
        }
        "props_threshold" => {
            config.props_threshold = raw
                .parse()
                .map_err(|_| ConfigError::InvalidValue(format!("expected integer: {raw}")))?;
        }
        "finding_noise_budget" => {
            config.finding_noise_budget = raw
                .parse()
                .map_err(|_| ConfigError::InvalidValue(format!("expected integer: {raw}")))?;
        }
        "finding_noise_global_budget" => {
            config.finding_noise_global_budget = raw
                .parse()
                .map_err(|_| ConfigError::InvalidValue(format!("expected integer: {raw}")))?;
        }
        "needs_rescan" => {
            config.needs_rescan = parse_bool(raw)?;
        }
        _ => {
            return Err(ConfigError::InvalidValue(format!(
                "unknown or non-settable key: {key}"
            )));
        }
    }
    Ok(())
}

/// Reset a key to its default value.
pub fn unset_config_value(config: &mut ProjectConfig, key: &str) -> Result<(), ConfigError> {
    let defaults = default_config();
    match key {
        "target_strict_score" => config.target_strict_score = defaults.target_strict_score,
        "review_max_age_days" => config.review_max_age_days = defaults.review_max_age_days,
        "review_batch_max_files" => config.review_batch_max_files = defaults.review_batch_max_files,
        "holistic_max_age_days" => config.holistic_max_age_days = defaults.holistic_max_age_days,
        "generate_scorecard" => config.generate_scorecard = defaults.generate_scorecard,
        "badge_path" => config.badge_path = defaults.badge_path,
        "exclude" => config.exclude = Vec::new(),
        "ignore" => config.ignore = Vec::new(),
        "ignore_metadata" => config.ignore_metadata = BTreeMap::new(),
        "zone_overrides" => config.zone_overrides = BTreeMap::new(),
        "review_dimensions" => config.review_dimensions = Vec::new(),
        "large_files_threshold" => config.large_files_threshold = 0,
        "props_threshold" => config.props_threshold = 0,
        "finding_noise_budget" => config.finding_noise_budget = defaults.finding_noise_budget,
        "finding_noise_global_budget" => config.finding_noise_global_budget = 0,
        "needs_rescan" => config.needs_rescan = false,
        "languages" => config.languages = BTreeMap::new(),
        _ => {
            return Err(ConfigError::InvalidValue(format!("unknown key: {key}")));
        }
    }
    Ok(())
}

/// Append a pattern to the ignore list (deduplicates).
pub fn add_ignore_pattern(config: &mut ProjectConfig, pattern: &str) {
    let p = pattern.to_string();
    if !config.ignore.contains(&p) {
        config.ignore.push(p);
    }
}

/// Append a pattern to the exclude list (deduplicates).
pub fn add_exclude_pattern(config: &mut ProjectConfig, pattern: &str) {
    let p = pattern.to_string();
    if !config.exclude.contains(&p) {
        config.exclude.push(p);
    }
}

/// Record metadata for an ignore pattern.
pub fn set_ignore_metadata(config: &mut ProjectConfig, pattern: &str, note: &str, added_at: &str) {
    config.ignore_metadata.insert(
        pattern.to_string(),
        IgnoreMeta {
            note: Some(note.to_string()),
            added_at: Some(added_at.to_string()),
        },
    );
}

fn parse_bool(s: &str) -> Result<bool, ConfigError> {
    match s.to_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(ConfigError::InvalidValue(format!("expected boolean: {s}"))),
    }
}

fn parse_age(s: &str) -> Result<u32, ConfigError> {
    if s == "never" {
        return Ok(0);
    }
    s.parse()
        .map_err(|_| ConfigError::InvalidValue(format!("expected integer or 'never': {s}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_config_values() {
        let c = default_config();
        assert_eq!(c.target_strict_score, 95);
        assert_eq!(c.review_max_age_days, 30);
        assert_eq!(c.review_batch_max_files, 80);
        assert_eq!(c.holistic_max_age_days, 30);
        assert!(c.generate_scorecard);
        assert_eq!(c.badge_path, "scorecard.png");
        assert_eq!(c.finding_noise_budget, 10);
        assert_eq!(c.finding_noise_global_budget, 0);
        assert!(!c.needs_rescan);
    }

    #[test]
    fn roundtrip_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");

        let mut config = default_config();
        config.target_strict_score = 80;
        config.exclude = vec!["node_modules".into(), "dist".into()];
        config.zone_overrides.insert("a.py".into(), "test".into());

        save_config(&config, &path).unwrap();
        let loaded = load_config(&path).unwrap();

        assert_eq!(loaded.target_strict_score, 80);
        assert_eq!(loaded.exclude, vec!["node_modules", "dist"]);
        assert_eq!(loaded.zone_overrides.get("a.py").unwrap(), "test");
        // Defaults filled in
        assert_eq!(loaded.review_max_age_days, 30);
    }

    #[test]
    fn missing_keys_get_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"target_strict_score": 42}"#).unwrap();

        let config = load_config(&path).unwrap();
        assert_eq!(config.target_strict_score, 42);
        assert_eq!(config.review_max_age_days, 30); // default
        assert!(config.generate_scorecard); // default
    }

    #[test]
    fn unknown_fields_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"target_strict_score": 95, "future_field": "hello"}"#,
        )
        .unwrap();

        let config = load_config(&path).unwrap();
        assert_eq!(
            config.extra.get("future_field").unwrap(),
            &serde_json::json!("hello")
        );

        // Roundtrip preserves the unknown field
        save_config(&config, &path).unwrap();
        let reloaded = load_config(&path).unwrap();
        assert_eq!(
            reloaded.extra.get("future_field").unwrap(),
            &serde_json::json!("hello")
        );
    }

    #[test]
    fn set_config_value_integer() {
        let mut c = default_config();
        set_config_value(&mut c, "target_strict_score", "80").unwrap();
        assert_eq!(c.target_strict_score, 80);
    }

    #[test]
    fn set_config_value_score_range() {
        let mut c = default_config();
        assert!(set_config_value(&mut c, "target_strict_score", "101").is_err());
    }

    #[test]
    fn set_config_value_bool() {
        let mut c = default_config();
        set_config_value(&mut c, "generate_scorecard", "false").unwrap();
        assert!(!c.generate_scorecard);
        set_config_value(&mut c, "generate_scorecard", "yes").unwrap();
        assert!(c.generate_scorecard);
    }

    #[test]
    fn set_config_value_age_never() {
        let mut c = default_config();
        set_config_value(&mut c, "review_max_age_days", "never").unwrap();
        assert_eq!(c.review_max_age_days, 0);
    }

    #[test]
    fn unset_restores_default() {
        let mut c = default_config();
        c.target_strict_score = 50;
        unset_config_value(&mut c, "target_strict_score").unwrap();
        assert_eq!(c.target_strict_score, 95);
    }

    #[test]
    fn add_ignore_deduplicates() {
        let mut c = default_config();
        add_ignore_pattern(&mut c, "smells::*");
        add_ignore_pattern(&mut c, "smells::*");
        assert_eq!(c.ignore.len(), 1);
    }

    #[test]
    fn add_exclude_deduplicates() {
        let mut c = default_config();
        add_exclude_pattern(&mut c, "dist");
        add_exclude_pattern(&mut c, "dist");
        assert_eq!(c.exclude.len(), 1);
    }

    #[test]
    fn ignore_metadata_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");

        let mut config = default_config();
        set_ignore_metadata(&mut config, "smells::*", "noise", "2026-01-01T00:00:00Z");
        save_config(&config, &path).unwrap();

        let loaded = load_config(&path).unwrap();
        let meta = loaded.ignore_metadata.get("smells::*").unwrap();
        assert_eq!(meta.note.as_deref(), Some("noise"));
        assert_eq!(meta.added_at.as_deref(), Some("2026-01-01T00:00:00Z"));
    }

    #[test]
    fn load_or_default_missing_file() {
        let config = load_or_default(Path::new("/nonexistent/config.json"));
        assert_eq!(config.target_strict_score, 95);
    }

    #[test]
    fn badge_path_validation() {
        let mut c = default_config();
        assert!(set_config_value(&mut c, "badge_path", "assets/").is_err());
        assert!(set_config_value(&mut c, "badge_path", "").is_err());
        set_config_value(&mut c, "badge_path", "assets/score.png").unwrap();
        assert_eq!(c.badge_path, "assets/score.png");
    }
}
