use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use autoclimb_config::load_or_default;
use autoclimb_discovery::walk::{find_source_files, matches_exclusion, DiscoveryConfig};
use autoclimb_state::ledger::{Ledger, FACT_RECORDED, SNAPSHOT_TAKEN};
use autoclimb_types::run::{Coverage, Fact, FactKind, Snapshot};
use serde_json::json;

use crate::scan_engine::{
    collect_scan, known_source_extensions, language_exclusions, ScanConfig, ScanOutput,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(clap::Args)]
pub(crate) struct ExploreArgs {
    /// Path to explore (default: current directory)
    #[arg(short, long)]
    path: Option<PathBuf>,
}

pub(crate) fn run(args: ExploreArgs) -> Result<()> {
    let root = git_root(&args.path.unwrap_or(std::env::current_dir()?))?;
    let config = load_or_default(&root.join(".autoclimb/config.json"));
    let scan = collect_scan(&root, None, &ScanConfig::new(&config.exclude, false))?;
    let discovered = find_source_files(
        &root,
        &DiscoveryConfig {
            exclude_patterns: language_exclusions(&scan.lang),
            ..Default::default()
        },
    );
    let excluded = config
        .exclude
        .iter()
        .map(|pattern| {
            let count = discovered
                .iter()
                .filter(|file| matches_exclusion(file, pattern))
                .count();
            (pattern.clone(), count)
        })
        .collect::<Vec<_>>();
    let coverage = Coverage {
        seen: scan.files.len() as u64,
        total: discovered.len() as u64,
        excluded: excluded
            .iter()
            .map(|(pattern, count)| (pattern.clone(), format!("{count} files")))
            .collect(),
    };
    let snapshot = take_snapshot(&root, coverage.clone())?;
    let unknown = unscanned_extensions(&discovered, &scan.files, &config.exclude);
    let ledger_path = root.join(".autoclimb/events.jsonl");
    let existed = ledger_path.exists();
    let mut ledger = Ledger::open(&ledger_path).map_err(|error| {
        if existed {
            format!(
                "could not verify {}: {error}; move the corrupt ledger aside and retry",
                ledger_path.display()
            )
        } else {
            format!("could not create {}: {error}", ledger_path.display())
        }
    })?;
    let snapshot_id = ledger
        .append(SNAPSHOT_TAKEN, &snapshot)?
        .payload_hash
        .clone();
    let facts = make_facts(&snapshot_id, &scan, &coverage, &unknown);
    for fact in &facts {
        ledger.append(FACT_RECORDED, fact)?;
    }

    let report_path = root.join(".autoclimb/EXPLORATION.md");
    fs::write(
        &report_path,
        render(&snapshot, &snapshot_id, &scan, &excluded, &unknown),
    )?;
    println!("snapshot {}", &snapshot_id[..12]);
    println!("{} facts recorded", facts.len());
    println!("{}", report_path.display());
    Ok(())
}

fn git_root(path: &Path) -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(path)
        .output()
        .map_err(|error| format!("could not run git in {}: {error}", path.display()))?;
    if !output.status.success() {
        return Err(format!("{} is not a git repository", path.display()).into());
    }
    Ok(PathBuf::from(String::from_utf8(output.stdout)?.trim()))
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    Err(format!(
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    )
    .into())
}

fn git_text(root: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(git(root, args)?)?.trim().to_owned())
}

fn take_snapshot(root: &Path, file_universe: Coverage) -> Result<Snapshot> {
    let status = git(root, &["status", "--porcelain=v2", "-z"])?;
    let dirty_digest = if status.is_empty() {
        "clean".into()
    } else {
        sha256(&status)?
    };
    let rustc = command_text("rustc", &["--version"])?;
    let taken_at = command_text("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"])?;
    Ok(serde_json::from_value(json!({
        "repo_id": git_text(root, &["rev-list", "--max-parents=0", "HEAD"] )?,
        "head": git_text(root, &["rev-parse", "HEAD"] )?,
        "tree": git_text(root, &["rev-parse", "HEAD^{tree}"] )?,
        "dirty_digest": dirty_digest, "file_universe": file_universe,
        "tool_versions": {"autoclimb": env!("CARGO_PKG_VERSION"), "rustc": rustc},
        "taken_at": taken_at,
    }))?)
}

fn command_text(command: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(command).args(args).output()?;
    if !output.status.success() {
        return Err(format!("{command} {} failed", args.join(" ")).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn sha256(bytes: &[u8]) -> Result<String> {
    for (command, args) in [("sha256sum", &[][..]), ("shasum", &["-a", "256"][..])] {
        let Ok(mut child) = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
        else {
            continue;
        };
        child.stdin.take().expect("piped stdin").write_all(bytes)?;
        let output = child.wait_with_output()?;
        if output.status.success() {
            return output
                .stdout
                .split(|byte| byte.is_ascii_whitespace())
                .next()
                .ok_or_else(|| "sha256 command returned no digest".into())
                .and_then(|digest| Ok(String::from_utf8(digest.to_vec())?));
        }
    }
    Err("neither sha256sum nor shasum is available".into())
}

fn unscanned_extensions(
    discovered: &[String],
    scanned: &[String],
    exclude: &[String],
) -> BTreeMap<String, usize> {
    let known = known_source_extensions();
    let scanned = scanned.iter().collect::<BTreeSet<_>>();
    let mut counts = BTreeMap::new();
    for file in discovered {
        if scanned.contains(file)
            || exclude
                .iter()
                .any(|pattern| matches_exclusion(file, pattern))
        {
            continue;
        }
        let Some(extension) = Path::new(file).extension().and_then(|value| value.to_str()) else {
            continue;
        };
        if known.contains(extension) {
            *counts.entry(format!(".{extension}")).or_insert(0) += 1;
        }
    }
    counts
}

fn make_facts(
    snapshot: &str,
    scan: &ScanOutput,
    coverage: &Coverage,
    unknown: &BTreeMap<String, usize>,
) -> Vec<Fact> {
    let mut facts = scan.findings.iter().map(|finding| Fact {
        id: String::new(), snapshot: snapshot.into(), subject: finding.file.clone(),
        predicate: finding.detector.clone(),
        value: json!({"summary": finding.summary, "tier": finding.tier.as_u8(), "confidence": finding.confidence.to_string()}),
        kind: FactKind::Observation, evidence: vec![finding.id.clone()], denominator: None,
    }).collect::<Vec<_>>();
    let mut counts = BTreeMap::new();
    for finding in &scan.findings {
        *counts.entry(finding.detector.clone()).or_insert(0usize) += 1;
    }
    facts.push(repo_fact(
        snapshot,
        "file_universe",
        json!({"seen": coverage.seen, "total": coverage.total, "excluded": coverage.excluded}),
        FactKind::Observation,
        coverage,
    ));
    facts.push(repo_fact(
        snapshot,
        "language_selection",
        json!({
            "language": scan.lang,
            "method": "first-marker resolver (python first, then built-in language order)",
            "unscanned_source_extensions": unknown,
        }),
        FactKind::Unknown,
        coverage,
    ));
    for (detector, count) in counts {
        facts.push(repo_fact(
            snapshot,
            "detector_finding_count",
            json!({"detector": detector, "count": count}),
            FactKind::Observation,
            coverage,
        ));
    }
    facts.push(repo_fact(
        snapshot,
        "detector_visibility_gap",
        json!({
            "detector": "duplicates", "count": 0,
            "why": "requires function bodies; no language plugin supplies them",
        }),
        FactKind::Unknown,
        coverage,
    ));
    for (index, fact) in facts.iter_mut().enumerate() {
        fact.id = format!("{snapshot}:fact:{}", index + 1);
    }
    facts
}

fn repo_fact(
    snapshot: &str,
    predicate: &str,
    value: serde_json::Value,
    kind: FactKind,
    coverage: &Coverage,
) -> Fact {
    Fact {
        id: String::new(),
        snapshot: snapshot.into(),
        subject: "repository".into(),
        predicate: predicate.into(),
        value,
        kind,
        evidence: Vec::new(),
        denominator: Some(coverage.clone()),
    }
}

fn render(
    snapshot: &Snapshot,
    id: &str,
    scan: &ScanOutput,
    excluded: &[(String, usize)],
    unknown: &BTreeMap<String, usize>,
) -> String {
    let dirty = if snapshot.dirty_digest == "clean" {
        "clean".into()
    } else {
        format!("dirty (sha256 {})", snapshot.dirty_digest)
    };
    let mut out = format!(
        "# Exploration\n\n- Snapshot: `{}`\n- HEAD: `{}`\n- Tree: `{}`\n- Worktree: {}\n- Taken: `{}`\n",
        &id[..12], &snapshot.head[..8], snapshot.tree, dirty, snapshot.taken_at.to_rfc3339()
    );
    for (tool, version) in &snapshot.tool_versions {
        writeln!(out, "- {tool}: `{version}`").unwrap();
    }
    write!(out, "\n## File Universe\n\n- Seen: {} files\n- Total before exclusions: {} files\n- Excluded by configured patterns: {} files\n",
        snapshot.file_universe.seen, snapshot.file_universe.total,
        excluded.iter().map(|(_, count)| count).sum::<usize>()).unwrap();
    for (pattern, count) in excluded {
        writeln!(out, "  - `{pattern}`: {count} files").unwrap();
    }

    let mut detectors = BTreeMap::new();
    let mut files = BTreeMap::new();
    for finding in &scan.findings {
        *detectors.entry(&finding.detector).or_insert(0usize) += 1;
        *files.entry(&finding.file).or_insert(0usize) += 1;
    }
    out.push_str("\n## What We Can See\n\n### Findings by Detector\n\n");
    if detectors.is_empty() {
        out.push_str("- No findings emitted.\n");
    }
    for (detector, count) in detectors {
        writeln!(out, "- `{detector}`: {count}").unwrap();
    }
    out.push_str("\n### Top Files by Finding Count\n\n");
    let mut files = files.into_iter().collect::<Vec<_>>();
    files.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
    if files.is_empty() {
        out.push_str("- No files had findings.\n");
    }
    for (file, count) in files.into_iter().take(10) {
        writeln!(out, "- `{file}`: {count}").unwrap();
    }

    writeln!(
        out,
        "\n## What We Cannot See\n\n- Language: `{}` was chosen by the first-marker resolver.",
        scan.lang
    )
    .unwrap();
    if unknown.is_empty() {
        out.push_str("  - No other recognized source extensions were present outside the scan.\n");
    }
    for (extension, count) in unknown {
        writeln!(
            out,
            "  - `{extension}`: {count} files present but unscanned"
        )
        .unwrap();
    }
    out.push_str("- Detector: `duplicates` emitted nothing because it requires function bodies, and no language plugin supplies them.\n\n");
    let (seen, total) = (snapshot.file_universe.seen, snapshot.file_universe.total);
    let percent = if total == 0 {
        0.0
    } else {
        seen as f64 * 100.0 / total as f64
    };
    writeln!(out, "**Denominator honesty:** {seen} / {total} discovered repository files were analyzed ({percent:.1}%).").unwrap();
    out
}
