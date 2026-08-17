//! Scorecard image generation (stub).
//!
//! Full implementation requires `image`, `imageproc`, and `rusttype` crates
//! behind a cargo feature flag. This stub provides the API surface.

use std::collections::BTreeMap;
use std::path::Path;

/// Scorecard configuration.
pub struct ScorecardConfig {
    pub title: String,
    pub overall_score: f64,
    pub strict_score: f64,
    pub target_strict: f64,
    pub dimensions: BTreeMap<String, f64>,
    pub strict_dimensions: BTreeMap<String, f64>,
}

/// Generate a scorecard image at the given path.
///
/// Returns `Ok(true)` if generated, `Ok(false)` if image feature not available.
pub fn generate_scorecard(
    _config: &ScorecardConfig,
    _output: &Path,
) -> Result<bool, Box<dyn std::error::Error>> {
    // Stub: image generation not yet available.
    // Enable the `image` cargo feature to activate real rendering.
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_returns_false() {
        let config = ScorecardConfig {
            title: "test".into(),
            overall_score: 85.0,
            strict_score: 80.0,
            target_strict: 95.0,
            dimensions: BTreeMap::new(),
            strict_dimensions: BTreeMap::new(),
        };
        assert!(!generate_scorecard(&config, Path::new("out.png")).unwrap());
    }
}
