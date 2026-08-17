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
    ChangeStatusEvent, Ledger, Projection, ATTEMPT_LOGGED, CHANGE_STATUS, VERIFICATION_RECORDED,
};
use autoclimb_types::newtypes::Timestamp;
use autoclimb_types::run::{
    Change, ChangeStatus, DecisionStatus, LevelResult, RuleSet, Verdict, Verification, VerifyLevel,
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
struct Ready { root: PathBuf, change: Change, ruleset: RuleSet, brief: String }
#[rustfmt::skip]
struct CommandResult { command: String, verdict: Verdict, digest: String, output: String }

pub(crate) fn run(args: ClimbArgs) -> Result<()> {
    let root = git_root(&args.path)?;
    if args.discard_stuck {
        return discard_stuck(&root);
    }
    let ledger_path = root.join(".autoclimb/events.jsonl");
    if !ledger_path.exists() {
        return Err("expected ledger .autoclimb/events.jsonl, actual absent".into());
    }
    let mut ledger = Ledger::open(&ledger_path)?;
    let ready = preconditions(root, &Projection::replay(&ledger)?, &ledger)?;
    if args.dry_run {
        print_plan(&ready, &args.backend);
        return Ok(());
    }
    if !matches!(args.backend.as_str(), "codex" | "manual") {
        return Err(format!("expected backend codex or manual, actual {}", args.backend).into());
    }

    let lane = Lane::create(
        &ready.root,
        &ready.change.base.head,
        &ready.root.join(".autoclimb/lanes"),
    )?;
    status(&mut ledger, &ready.change.id, ChangeStatus::Lane)?;
    write_active(&ready.root, &ready.change.id, &lane.path)?;
    let brief = implementation_brief(&ready);
    let started_at = Timestamp::now().0;
    let attempt = if args.backend == "manual" {
        println!("manual lane ready: {}", lane.path.display());
        println!("edit only the declared write set, then press Enter here");
        io::stdin().read_line(&mut String::new())?;
        None
    } else {
        Some(CodexBackend::default().run(
            &Brief::new(&brief),
            &lane.path,
            &AttemptBudget {
                wall_secs: ready.ruleset.budget.wall_secs,
                max_attempts: ready.ruleset.budget.attempts,
            },
        ))
    };
    let (exit, transcript_hash) = attempt.as_ref().map_or_else(
        || ("success".to_owned(), sha256(b"manual operator edit")),
        |record| (record.exit.clone(), record.transcript_hash.clone()),
    );
    ledger.append(ATTEMPT_LOGGED, &serde_json::json!({
        "backend": args.backend, "brief_hash": sha256(brief.as_bytes()), "budget": ready.ruleset.budget,
        "started_at": started_at, "ended_at": Timestamp::now().0, "exit": exit.clone(),
        "transcript_hash": transcript_hash, "produced_tree": lane.result_tree().ok(),
    }))?;
    if exit != "success" {
        discard_lane(
            &ready.root,
            &mut ledger,
            &ready.change.id,
            lane,
            &format!("agent {exit}"),
        )?;
        return Ok(());
    }

    if lane.diff_paths()?.is_empty() {
        discard_lane(
            &ready.root,
            &mut ledger,
            &ready.change.id,
            lane,
            "agent produced no change",
        )?;
        return Ok(());
    }
    if let Err(violation) = lane.enforce(&ready.change.write_set, &ready.ruleset.protected_paths) {
        print_violation(&violation);
        discard_lane(
            &ready.root,
            &mut ledger,
            &ready.change.id,
            lane,
            "lane path violation",
        )?;
        return Ok(());
    }
    status(&mut ledger, &ready.change.id, ChangeStatus::Verifying)?;
    let commands = verify(&lane.path, &ready.ruleset)?;
    let result_tree = lane.result_tree()?;
    let verification = Verification {
        change: ready.change.id.clone(),
        base_tree: lane.base_tree.clone(),
        result_tree: result_tree.clone(),
        patch_hash: lane.patch_sha256()?,
        ruleset_hash: ready.change.lineage.clone(),
        levels_run: levels(&commands),
        behaviour_changed: Vec::new(),
        behaviour_preserved: commands
            .iter()
            .map(|r| format!("verifier command: {}", r.command))
            .collect(),
        residual_uncertainty: vec!["existing checks only; semantic preservation not proven".into()],
        realized: Vec::new(),
        rollback: "discard lane / git revert of the landing commit".into(),
    };
    ledger.append(VERIFICATION_RECORDED, &verification)?;
    if let Some(failure) = commands
        .iter()
        .find(|result| result.verdict == Verdict::Failed)
    {
        status_reason(
            &mut ledger,
            &ready.change.id,
            &format!("verifier failed: {}", failure.command),
        )?;
        println!("verification failed: {}", failure.command);
        println!("last 40 lines:\n{}", last_lines(&failure.output, 40));
        println!("lane retained: {}", lane.path.display());
        return Ok(());
    }
    status(&mut ledger, &ready.change.id, ChangeStatus::Verified)?;
    print_packet(&ready.change, &lane, &commands, &result_tree)
}

#[rustfmt::skip]
fn preconditions(root: PathBuf, projection: &Projection, ledger: &Ledger) -> Result<Ready> {
    if let Some(change) = projection.changes.values().find(|change| matches!(change.status, ChangeStatus::Lane | ChangeStatus::Verifying)) {
        return Err(stuck_message(change, &root, ledger).into());
    }
    if let Ok((id, path)) = read_active(&root) { if projection.changes.get(&id).is_some_and(|change| change.status == ChangeStatus::Discarded) { return Err(format!("failed lane retained at {}; inspect it or use climb --discard-stuck", path.display()).into()); } }
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
    let brief_path = root.join(".autoclimb/change-briefs").join(format!("{}.txt", change.id));
    let brief = fs::read_to_string(&brief_path).map_err(|_| format!("expected implementer brief {}, actual absent", brief_path.display()))?;
    exact("brief hash", &change.brief_hash, &sha256(brief.as_bytes()))?;
    Ok(Ready { root, change, ruleset: current, brief })
}

#[rustfmt::skip]
fn exact(label: &str, expected: &str, actual: &str) -> Result<()> {
    if expected == actual { Ok(()) } else { Err(format!("expected {label} {expected}, actual {actual}").into()) }
}

#[rustfmt::skip]
fn implementation_brief(ready: &Ready) -> String {
    format!("{}\n\nThesis: {}\nHard write boundary: {}\nDo not create or modify tests.\nNever use the word spelled r-e-c-e-i-p-t; write 'evidence'.\nYour final message is a report; the working tree is the deliverable. Do not commit.", ready.brief, ready.change.thesis, ready.change.write_set.join(", "))
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
fn levels(results: &[CommandResult]) -> Vec<LevelResult> {
    results.iter().enumerate().map(|(index, result)| (match index { 0 => VerifyLevel::L0, 1 => VerifyLevel::L1, 2 => VerifyLevel::L2, 3 => VerifyLevel::L3, 4 => VerifyLevel::L4, _ => VerifyLevel::L5Plus }, result.verdict, result.digest.clone())).collect()
}

#[rustfmt::skip]
fn discard_lane(root: &Path, ledger: &mut Ledger, change: &str, lane: Lane, reason: &str) -> Result<()> {
    println!("discarded {change}: {reason}");
    status(ledger, change, ChangeStatus::Verifying)?;
    status_reason(ledger, change, reason)?;
    lane.remove_discarding()?;
    clear_active(root)
}

#[rustfmt::skip]
fn discard_stuck(root: &Path) -> Result<()> {
    let (change_id, lane_path) = read_active(root)?;
    let mut ledger = Ledger::open(root.join(".autoclimb/events.jsonl"))?;
    let projection = Projection::replay(&ledger)?;
    let change = projection.changes.get(&change_id).ok_or("active lane names unknown change")?;
    if change.status == ChangeStatus::Verified { return Err("refusing to discard a Verified lane".into()); }
    println!("diff stat:\n{}", git(&lane_path, &["diff", "--stat", &change.base.head])?);
    if change.status == ChangeStatus::Lane { status(&mut ledger, &change_id, ChangeStatus::Verifying)?; status_reason(&mut ledger, &change_id, "operator discarded stuck lane")?; }
    else if change.status == ChangeStatus::Verifying { status_reason(&mut ledger, &change_id, "operator discarded stuck lane")?; }
    git(root, &["worktree", "remove", "--force", &lane_path.to_string_lossy()])?;
    git(root, &["worktree", "prune"])?;
    clear_active(root)?;
    println!("discarded {} and removed {}", change_id, lane_path.display());
    Ok(())
}

#[rustfmt::skip]
fn status(ledger: &mut Ledger, change: &str, status: ChangeStatus) -> Result<()> {
    ledger.append(CHANGE_STATUS, &ChangeStatusEvent { change_id: change.into(), status })?;
    Ok(())
}
#[rustfmt::skip]
fn status_reason(ledger: &mut Ledger, change: &str, reason: &str) -> Result<()> { ledger.append(CHANGE_STATUS, &serde_json::json!({"change_id": change, "status": "discarded", "reason": reason}))?; Ok(()) }

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
    println!("diff --stat:\n{}", git(&lane.path, &["diff", "--stat", &change.base.head])?);
    for result in commands { println!("level: {} => {:?} sha256:{}", result.command, result.verdict, result.digest); }
    println!("result tree: {result_tree}");
    println!("lane path: {}", lane.path.display());
    println!("next step: git -C {} diff", lane.path.display());
    Ok(())
}

#[rustfmt::skip]
fn write_active(root: &Path, change: &str, lane: &Path) -> Result<()> { fs::write(root.join(".autoclimb/active-lane"), format!("{change}\n{}\n", lane.display()))?; Ok(()) }
#[rustfmt::skip]
fn read_active(root: &Path) -> Result<(String, PathBuf)> { let text = fs::read_to_string(root.join(".autoclimb/active-lane"))?; let mut lines = text.lines(); Ok((lines.next().ok_or("active lane missing change id")?.into(), PathBuf::from(lines.next().ok_or("active lane missing path")?))) }
#[rustfmt::skip]
fn clear_active(root: &Path) -> Result<()> { let path = root.join(".autoclimb/active-lane"); if path.exists() { fs::remove_file(path)?; } Ok(()) }
#[rustfmt::skip]
fn stuck_message(change: &Change, root: &Path, ledger: &Ledger) -> String { let lane = read_active(root).ok().map(|(_, path)| path.display().to_string()).unwrap_or_else(|| "unknown (lane identity was not recorded)".into()); let attempt = ledger.events().iter().rev().find(|event| event.kind == ATTEMPT_LOGGED).and_then(|event| event.payload["exit"].as_str()).unwrap_or("no attempt logged"); format!("change {} is stuck in {:?}; lane {}; latest attempt {}; use climb --discard-stuck", change.id, change.status, lane, attempt) }
#[rustfmt::skip]
fn last_lines(text: &str, count: usize) -> String { text.lines().rev().take(count).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n") }
#[rustfmt::skip]
fn git_root(path: &Path) -> Result<PathBuf> { Ok(PathBuf::from(git(path, &["rev-parse", "--show-toplevel"])?.trim())) }
#[rustfmt::skip]
fn git(root: &Path, args: &[&str]) -> Result<String> { let output = Command::new("git").arg("-C").arg(root).args(args).output()?; if !output.status.success() { return Err(format!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim()).into()); } Ok(String::from_utf8(output.stdout)?) }
fn sha256(bytes: &[u8]) -> String {
    sha256_hex(&String::from_utf8_lossy(bytes))
}
