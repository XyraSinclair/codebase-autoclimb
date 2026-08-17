use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use autoclimb_review::agent_backend::{AgentBackend, AttemptBudget, Brief, CodexBackend};
use autoclimb_review::external::sha256_hex;
use autoclimb_state::lane::{Lane, LaneViolation};
use autoclimb_state::ledger::{
    AttemptLogged, ChangeStatusEvent, LaneClosed, LaneOpened, LaneOutcome, Ledger, Projection,
    ATTEMPT_LOGGED, CHANGE_STATUS, LANE_CLOSED, LANE_OPENED, VERIFICATION_RECORDED,
};
use autoclimb_types::newtypes::Timestamp;
use autoclimb_types::run::{
    Change, ChangeStatus, DecisionStatus, LevelResult, RuleSet, Verdict, Verification,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[rustfmt::skip]
#[derive(clap::Args)]
pub(crate) struct ClimbArgs {
    #[arg(short, long)] path: PathBuf,
    #[arg(long, default_value = "codex")] backend: String,
    #[arg(long)] dry_run: bool,
    #[arg(long)] discard_stuck: bool,
}
#[rustfmt::skip]
struct Ready { root: PathBuf, change: Change, ruleset: RuleSet }
#[rustfmt::skip]
struct CommandResult { command: String, verdict: Verdict, digest: String, output: String }

#[rustfmt::skip]
pub(crate) fn run(args: ClimbArgs) -> Result<()> {
    let root = git_root(&args.path)?;
    if args.discard_stuck { return discard_stuck(&root); }
    let ledger_path = root.join(".autoclimb/events.jsonl");
    if !ledger_path.exists() { return Err("expected ledger .autoclimb/events.jsonl, actual absent".into()); }
    let mut ledger = Ledger::open(&ledger_path)?;
    let ready = preconditions(root, &Projection::replay(&ledger)?)?;
    if args.dry_run { print_plan(&ready, &args.backend); return Ok(()); }
    if !matches!(args.backend.as_str(), "codex" | "manual") { return Err(format!("expected backend codex or manual, actual {}", args.backend).into()); }

    let lane = Lane::create(&ready.root, &ready.change.base.head, &ready.root.join(".autoclimb/lanes"))?;
    open_lane(&mut ledger, &ready.change.id, &lane)?;
    status(&mut ledger, &ready.change.id, ChangeStatus::Lane)?;
    let brief = implementation_brief(&ready);
    let started_at = Timestamp::now().0;
    let attempt = if args.backend == "manual" {
        println!("manual lane ready: {}", lane.path.display());
        println!("edit only the declared write set, then press Enter here");
        io::stdin().read_line(&mut String::new())?;
        None
    } else {
        Some(CodexBackend::default().run(
            &Brief::new(&brief), &lane.path,
            &AttemptBudget { wall_secs: ready.ruleset.budget.wall_secs, max_attempts: ready.ruleset.budget.attempts },
        ))
    };
    let (exit, transcript_hash) = attempt.as_ref().map_or_else(|| ("success".to_owned(), sha256(b"manual operator edit")), |record| (record.exit.clone(), record.transcript_hash.clone()));
    ledger.append(ATTEMPT_LOGGED, &AttemptLogged { change_id: ready.change.id.clone(), backend: args.backend, brief_hash: sha256(brief.as_bytes()), budget: ready.ruleset.budget.clone(), started_at: started_at.parse()?, ended_at: Timestamp::now().0.parse()?, exit: exit.clone(), transcript_hash, produced_tree: Some(lane.result_tree()?) })?;
    if exit != "success" { return discard_lane(&mut ledger, &ready.change.id, lane, &format!("agent {exit}")); }

    if lane.diff_paths()?.is_empty() { return discard_lane(&mut ledger, &ready.change.id, lane, "agent produced no change"); }
    if let Err(violation) = lane.enforce(&ready.change.write_set, &ready.ruleset.protected_paths) {
        print_violation(&violation);
        return discard_lane(&mut ledger, &ready.change.id, lane, "lane path violation");
    }
    status(&mut ledger, &ready.change.id, ChangeStatus::Verifying)?;
    let commands = verify(&lane.path, &ready.ruleset)?;
    let result_tree = lane.result_tree()?;
    let verification = Verification {
        change: ready.change.id.clone(), base_tree: lane.base_tree.clone(), result_tree: result_tree.clone(),
        patch_hash: lane.patch_sha256()?, ruleset_hash: ready.change.lineage.clone(), levels_run: level_results(&commands),
        behaviour_changed: Vec::new(), behaviour_preserved: Vec::new(),
        residual_uncertainty: vec!["existing checks only; semantic preservation not proven".into()],
        realized: Vec::new(), rollback: "discard lane / git revert of the landing commit".into(),
    };
    ledger.append(VERIFICATION_RECORDED, &verification)?;
    if let Some(failure) = commands.iter().find(|result| result.verdict == Verdict::Failed) {
        status_reason(&mut ledger, &ready.change.id, &format!("verifier failed: {}", failure.command))?;
        retain_lane(&mut ledger, &ready.change.id, &lane, "verification failed; lane retained for inspection")?;
        println!("verification failed: {}", failure.command);
        println!("last 40 lines:\n{}", last_lines(&failure.output, 40));
        println!("lane retained: {}", lane.path.display());
        return Ok(());
    }
    status(&mut ledger, &ready.change.id, ChangeStatus::Verified)?;
    retain_lane(&mut ledger, &ready.change.id, &lane, "verified lane retained for landing")?;
    print_packet(&ready.change, &lane, &commands, &result_tree)
}

#[rustfmt::skip]
fn preconditions(root: PathBuf, projection: &Projection) -> Result<Ready> {
    if let Some(opened) = projection.active_lanes.values().next() {
        return Err(stuck_message(projection, opened).into());
    }
    if let Some(lane) = projection.retained_lanes.values().find(|lane| projection.changes.get(&lane.change_id).is_some_and(|change| change.status == ChangeStatus::Discarded)) {
        return Err(format!("failed lane retained at {}; inspect it or use climb --discard-stuck", lane.lane_path).into());
    }
    let open = projection.decisions.values().filter(|decision| decision.status == DecisionStatus::Open).map(|decision| decision.id.as_str()).collect::<Vec<_>>();
    if !open.is_empty() { return Err(format!("open decisions {:?}: answer the frontier questions or supersede them", open).into()); }
    let (ratified, ratified_hash) = projection.ruleset.as_ref().ok_or("expected ratified RuleSet, actual none")?;
    if ratified.purpose.starts_with("UNRATIFIED") { return Err(format!("expected ratified purpose, actual {:?}", ratified.purpose).into()); }
    if ratified.verifier_commands.is_empty() { return Err("expected non-empty verifier_commands, actual []".into()); }
    if ratified.verifier_commands.len() > ratified.budget.subprocesses as usize {
        return Err(format!("expected verifier count <= subprocess budget {}, actual {}", ratified.budget.subprocesses, ratified.verifier_commands.len()).into());
    }
    let planned = projection.changes.values().filter(|change| change.status == ChangeStatus::Planned).collect::<Vec<_>>();
    if planned.len() != 1 { return Err(format!("expected exactly 1 Planned change, actual {}", planned.len()).into()); }
    let change = planned[0].clone();
    let dirty = git(&root, &["status", "--porcelain"])?;
    if !dirty.trim().is_empty() { return Err(format!("expected clean repository, actual paths:\n{}", dirty.trim()).into()); }
    exact("HEAD", &change.base.head, git(&root, &["rev-parse", "HEAD"])?.trim())?;
    exact("base tree", &change.base.tree, git(&root, &["rev-parse", "HEAD^{tree}"])?.trim())?;
    let current: RuleSet = serde_json::from_str(&fs::read_to_string(root.join(".autoclimb/ruleset.json"))?)?;
    exact("RuleSet hash", &change.lineage, &current.canonical_hash())?;
    exact("latest ratified hash", &change.lineage, ratified_hash)?;
    exact("brief hash", &change.brief_hash, &sha256(change.brief.as_bytes()))?;
    Ok(Ready { root, change, ruleset: current })
}

#[rustfmt::skip]
fn exact(label: &str, expected: &str, actual: &str) -> Result<()> {
    if expected == actual { Ok(()) } else { Err(format!("expected {label} {expected}, actual {actual}").into()) }
}

#[rustfmt::skip]
fn implementation_brief(ready: &Ready) -> String {
    format!("{}\n\nThesis: {}\nHard write boundary: {}\nDo not create or modify tests.\nUse 'evidence' for supporting artifacts.\nYour final message is a report; the working tree is the deliverable. Do not commit.", ready.change.brief, ready.change.thesis, ready.change.write_set.join(", "))
}

#[rustfmt::skip]
fn verify(lane: &Path, ruleset: &RuleSet) -> Result<Vec<CommandResult>> {
    let mut results = Vec::new();
    let mut stopped = false;
    for command in &ruleset.verifier_commands {
        if stopped { results.push(CommandResult { command: command.clone(), verdict: Verdict::Inconclusive, digest: sha256(b"unrun after earlier failure"), output: "unrun after earlier failure".into() }); continue; }
        let result = run_command(lane, command, ruleset.budget.wall_secs).unwrap_or_else(|error| CommandResult { command: command.clone(), verdict: Verdict::Failed, digest: sha256(error.to_string().as_bytes()), output: error.to_string() });
        stopped = result.verdict == Verdict::Failed;
        results.push(result);
    }
    Ok(results)
}

fn run_command(lane: &Path, text: &str, wall_secs: u64) -> Result<CommandResult> {
    let parts = text.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return Ok(CommandResult {
            command: text.into(),
            verdict: Verdict::Failed,
            digest: sha256(b"empty command"),
            output: "empty command".into(),
        });
    }
    let output_path = lane.join(".autoclimb-tmp/verifier-output.txt");
    let output_file = File::create(&output_path)?;
    let mut child = Command::new(parts[0])
        .args(&parts[1..])
        .current_dir(lane)
        .stdout(Stdio::from(output_file.try_clone()?))
        .stderr(Stdio::from(output_file))
        .spawn()?;
    let started = Instant::now();
    let success = loop {
        if let Some(status) = child.try_wait()? {
            break status.success();
        }
        if started.elapsed() >= Duration::from_secs(wall_secs) {
            child.kill()?;
            child.wait()?;
            break false;
        }
        thread::sleep(Duration::from_millis(50));
    };
    let mut bytes = Vec::new();
    File::open(output_path)?.read_to_end(&mut bytes)?;
    Ok(CommandResult {
        command: text.into(),
        verdict: if success {
            Verdict::Passed
        } else {
            Verdict::Failed
        },
        digest: sha256(&bytes),
        output: String::from_utf8_lossy(&bytes).into_owned(),
    })
}

#[rustfmt::skip]
fn discard_lane(ledger: &mut Ledger, change: &str, lane: Lane, reason: &str) -> Result<()> {
    println!("discarded {change}: {reason}");
    status_reason(ledger, change, reason)?;
    let lane_path = lane.path.display().to_string();
    lane.remove_discarding()?;
    close_lane_path(ledger, change, &lane_path, LaneOutcome::Removed, reason)
}

#[rustfmt::skip]
fn open_lane(ledger: &mut Ledger, change: &str, lane: &Lane) -> Result<()> { ledger.append(LANE_OPENED, &LaneOpened { change_id: change.into(), lane_path: lane.path.display().to_string(), base_head: lane.base_head.clone(), base_tree: lane.base_tree.clone() })?; Ok(()) }

#[rustfmt::skip]
fn level_results(results: &[CommandResult]) -> Vec<LevelResult> { results.iter().map(|result| LevelResult { command: result.command.clone(), verdict: result.verdict, output_digest: result.digest.clone() }).collect() }

#[rustfmt::skip]
fn discard_stuck(root: &Path) -> Result<()> {
    let mut ledger = Ledger::open(root.join(".autoclimb/events.jsonl"))?;
    let projection = Projection::replay(&ledger)?;
    let (change_id, lane_path) = if let Some(opened) = projection.active_lanes.values().next() {
        (opened.change_id.clone(), PathBuf::from(&opened.lane_path))
    } else {
        let retained = projection.retained_lanes.values().find(|lane| projection.changes.get(&lane.change_id).is_some_and(|change| change.status == ChangeStatus::Discarded)).ok_or("no stuck lane recorded")?;
        (retained.change_id.clone(), PathBuf::from(&retained.lane_path))
    };
    let change = projection.changes.get(&change_id).ok_or("lane names unknown change")?;
    if change.status == ChangeStatus::Verified { return Err("refusing to discard a Verified lane".into()); }
    let lane = Lane::recorded(root, lane_path.clone(), change.base.head.clone(), change.base.tree.clone());
    println!("diff stat:\n{}", lane.patch_stat()?);
    if change.status != ChangeStatus::Discarded { status_reason(&mut ledger, &change_id, "operator discarded stuck lane")?; }
    lane.remove_discarding()?;
    close_lane_path(&mut ledger, &change_id, &lane_path.display().to_string(), LaneOutcome::Removed, "operator discarded stuck lane")?;
    println!("discarded {} and removed {}", change_id, lane_path.display());
    Ok(())
}

#[rustfmt::skip]
fn status(ledger: &mut Ledger, change: &str, status: ChangeStatus) -> Result<()> {
    ledger.append(CHANGE_STATUS, &ChangeStatusEvent { change_id: change.into(), status, reason: None })?;
    Ok(())
}
#[rustfmt::skip]
fn status_reason(ledger: &mut Ledger, change: &str, reason: &str) -> Result<()> { ledger.append(CHANGE_STATUS, &ChangeStatusEvent { change_id: change.into(), status: ChangeStatus::Discarded, reason: Some(reason.into()) })?; Ok(()) }

#[rustfmt::skip]
fn retain_lane(ledger: &mut Ledger, change: &str, lane: &Lane, reason: &str) -> Result<()> { close_lane_path(ledger, change, &lane.path.display().to_string(), LaneOutcome::Retained, reason) }
#[rustfmt::skip]
fn close_lane_path(ledger: &mut Ledger, change: &str, lane_path: &str, outcome: LaneOutcome, reason: &str) -> Result<()> { ledger.append(LANE_CLOSED, &LaneClosed { change_id: change.into(), lane_path: lane_path.into(), outcome, reason: reason.into() })?; Ok(()) }

#[rustfmt::skip]
fn print_violation(violation: &LaneViolation) {
    match violation {
        LaneViolation::Paths { outside, protected } => { for path in outside { println!("outside write set: {path}"); } for path in protected { println!("protected path: {path}"); } }
        LaneViolation::Invalid(message) => println!("path enforcement failed: {message}"),
    }
}

#[rustfmt::skip]
fn print_plan(ready: &Ready, backend: &str) {
    println!("would run change {} with backend {}", ready.change.id, backend);
    println!("thesis: {}", ready.change.thesis);
    println!("write set: {}", ready.change.write_set.join(", "));
    for command in &ready.ruleset.verifier_commands { println!("verify: {command}"); }
}

#[rustfmt::skip]
fn print_packet(change: &Change, lane: &Lane, commands: &[CommandResult], result_tree: &str) -> Result<()> {
    println!("thesis: {}", change.thesis);
    println!("diff --stat:\n{}", lane.patch_stat()?);
    for result in commands { println!("level: {} => {:?} sha256:{}", result.command, result.verdict, result.digest); }
    println!("result tree: {result_tree}");
    println!("lane path: {}", lane.path.display());
    println!("next step: git -C {} diff {} {}", lane.path.display(), lane.base_tree, result_tree);
    Ok(())
}

#[rustfmt::skip]
fn stuck_message(projection: &Projection, opened: &LaneOpened) -> String { let change = &projection.changes[&opened.change_id]; let attempt = projection.attempts.get(&opened.change_id).and_then(|attempts| attempts.last()).map(|attempt| attempt.exit.as_str()).unwrap_or("no attempt logged"); format!("change {} is stuck in {:?}; lane {}; latest attempt {}; use climb --discard-stuck", change.id, change.status, opened.lane_path, attempt) }
#[rustfmt::skip]
fn last_lines(text: &str, count: usize) -> String { text.lines().rev().take(count).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n") }
#[rustfmt::skip]
fn git_root(path: &Path) -> Result<PathBuf> { Ok(PathBuf::from(git(path, &["rev-parse", "--show-toplevel"])?.trim())) }
#[rustfmt::skip]
fn git(root: &Path, args: &[&str]) -> Result<String> { let output = Command::new("git").arg("-C").arg(root).args(args).output()?; if !output.status.success() { return Err(format!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim()).into()); } Ok(String::from_utf8(output.stdout)?) }
fn sha256(bytes: &[u8]) -> String {
    sha256_hex(&String::from_utf8_lossy(bytes))
}
