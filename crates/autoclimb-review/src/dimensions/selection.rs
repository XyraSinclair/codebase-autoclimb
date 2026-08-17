//! Dimension selection and validation.

use super::DimensionRegistry;

/// Validate a list of dimension keys against the registry.
/// Returns (valid, invalid) split.
pub fn validate_dimensions<'a>(
    registry: &DimensionRegistry,
    keys: &'a [String],
) -> (Vec<&'a str>, Vec<&'a str>) {
    let mut valid = Vec::new();
    let mut invalid = Vec::new();
    for key in keys {
        if registry.get(key).is_some() {
            valid.push(key.as_str());
        } else {
            invalid.push(key.as_str());
        }
    }
    (valid, invalid)
}

/// Parse a comma-separated dimension string into keys.
pub fn parse_dimension_csv(csv: &str) -> Vec<String> {
    csv.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Select dimensions for a review session.
/// If explicit list provided, use that (validated). Otherwise use defaults.
pub fn select_dimensions(registry: &DimensionRegistry, explicit: Option<&[String]>) -> Vec<String> {
    match explicit {
        Some(keys) => {
            let (valid, _) = validate_dimensions(registry, keys);
            valid.into_iter().map(|s| s.to_string()).collect()
        }
        None => registry
            .default_keys()
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

/// Filter dimensions relevant to a specific language.
/// Currently all dimensions apply to all languages; language-specific
/// guidance is handled in the prompt template via evidence_focus overrides.
pub fn dimensions_for_language(
    registry: &DimensionRegistry,
    _lang: &str,
    explicit: Option<&[String]>,
) -> Vec<String> {
    select_dimensions(registry, explicit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_csv_basic() {
        let dims = parse_dimension_csv("naming_quality, logic_clarity, type_safety");
        assert_eq!(dims, vec!["naming_quality", "logic_clarity", "type_safety"]);
    }

    #[test]
    fn parse_csv_empty() {
        let dims = parse_dimension_csv("");
        assert!(dims.is_empty());
    }

    #[test]
    fn validate_splits_correctly() {
        let reg = DimensionRegistry::new();
        let keys = vec![
            "naming_quality".to_string(),
            "bogus_dimension".to_string(),
            "logic_clarity".to_string(),
        ];
        let (valid, invalid) = validate_dimensions(&reg, &keys);
        assert_eq!(valid, vec!["naming_quality", "logic_clarity"]);
        assert_eq!(invalid, vec!["bogus_dimension"]);
    }

    #[test]
    fn select_defaults() {
        let reg = DimensionRegistry::new();
        let dims = select_dimensions(&reg, None);
        assert!(dims.len() >= 15);
        assert!(dims.contains(&"naming_quality".to_string()));
    }

    #[test]
    fn select_explicit() {
        let reg = DimensionRegistry::new();
        let explicit = vec!["naming_quality".to_string(), "logic_clarity".to_string()];
        let dims = select_dimensions(&reg, Some(&explicit));
        assert_eq!(dims.len(), 2);
    }
}
