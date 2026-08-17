use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use autoclimb_state::ledger::{
    Ledger, Projection, DECISION_ANSWERED, RULESET_RATIFIED, SNAPSHOT_TAKEN,
};
use autoclimb_types::run::{Budget, RiskClass, RuleSet, Snapshot};
use serde_json::Value;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const UNRATIFIED_PURPOSE: &str = "UNRATIFIED: describe what this repository is for";

#[derive(clap::Args)]
pub(crate) struct ConstituteArgs {
    /// Path to constitute
    #[arg(short, long)]
    path: PathBuf,
}

pub(crate) fn run(args: ConstituteArgs) -> Result<()> {
    let root = git_root(&args.path)?;
    let ledger_path = root.join(".autoclimb/events.jsonl");
    if !ledger_path.exists() {
        return Err("run explore first: .autoclimb/events.jsonl is absent".into());
    }
    let mut ledger = Ledger::open(&ledger_path)?;
    let projection = Projection::replay(&ledger)?;
    let snapshot = latest_snapshot(&ledger)?;
    let files = snapshot_files(&root, &snapshot.tree)?;

    let last_ratification = ledger
        .events()
        .iter()
        .rposition(|event| event.kind == RULESET_RATIFIED);
    let prior_hash = projection.ruleset.as_ref().map(|(_, hash)| hash.to_owned());
    let mut ruleset = match projection.ruleset {
        Some((ruleset, _)) => ruleset,
        None => conservative_ruleset(&files),
    };
    apply_answered_decisions(&mut ruleset, &ledger, last_ratification)?;
    validate_authority(&ruleset)?;

    ruleset.hash = ruleset.canonical_hash();
    let unchanged = prior_hash.as_ref() == Some(&ruleset.hash);
    if !unchanged {
        ledger.append(RULESET_RATIFIED, &ruleset)?;
    }

    let authority_dir = root.join(".autoclimb");
    fs::create_dir_all(&authority_dir)?;
    write_atomic(
        &authority_dir.join("ruleset.json"),
        format!("{}\n", serde_json::to_string_pretty(&ruleset)?).as_bytes(),
    )?;
    write_atomic(
        &authority_dir.join("RULESET.md"),
        render_markdown(&ruleset).as_bytes(),
    )?;
    write_atomic(
        &authority_dir.join(".gitignore"),
        b"*\n!ruleset.json\n!RULESET.md\n!.gitignore\n",
    )?;
    make_trackable(&root)?;

    println!("ruleset sha256:{}", ruleset.hash);
    println!("path {}", authority_dir.join("ruleset.json").display());
    println!("risk ceiling {:?}", ruleset.risk_ceiling);
    if ruleset.verifier_commands.is_empty() {
        println!("verifier commands: none (draft-only)");
    } else {
        println!("verifier commands:");
        for command in &ruleset.verifier_commands {
            println!("- {command}");
        }
    }
    println!(
        "purpose UNRATIFIED: {}",
        ruleset.purpose.starts_with("UNRATIFIED:")
    );
    if unchanged {
        println!("unchanged");
    }
    Ok(())
}

fn conservative_ruleset(files: &[String]) -> RuleSet {
    let cargo = files.iter().any(|path| path == "Cargo.toml");
    let mut protected = BTreeSet::from([
        "**/.github/**".to_owned(),
        "**/Cargo.toml".to_owned(),
        "**/Cargo.lock".to_owned(),
        "**/tests/**".to_owned(),
        "**/*_test.*".to_owned(),
        ".autoclimb/**".to_owned(),
    ]);
    protected.extend(files.iter().filter(|path| is_ci_or_config(path)).cloned());
    RuleSet {
        purpose: UNRATIFIED_PURPOSE.into(),
        non_goals: Vec::new(),
        allowed_paths: vec!["**".into()],
        protected_paths: protected.into_iter().collect(),
        compatibility: Default::default(),
        verifier_commands: if cargo {
            vec![
                "cargo build --workspace".into(),
                "cargo test --workspace".into(),
                "cargo clippy --workspace --all-targets -- -D warnings".into(),
                "cargo fmt --all -- --check".into(),
            ]
        } else {
            Vec::new()
        },
        risk_ceiling: if cargo { RiskClass::R2 } else { RiskClass::R0 },
        budget: Budget {
            attempts: 2,
            wall_secs: 1800,
            subprocesses: 4,
        },
        hash: String::new(),
    }
}

fn is_ci_or_config(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or_default();
    lower.split('/').any(|part| {
        matches!(
            part,
            ".github" | ".gitlab" | ".circleci" | "ci" | "workflows"
        )
    }) || name == "config"
        || name.starts_with("config.")
        || name.ends_with(".config")
        || name.contains(".config.")
        || name.starts_with("ci.")
        || name.contains("-ci.")
        || name.contains("_ci.")
}

fn apply_answered_decisions(
    ruleset: &mut RuleSet,
    ledger: &Ledger,
    last_ratification: Option<usize>,
) -> Result<()> {
    let mut document = serde_json::to_value(&*ruleset)?;
    for event in ledger
        .events()
        .iter()
        .skip(last_ratification.map_or(0, |index| index + 1))
        .filter(|event| event.kind == DECISION_ANSWERED)
    {
        let decision = event
            .payload
            .get("decision_id")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>");
        let patch = event.payload.get("patch").ok_or_else(|| {
            format!("decision {decision} has no compiled JSON Merge Patch in field `patch`")
        })?;
        validate_patch(patch)?;
        merge_patch(&mut document, patch);
    }
    *ruleset = serde_json::from_value(document)?;
    Ok(())
}

fn validate_patch(patch: &Value) -> Result<()> {
    let object = patch
        .as_object()
        .ok_or("RuleSet JSON Merge Patch must be an object")?;
    const FIELDS: &[&str] = &[
        "purpose",
        "non_goals",
        "allowed_paths",
        "protected_paths",
        "compatibility",
        "verifier_commands",
        "risk_ceiling",
        "budget",
        "hash",
    ];
    for (field, value) in object {
        if !FIELDS.contains(&field.as_str()) {
            return Err(format!("unknown RuleSet field `{field}`").into());
        }
        if field == "budget" {
            if let Some(budget) = value.as_object() {
                for key in budget.keys() {
                    if !matches!(key.as_str(), "attempts" | "wall_secs" | "subprocesses") {
                        return Err(format!("unknown RuleSet field `budget.{key}`").into());
                    }
                }
            }
        }
    }
    Ok(())
}

fn merge_patch(document: &mut Value, patch: &Value) {
    let Some(patch) = patch.as_object() else {
        *document = patch.clone();
        return;
    };
    if !document.is_object() {
        *document = Value::Object(Default::default());
    }
    let target = document
        .as_object_mut()
        .expect("document was made an object");
    for (field, value) in patch {
        if value.is_null() {
            target.remove(field);
        } else {
            merge_patch(target.entry(field).or_insert(Value::Null), value);
        }
    }
}

fn validate_authority(ruleset: &RuleSet) -> Result<()> {
    if ruleset.verifier_commands.is_empty() && ruleset.risk_ceiling > RiskClass::R0 {
        return Err("a RuleSet without verifier commands cannot authorize changes above R0".into());
    }
    Ok(())
}

fn latest_snapshot(ledger: &Ledger) -> Result<Snapshot> {
    ledger
        .events()
        .iter()
        .rev()
        .find(|event| event.kind == SNAPSHOT_TAKEN)
        .ok_or_else(|| "run explore first: ledger has no snapshot".into())
        .and_then(|event| Ok(serde_json::from_value(event.payload.clone())?))
}

fn snapshot_files(root: &Path, tree: &str) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["ls-tree", "-r", "--name-only", tree])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "could not enumerate snapshot tree {tree}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?
        .lines()
        .map(str::to_owned)
        .collect())
}

fn git_root(path: &Path) -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(path)
        .output()?;
    if !output.status.success() {
        return Err(format!("{} is not a git repository", path.display()).into());
    }
    Ok(PathBuf::from(String::from_utf8(output.stdout)?.trim()))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn make_trackable(root: &Path) -> Result<()> {
    let path = root.join(".gitignore");
    let mut contents = fs::read_to_string(&path).unwrap_or_default();
    if !contents.lines().any(|line| line == "!/.autoclimb/") {
        if !contents.is_empty() && !contents.ends_with('\n') {
            contents.push('\n');
        }
        contents.push_str("!/.autoclimb/\n");
        write_atomic(&path, contents.as_bytes())?;
    }
    Ok(())
}

fn render_markdown(ruleset: &RuleSet) -> String {
    let draft = if ruleset.verifier_commands.is_empty() {
        "\n**Draft-only:** no verifier commands are configured, so this RuleSet cannot authorize changes above R0.\n"
    } else {
        ""
    };
    format!(
        "<!-- generated from ruleset.json; hash: sha256:{} -->\n<!-- NEVER HAND-EDIT: regenerate with `autoclimb constitute`. -->\n\n# Repository RuleSet\n\n`#[cfg(test)]` modules inside `src` files cannot be path-protected; verifier commands remain the enforcement surface for them.\n{}\n```json\n{}\n```\n",
        ruleset.hash,
        draft,
        serde_json::to_string_pretty(ruleset).expect("RuleSet serialization cannot fail")
    )
}
