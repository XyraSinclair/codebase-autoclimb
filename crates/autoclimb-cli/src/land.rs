use std::path::{Path, PathBuf};
use std::process::Command;

use autoclimb_state::lane::Lane;
use autoclimb_state::ledger::{
    ChangeStatusEvent, LaneClosed, LaneOutcome, Ledger, Projection, CHANGE_STATUS, LANE_CLOSED,
    VERIFICATION_RECORDED,
};
use autoclimb_types::run::{Change, ChangeStatus, Verification};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[rustfmt::skip]
#[derive(clap::Args)]
pub(crate) struct LandArgs { #[arg(short, long)] path: PathBuf }

struct Ready {
    root: PathBuf,
    change: Change,
    lane: Lane,
    verification: Verification,
}

#[rustfmt::skip]
pub(crate) fn run(args: LandArgs) -> Result<()> {
    let root = git_root(&args.path)?;
    let ledger_path = root.join(".autoclimb/events.jsonl");
    if !ledger_path.exists() { return Err("expected ledger .autoclimb/events.jsonl, actual absent".into()); }
    let mut ledger = Ledger::open(&ledger_path)?;
    let ready = preconditions(root, &ledger, &Projection::replay(&ledger)?)?;
    let result_tree = ready.verification.result_tree.clone();
    let commit = git(&ready.root, &["commit-tree", &result_tree, "-p", "HEAD", "-m", &commit_message(&ready.change, &ready.verification)])?.trim().to_owned();
    git(&ready.root, &["merge", "--ff-only", &commit])?;
    let landed_tree = git(&ready.root, &["rev-parse", "HEAD^{tree}"])?;
    if landed_tree.trim() != result_tree { return Err(format!("fatal landing tree mismatch: expected {result_tree}, actual {}", landed_tree.trim()).into()); }
    let reason = format!("landed as {commit}");
    landed(&mut ledger, &ready.change.id, &reason)?;
    let lane_path = ready.lane.path.display().to_string();
    ready.lane.remove_discarding()?;
    close_lane(&mut ledger, &ready.change.id, &lane_path, &reason)?;
    println!("landing commit: {commit}");
    println!("result tree: {result_tree}");
    println!("push is a separate operator step");
    Ok(())
}

#[rustfmt::skip]
fn preconditions(root: PathBuf, ledger: &Ledger, projection: &Projection) -> Result<Ready> {
    let dirty = git(&root, &["status", "--porcelain"])?;
    if !dirty.trim().is_empty() { return Err(format!("expected clean repository, actual paths:\n{}", dirty.trim()).into()); }
    let head = git(&root, &["rev-parse", "HEAD"])?.trim().to_owned();
    let tree = git(&root, &["rev-parse", "HEAD^{tree}"])?.trim().to_owned();
    let verified = projection.changes.values().filter(|change| change.status == ChangeStatus::Verified).collect::<Vec<_>>();
    let candidates = verified.iter().copied().filter(|change| projection.retained_lanes.contains_key(&change.id) && change.base.head == head && change.base.tree == tree).collect::<Vec<_>>();
    if candidates.len() != 1 { return Err(candidate_error(&verified, &candidates, projection, &head, &tree).into()); }
    let change = candidates[0].clone();
    let retained = &projection.retained_lanes[&change.id];
    let lane_path = PathBuf::from(&retained.lane_path);
    if !lane_path.exists() { return Err(format!("expected retained lane path {}, actual absent", lane_path.display()).into()); }
    let verification = latest_verification(ledger, &change.id)?;
    let lane = Lane::recorded(&root, lane_path, change.base.head.clone(), change.base.tree.clone());
    exact("verified result tree", &verification.result_tree, &lane.result_tree()?)?;
    Ok(Ready { root, change, lane, verification })
}

fn candidate_error(
    verified: &[&Change],
    candidates: &[&Change],
    projection: &Projection,
    head: &str,
    tree: &str,
) -> String {
    let mut lines = vec![
        format!(
            "expected exactly 1 landable Verified change, actual {}",
            candidates.len()
        ),
        format!("Verified changes: {}", verified.len()),
    ];
    if candidates.len() > 1 {
        for change in candidates {
            lines.push(format!("candidate: {}", change.id));
        }
        return lines.join("\n");
    }
    for change in verified {
        let mut reasons = Vec::new();
        if !projection.retained_lanes.contains_key(&change.id) {
            reasons.push("no retained lane".to_owned());
        }
        if change.base.head != head || change.base.tree != tree {
            reasons.push(format!(
                "wrong base head (expected {} tree {}, actual {head} tree {tree})",
                change.base.head, change.base.tree
            ));
        }
        lines.push(format!("{}: {}", change.id, reasons.join("; ")));
    }
    lines.join("\n")
}

fn latest_verification(ledger: &Ledger, change_id: &str) -> Result<Verification> {
    for event in ledger.events().iter().rev() {
        if event.kind == VERIFICATION_RECORDED {
            let verification: Verification = serde_json::from_value(event.payload.clone())?;
            if verification.change == change_id {
                return Ok(verification);
            }
        }
    }
    Err(format!("expected verification_recorded for change {change_id}, actual none").into())
}

fn commit_message(change: &Change, verification: &Verification) -> String {
    let prefix = format!("autoclimb land {}: ", change.id);
    let thesis = change
        .thesis
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let sentence_end = thesis
        .char_indices()
        .find(|(_, character)| matches!(character, '.' | '!' | '?'))
        .map_or(thesis.len(), |(index, character)| {
            index + character.len_utf8()
        });
    let title = format!(
        "{prefix}{}",
        truncate(
            &thesis[..sentence_end],
            72usize.saturating_sub(prefix.chars().count())
        )
    );
    let mut lines = vec![
        title,
        String::new(),
        format!("result tree {}", verification.result_tree),
    ];
    lines.extend(
        verification
            .levels_run
            .iter()
            .map(|level| format!("{:?}: {}", level.verdict, level.command)),
    );
    lines.join("\n")
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    if limit == 0 {
        return String::new();
    }
    let mut truncated = text
        .chars()
        .take(limit.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

#[rustfmt::skip]
fn exact(label: &str, expected: &str, actual: &str) -> Result<()> { if expected == actual { Ok(()) } else { Err(format!("expected {label} {expected}, actual {actual}").into()) } }
#[rustfmt::skip]
fn landed(ledger: &mut Ledger, change: &str, reason: &str) -> Result<()> { ledger.append(CHANGE_STATUS, &ChangeStatusEvent { change_id: change.into(), status: ChangeStatus::Landed, reason: Some(reason.into()) })?; Ok(()) }
#[rustfmt::skip]
fn close_lane(ledger: &mut Ledger, change: &str, lane_path: &str, reason: &str) -> Result<()> { ledger.append(LANE_CLOSED, &LaneClosed { change_id: change.into(), lane_path: lane_path.into(), outcome: LaneOutcome::Removed, reason: reason.into() })?; Ok(()) }
#[rustfmt::skip]
fn git_root(path: &Path) -> Result<PathBuf> { Ok(PathBuf::from(git(path, &["rev-parse", "--show-toplevel"])?.trim())) }
#[rustfmt::skip]
fn git(root: &Path, args: &[&str]) -> Result<String> { let output = Command::new("git").arg("-C").arg(root).args(args).output()?; if !output.status.success() { return Err(format!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim()).into()); } Ok(String::from_utf8(output.stdout)?) }
