use std::collections::BTreeMap;
use std::env;

use autoclimb_state::ledger::{Ledger, Projection, FACT_RECORDED, SNAPSHOT_TAKEN};
use autoclimb_types::run::{Coverage, Fact, FactKind, Snapshot};
use chrono::Utc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).ok_or("usage: ledger_smoke PATH")?;
    let mut ledger = Ledger::open(path)?;
    let snapshot = Snapshot {
        repo_id: "ledger-smoke".into(),
        head: "head".into(),
        tree: "tree".into(),
        dirty_digest: "clean".into(),
        file_universe: Coverage {
            seen: 1,
            total: 1,
            excluded: Vec::new(),
        },
        tool_versions: BTreeMap::new(),
        taken_at: Utc::now(),
    };
    let snapshot_id = ledger
        .append(SNAPSHOT_TAKEN, &snapshot)?
        .payload_hash
        .clone();
    ledger.append(
        FACT_RECORDED,
        &Fact {
            id: "fact-1".into(),
            snapshot: snapshot_id,
            subject: "smoke".into(),
            predicate: "runs".into(),
            value: true.into(),
            kind: FactKind::Observation,
            evidence: vec!["direct execution".into()],
            denominator: None,
        },
    )?;
    let projection = Projection::replay(&ledger)?;
    println!(
        "snapshots={} facts={}",
        projection.snapshots.len(),
        projection.facts.len()
    );
    Ok(())
}
