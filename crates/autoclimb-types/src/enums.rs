use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

// ── Confidence ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
    #[default]
    Low,
}

impl Confidence {
    pub fn weight(self) -> f64 {
        match self {
            Confidence::High => 1.0,
            Confidence::Medium => 0.7,
            Confidence::Low => 0.3,
        }
    }
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Confidence::High => write!(f, "high"),
            Confidence::Medium => write!(f, "medium"),
            Confidence::Low => write!(f, "low"),
        }
    }
}

// ── Status ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Status {
    #[default]
    Open,
    Fixed,
    Wontfix,
    FalsePositive,
    AutoResolved,
    /// Legacy on-disk value; canonicalized to `Fixed` on load.
    LegacyResolved,
}

impl Status {
    /// Convert legacy status to canonical form.
    pub fn canonical(self) -> Self {
        match self {
            Status::LegacyResolved => Status::Fixed,
            other => other,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Fixed => "fixed",
            Status::Wontfix => "wontfix",
            Status::FalsePositive => "false_positive",
            Status::AutoResolved => "auto_resolved",
            Status::LegacyResolved => "resolved",
        }
    }

    /// Parse from string, accepting legacy values.
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "open" => Status::Open,
            "fixed" => Status::Fixed,
            "wontfix" => Status::Wontfix,
            "false_positive" => Status::FalsePositive,
            "auto_resolved" => Status::AutoResolved,
            "resolved" => Status::LegacyResolved,
            _ => Status::Open, // default for unknown
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for Status {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Always serialize canonical form (LegacyResolved -> "fixed")
        serializer.serialize_str(self.canonical().as_str())
    }
}

impl<'de> Deserialize<'de> for Status {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Status::from_str_loose(&s))
    }
}

// ── Tier ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Tier {
    AutoFix = 1,
    QuickFix = 2,
    #[default]
    Judgment = 3,
    MajorRefactor = 4,
}

impl Tier {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Tier::AutoFix),
            2 => Some(Tier::QuickFix),
            3 => Some(Tier::Judgment),
            4 => Some(Tier::MajorRefactor),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn weight(self) -> u32 {
        match self {
            Tier::AutoFix => 1,
            Tier::QuickFix => 2,
            Tier::Judgment => 3,
            Tier::MajorRefactor => 4,
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_u8())
    }
}

// Custom serde: serialize as integer, deserialize from int or string
impl Serialize for Tier {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.as_u8())
    }
}

impl<'de> Deserialize<'de> for Tier {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TierVisitor;

        impl<'de> serde::de::Visitor<'de> for TierVisitor {
            type Value = Tier;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("integer 1-4 or string \"1\"-\"4\"")
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Tier, E> {
                Tier::from_u8(v as u8).ok_or_else(|| E::custom(format!("invalid tier: {v}")))
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Tier, E> {
                Tier::from_u8(v as u8).ok_or_else(|| E::custom(format!("invalid tier: {v}")))
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Tier, E> {
                let n: u8 = v.trim().parse().map_err(E::custom)?;
                Tier::from_u8(n).ok_or_else(|| E::custom(format!("invalid tier: {n}")))
            }
        }

        deserializer.deserialize_any(TierVisitor)
    }
}

// ── Zone ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Zone {
    #[default]
    Production,
    Test,
    Config,
    Generated,
    Script,
    Vendor,
}

impl Zone {
    /// Zones excluded from scoring.
    pub fn is_scoring_excluded(self) -> bool {
        matches!(
            self,
            Zone::Test | Zone::Config | Zone::Generated | Zone::Vendor
        )
    }
}

impl fmt::Display for Zone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Zone::Production => write!(f, "production"),
            Zone::Test => write!(f, "test"),
            Zone::Config => write!(f, "config"),
            Zone::Generated => write!(f, "generated"),
            Zone::Script => write!(f, "script"),
            Zone::Vendor => write!(f, "vendor"),
        }
    }
}

// ── ActionType ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    AutoFix,
    Reorganize,
    Refactor,
    ManualFix,
}

impl ActionType {
    pub fn label(self) -> &'static str {
        match self {
            ActionType::AutoFix => "fix",
            ActionType::Reorganize => "move",
            ActionType::Refactor => "refactor",
            ActionType::ManualFix => "manual",
        }
    }

    pub fn priority(self) -> u8 {
        match self {
            ActionType::AutoFix => 0,
            ActionType::Reorganize => 1,
            ActionType::Refactor => 2,
            ActionType::ManualFix => 3,
        }
    }
}

// ── ScoreMode ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreMode {
    Lenient,
    Strict,
    VerifiedStrict,
}

impl ScoreMode {
    pub const ALL: [ScoreMode; 3] = [
        ScoreMode::Lenient,
        ScoreMode::Strict,
        ScoreMode::VerifiedStrict,
    ];

    /// Which statuses count as failures in this scoring mode.
    pub fn failure_statuses(self) -> &'static [Status] {
        match self {
            ScoreMode::Lenient => &[Status::Open],
            ScoreMode::Strict => &[Status::Open, Status::Wontfix],
            ScoreMode::VerifiedStrict => &[
                Status::Open,
                Status::Wontfix,
                Status::Fixed,
                Status::FalsePositive,
            ],
        }
    }

    /// Check if a status counts as a failure in this scoring mode.
    pub fn is_failure(self, status: Status) -> bool {
        self.failure_statuses().contains(&status.canonical())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_serde_roundtrip() {
        let json = serde_json::to_string(&Confidence::High).unwrap();
        assert_eq!(json, "\"high\"");
        let parsed: Confidence = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Confidence::High);
    }

    #[test]
    fn confidence_weights() {
        assert_eq!(Confidence::High.weight(), 1.0);
        assert_eq!(Confidence::Medium.weight(), 0.7);
        assert_eq!(Confidence::Low.weight(), 0.3);
    }

    #[test]
    fn status_serde_canonical() {
        // LegacyResolved serializes as "fixed"
        let json = serde_json::to_string(&Status::LegacyResolved).unwrap();
        assert_eq!(json, "\"fixed\"");

        // "resolved" deserializes as LegacyResolved
        let parsed: Status = serde_json::from_str("\"resolved\"").unwrap();
        assert_eq!(parsed, Status::LegacyResolved);
        assert_eq!(parsed.canonical(), Status::Fixed);
    }

    #[test]
    fn status_all_variants_roundtrip() {
        for status in [
            Status::Open,
            Status::Fixed,
            Status::Wontfix,
            Status::FalsePositive,
            Status::AutoResolved,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: Status = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn status_unknown_defaults_to_open() {
        let parsed: Status = serde_json::from_str("\"garbage\"").unwrap();
        assert_eq!(parsed, Status::Open);
    }

    #[test]
    fn tier_serde_as_integer() {
        let json = serde_json::to_string(&Tier::Judgment).unwrap();
        assert_eq!(json, "3");
    }

    #[test]
    fn tier_deserialize_from_int() {
        let parsed: Tier = serde_json::from_str("3").unwrap();
        assert_eq!(parsed, Tier::Judgment);
    }

    #[test]
    fn tier_deserialize_from_string() {
        let parsed: Tier = serde_json::from_str("\"3\"").unwrap();
        assert_eq!(parsed, Tier::Judgment);
    }

    #[test]
    fn zone_scoring_excluded() {
        assert!(!Zone::Production.is_scoring_excluded());
        assert!(Zone::Test.is_scoring_excluded());
        assert!(Zone::Config.is_scoring_excluded());
        assert!(Zone::Generated.is_scoring_excluded());
        assert!(!Zone::Script.is_scoring_excluded());
        assert!(Zone::Vendor.is_scoring_excluded());
    }

    #[test]
    fn score_mode_failure_statuses() {
        assert_eq!(ScoreMode::Lenient.failure_statuses(), &[Status::Open]);
        assert!(ScoreMode::Strict.is_failure(Status::Wontfix));
        assert!(!ScoreMode::Lenient.is_failure(Status::Wontfix));
        assert!(ScoreMode::VerifiedStrict.is_failure(Status::Fixed));
        // LegacyResolved canonicalizes to Fixed, which is a failure in verified_strict
        assert!(ScoreMode::VerifiedStrict.is_failure(Status::LegacyResolved));
    }
}
