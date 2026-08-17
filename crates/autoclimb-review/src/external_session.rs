//! External review session management.
//!
//! Supports creating external review sessions (e.g., for Claude),
//! generating blind packets, and importing submitted results.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use autoclimb_types::state::StateModel;

use crate::prepare::{self, PrepareOptions, ReviewPacket};
use crate::trust;

fn cli_command(args: &str) -> String {
    let base = std::env::var("AUTOCLIMB_CMD")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "autoclimb".to_string());

    if args.is_empty() {
        base
    } else {
        format!("{base} {args}")
    }
}

/// External session metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalSession {
    /// Unique session identifier.
    pub session_id: String,
    /// When the session was created.
    pub created: String,
    /// Runner backend (e.g., "claude").
    pub runner: String,
    /// TTL in hours.
    pub ttl_hours: u32,
    /// Blind packet hash for verification.
    pub blind_packet_hash: String,
    /// Status: "pending", "submitted", "expired".
    pub status: String,
}

/// Create a new external review session.
pub fn create_session(
    state: &StateModel,
    project_root: &Path,
    runner: &str,
    ttl_hours: u32,
    opts: &PrepareOptions,
) -> std::io::Result<(ExternalSession, ReviewPacket)> {
    let now = autoclimb_types::newtypes::Timestamp::now();
    let random_suffix: u32 = rand::random::<u32>() % 10000;
    let session_id = format!(
        "ext_{}_{}",
        now.0
            .replace([':', '-', '+', 'T'], "")
            .chars()
            .take(14)
            .collect::<String>(),
        random_suffix
    );

    // Prepare the review packet
    let packet = prepare::prepare_review_packet(state, project_root, opts);

    // Create blind variant and hash it
    let blind = prepare::make_blind_packet(&packet);
    let blind_json = serde_json::to_string_pretty(&blind).map_err(std::io::Error::other)?;
    let blind_hash = trust::hash_packet(&blind_json);

    // Create session directory
    let session_dir = session_directory(project_root, &session_id);
    std::fs::create_dir_all(&session_dir)?;

    // Write session metadata
    let session = ExternalSession {
        session_id: session_id.clone(),
        created: now.0,
        runner: runner.to_string(),
        ttl_hours,
        blind_packet_hash: blind_hash,
        status: "pending".to_string(),
    };

    let session_json = serde_json::to_string_pretty(&session).map_err(std::io::Error::other)?;
    std::fs::write(session_dir.join("session.json"), session_json)?;

    // Write blind packet
    std::fs::write(session_dir.join("review_packet_blind.json"), blind_json)?;

    // Write launch prompt
    let launch_prompt = generate_launch_prompt(&session, &blind, project_root);
    std::fs::write(session_dir.join("launch_prompt.md"), launch_prompt)?;

    Ok((session, packet))
}

/// Load an existing session by ID.
pub fn load_session(project_root: &Path, session_id: &str) -> std::io::Result<ExternalSession> {
    let session_dir = session_directory(project_root, session_id);
    let session_json = std::fs::read_to_string(session_dir.join("session.json"))?;
    serde_json::from_str(&session_json).map_err(std::io::Error::other)
}

/// Get the session directory path.
fn session_directory(project_root: &Path, session_id: &str) -> PathBuf {
    project_root
        .join(".autoclimb")
        .join("external_review_sessions")
        .join(session_id)
}

/// Generate the launch prompt for an external reviewer.
fn generate_launch_prompt(
    session: &ExternalSession,
    packet: &ReviewPacket,
    project_root: &Path,
) -> String {
    let dims: Vec<String> = packet
        .batches
        .iter()
        .flat_map(|b| b.dimensions.iter())
        .cloned()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let files: Vec<String> = packet
        .batches
        .iter()
        .flat_map(|b| b.files_to_read.iter())
        .cloned()
        .collect();

    format!(
        "# External Code Review Session\n\n\
         Session ID: {session_id}\n\
         Runner: {runner}\n\
         Repository: {repo}\n\n\
         ## Instructions\n\n\
         You are conducting a blind holistic code review. You do NOT have access \
         to the current scores — assess each dimension independently based on \
         the code you read.\n\n\
         ## Dimensions to Assess\n\n\
         {dim_list}\n\n\
         ## Files to Review\n\n\
         {file_list}\n\n\
         ## Output Format\n\n\
         Return a single JSON object with:\n\
         - `assessments`: dimension → score (0-100, one decimal)\n\
         - `findings`: array of finding objects\n\
         - `dimension_notes`: dimension → notes object\n\n\
        Save your output as a JSON file and submit with:\n\
         ```\n\
         {submit_cmd} <your_file.json>\n\
         ```",
        session_id = session.session_id,
        runner = session.runner,
        repo = project_root.display(),
        submit_cmd = cli_command(&format!(
            "review --external-submit {} --import",
            session.session_id
        )),
        dim_list = dims
            .iter()
            .map(|d| format!("- {d}"))
            .collect::<Vec<_>>()
            .join("\n"),
        file_list = if files.is_empty() {
            "(explore from project root)".to_string()
        } else {
            files
                .iter()
                .map(|f| format!("- `{f}`"))
                .collect::<Vec<_>>()
                .join("\n")
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_prompt_contains_session_id() {
        let session = ExternalSession {
            session_id: "ext_20250101_1234".to_string(),
            created: "2025-01-01T00:00:00Z".to_string(),
            runner: "claude".to_string(),
            ttl_hours: 24,
            blind_packet_hash: "abc".to_string(),
            status: "pending".to_string(),
        };

        let packet = ReviewPacket {
            version: 1,
            created: "2025-01-01T00:00:00Z".to_string(),
            batches: vec![],
            score_snapshot: prepare::ScoreSnapshot {
                overall: 0.0,
                objective: 0.0,
                strict: 0.0,
                verified_strict: 0.0,
            },
            next_command: None,
        };

        let prompt = generate_launch_prompt(&session, &packet, Path::new("/repo"));
        assert!(prompt.contains("ext_20250101_1234"));
        assert!(prompt.contains("claude"));
        assert!(prompt.contains("external-submit"));
    }

    #[test]
    fn session_serializes() {
        let session = ExternalSession {
            session_id: "ext_test_0001".to_string(),
            created: "2025-01-01T00:00:00Z".to_string(),
            runner: "claude".to_string(),
            ttl_hours: 24,
            blind_packet_hash: "deadbeef".to_string(),
            status: "pending".to_string(),
        };

        let json = serde_json::to_string_pretty(&session).unwrap();
        let parsed: ExternalSession = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.session_id, "ext_test_0001");
    }
}
