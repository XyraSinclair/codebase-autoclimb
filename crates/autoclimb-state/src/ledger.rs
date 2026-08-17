use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use autoclimb_types::run::{
    Budget, Change, ChangeStatus, Decision, DecisionStatus, Fact, RuleSet, Snapshot, Verification,
};
use chrono::{DateTime, SecondsFormat, Utc};
use fs2::FileExt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const SNAPSHOT_TAKEN: &str = "snapshot_taken";
pub const FACT_RECORDED: &str = "fact_recorded";
pub const DECISION_OPENED: &str = "decision_opened";
pub const DECISION_ANSWERED: &str = "decision_answered";
pub const RULESET_RATIFIED: &str = "ruleset_ratified";
pub const CHANGE_PLANNED: &str = "change_planned";
pub const CHANGE_STATUS: &str = "change_status";
pub const ATTEMPT_LOGGED: &str = "attempt_logged";
pub const VERIFICATION_RECORDED: &str = "verification_recorded";
const SCHEMA: u32 = 1;
const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[rustfmt::skip]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub seq: u64, pub event_id: String, pub schema: u32, pub prev_hash: String,
    pub payload_hash: String, pub at: DateTime<Utc>, pub kind: String, pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerWarning {
    TruncatedFinalLine { offset: u64 },
}

#[rustfmt::skip]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionAnswer { pub decision_id: String, pub chosen: String, pub raw_text: String }

#[rustfmt::skip]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeStatusEvent { pub change_id: String, pub status: ChangeStatus }

#[rustfmt::skip]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptLogged {
    pub backend: String, pub brief_hash: String, pub budget: Budget,
    pub started_at: DateTime<Utc>, pub ended_at: DateTime<Utc>, pub exit: String,
    pub transcript_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub produced_tree: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("event {seq}: {message}")]
    Event { seq: u64, message: String },
}

pub struct Ledger {
    file: File,
    _lock: File,
    events: Vec<EventEnvelope>,
    warnings: Vec<LedgerWarning>,
}

impl Ledger {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LedgerError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let lock = File::create(path.with_extension("jsonl.lock"))?;
        lock.lock_exclusive()?;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        let (events, warnings) = load_events(&mut file)?;
        Projection::from_events(&events)?;
        Ok(Self {
            file,
            _lock: lock,
            events,
            warnings,
        })
    }

    pub fn events(&self) -> &[EventEnvelope] {
        &self.events
    }
    pub fn warnings(&self) -> &[LedgerWarning] {
        &self.warnings
    }

    pub fn append<T: Serialize>(
        &mut self,
        kind: &str,
        payload: &T,
    ) -> Result<&EventEnvelope, LedgerError> {
        let seq = self.events.len() as u64 + 1;
        let payload = serde_json::to_value(payload)?;
        let payload_hash = hash(&payload)?;
        let prev_hash = self
            .events
            .last()
            .map_or(GENESIS, |event| &event.payload_hash)
            .to_owned();
        let at = Utc::now();
        let event_id = digest(format!(
            "{seq}:{kind}:{prev_hash}:{payload_hash}:{}",
            at.to_rfc3339_opts(SecondsFormat::Nanos, true)
        ));
        let event = EventEnvelope {
            seq,
            event_id,
            schema: SCHEMA,
            prev_hash,
            payload_hash,
            at,
            kind: kind.into(),
            payload,
        };
        Projection::from_events(&self.events)?.apply(&event)?;
        self.file.seek(SeekFrom::End(0))?;
        serde_json::to_writer(&mut self.file, &event)?;
        self.file.write_all(b"\n")?;
        self.file.sync_all()?;
        self.events.push(event);
        Ok(self.events.last().expect("appended event exists"))
    }
}

#[rustfmt::skip]
#[derive(Debug, Default)]
pub struct Projection {
    pub snapshots: BTreeMap<String, Snapshot>, pub facts: BTreeMap<String, Fact>,
    pub decisions: BTreeMap<String, Decision>, pub ruleset: Option<(RuleSet, String)>,
    pub changes: BTreeMap<String, Change>, pub verifications: BTreeMap<String, Verification>,
    repo_id: Option<String>,
}

impl Projection {
    pub fn replay(ledger: &Ledger) -> Result<Self, LedgerError> {
        Self::from_events(&ledger.events)
    }

    fn from_events(events: &[EventEnvelope]) -> Result<Self, LedgerError> {
        let mut projection = Self::default();
        for event in events {
            projection.apply(event)?;
        }
        Ok(projection)
    }

    fn apply(&mut self, event: &EventEnvelope) -> Result<(), LedgerError> {
        let seq = event.seq;
        match event.kind.as_str() {
            SNAPSHOT_TAKEN => {
                let snapshot: Snapshot = decode(event)?;
                coverage(
                    seq,
                    snapshot.file_universe.seen,
                    snapshot.file_universe.total,
                )?;
                if self
                    .repo_id
                    .as_ref()
                    .is_some_and(|id| id != &snapshot.repo_id)
                {
                    return fail(seq, "snapshot repo_id differs from the ledger repo_id");
                }
                self.repo_id.get_or_insert_with(|| snapshot.repo_id.clone());
                if self
                    .snapshots
                    .insert(event.payload_hash.clone(), snapshot)
                    .is_some()
                {
                    return fail(seq, "duplicate snapshot payload hash");
                }
            }
            FACT_RECORDED => {
                let fact: Fact = decode(event)?;
                self.check_fact(seq, &fact)?;
                if self.facts.insert(fact.id.clone(), fact).is_some() {
                    return fail(seq, "duplicate fact id");
                }
            }
            DECISION_OPENED => {
                let decision: Decision = decode(event)?;
                if decision.status != DecisionStatus::Open {
                    return fail(seq, "decision_opened payload is not open");
                }
                if decision
                    .evidence
                    .iter()
                    .any(|id| !self.facts.contains_key(id))
                {
                    return fail(seq, "decision names an unknown fact");
                }
                if let Some(prior) = self.decisions.get(&decision.id) {
                    let mut reopened = prior.clone();
                    reopened.status = DecisionStatus::Open;
                    if reopened != decision || prior.status == DecisionStatus::Open {
                        return fail(
                            seq,
                            "decision reopening changed immutable fields or was already open",
                        );
                    }
                }
                self.decisions.insert(decision.id.clone(), decision);
            }
            DECISION_ANSWERED => {
                let answer: DecisionAnswer = decode(event)?;
                let decision = self
                    .decisions
                    .get_mut(&answer.decision_id)
                    .ok_or_else(|| error(seq, "answer names an unknown decision"))?;
                if decision.status != DecisionStatus::Open {
                    return fail(seq, "decision was answered without an explicit reopening");
                }
                if !decision
                    .gates
                    .iter()
                    .any(|branch| branch.id == answer.chosen)
                {
                    return fail(seq, "answer chose an unknown branch");
                }
                decision.status = DecisionStatus::Answered {
                    chosen: answer.chosen,
                    raw_text: answer.raw_text,
                };
            }
            RULESET_RATIFIED => {
                let ruleset: RuleSet = decode(event)?;
                let hash = ruleset.canonical_hash();
                if !ruleset.hash.is_empty() && ruleset.hash != hash {
                    return fail(seq, "ruleset hash does not match canonical JSON");
                }
                self.ruleset = Some((ruleset, hash));
            }
            CHANGE_PLANNED => {
                let change: Change = decode(event)?;
                if change.status != ChangeStatus::Planned {
                    return fail(seq, "change_planned payload is not planned");
                }
                self.check_snapshot(seq, &change.base)?;
                if self.ruleset.as_ref().map(|(_, hash)| hash) != Some(&change.lineage) {
                    return fail(seq, "change lineage is not the latest ratified ruleset");
                }
                for fact in &change.predicted {
                    self.check_fact(seq, fact)?;
                }
                if let Some(prior) = self.changes.get(&change.id) {
                    if prior.base != change.base
                        || prior.brief_hash != change.brief_hash
                        || prior.write_set != change.write_set
                        || prior.risk_class != change.risk_class
                    {
                        return fail(
                            seq,
                            "re-planning changed base, brief_hash, write_set, or risk_class",
                        );
                    }
                    return fail(seq, "change id was already planned");
                }
                self.changes.insert(change.id.clone(), change);
            }
            CHANGE_STATUS => {
                let update: ChangeStatusEvent = decode(event)?;
                let change = self
                    .changes
                    .get_mut(&update.change_id)
                    .ok_or_else(|| error(seq, "status names an unknown change"))?;
                if !matches!(
                    (change.status, update.status),
                    (ChangeStatus::Planned, ChangeStatus::Lane)
                        | (ChangeStatus::Lane, ChangeStatus::Verifying)
                        | (
                            ChangeStatus::Verifying,
                            ChangeStatus::Verified | ChangeStatus::Discarded
                        )
                ) {
                    return fail(
                        seq,
                        format!(
                            "illegal change transition {:?} -> {:?}",
                            change.status, update.status
                        ),
                    );
                }
                change.status = update.status;
            }
            ATTEMPT_LOGGED => {
                let _: AttemptLogged = decode(event)?;
            }
            VERIFICATION_RECORDED => {
                let verification: Verification = decode(event)?;
                let change = self
                    .changes
                    .get(&verification.change)
                    .ok_or_else(|| error(seq, "verification names an unknown change"))?;
                if change.status != ChangeStatus::Verifying {
                    return fail(seq, "verification was recorded outside Verifying");
                }
                if verification.base_tree != change.base.tree
                    || verification.ruleset_hash != change.lineage
                {
                    return fail(
                        seq,
                        "verification base tree or ruleset hash differs from the change",
                    );
                }
                for fact in &verification.realized {
                    self.check_fact(seq, fact)?;
                }
                if self
                    .verifications
                    .insert(verification.change.clone(), verification)
                    .is_some()
                {
                    return fail(seq, "duplicate verification for change");
                }
            }
            kind => return fail(seq, format!("unknown kind {kind}")),
        }
        Ok(())
    }

    fn check_fact(&self, seq: u64, fact: &Fact) -> Result<(), LedgerError> {
        if let Some(value) = &fact.denominator {
            coverage(seq, value.seen, value.total)?;
        }
        if !self.snapshots.contains_key(&fact.snapshot) {
            return fail(seq, format!("fact {} names an unknown snapshot", fact.id));
        }
        Ok(())
    }

    fn check_snapshot(&self, seq: u64, snapshot: &Snapshot) -> Result<(), LedgerError> {
        let id = hash(&serde_json::to_value(snapshot)?)?;
        if self.snapshots.get(&id) != Some(snapshot) {
            return fail(seq, "change base snapshot is not in the ledger");
        }
        Ok(())
    }
}

fn load_events(file: &mut File) -> Result<(Vec<EventEnvelope>, Vec<LedgerWarning>), LedgerError> {
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let (mut events, mut ids, mut offset, mut warnings) =
        (Vec::new(), BTreeSet::new(), 0, Vec::new());
    for (index, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
        let start = offset;
        offset += line.len() + 1;
        if line.is_empty() {
            if start < bytes.len() {
                return fail(index as u64 + 1, "empty ledger line");
            }
            continue;
        }
        let event = match serde_json::from_slice(line) {
            Ok(event) => event,
            Err(cause) if start + line.len() == bytes.len() && cause.is_eof() => {
                file.set_len(start as u64)?;
                file.sync_all()?;
                warnings.push(LedgerWarning::TruncatedFinalLine {
                    offset: start as u64,
                });
                break;
            }
            Err(cause) => return fail(index as u64 + 1, format!("invalid JSON: {cause}")),
        };
        verify(&event, events.last(), &mut ids)?;
        events.push(event);
    }
    Ok((events, warnings))
}

fn verify(
    event: &EventEnvelope,
    prior: Option<&EventEnvelope>,
    ids: &mut BTreeSet<String>,
) -> Result<(), LedgerError> {
    let expected = prior.map_or(1, |item| item.seq + 1);
    if event.seq != expected {
        return fail(
            event.seq,
            format!("non-contiguous seq; expected {expected}"),
        );
    }
    if event.schema != SCHEMA {
        return fail(event.seq, format!("unsupported schema {}", event.schema));
    }
    if !ids.insert(event.event_id.clone()) {
        return fail(event.seq, "duplicate event_id");
    }
    if event.prev_hash != prior.map_or(GENESIS, |item| &item.payload_hash) {
        return fail(event.seq, "previous payload hash mismatch");
    }
    if event.payload_hash != hash(&event.payload)? {
        return fail(event.seq, "payload hash mismatch");
    }
    Ok(())
}

fn decode<T: DeserializeOwned>(event: &EventEnvelope) -> Result<T, LedgerError> {
    serde_json::from_value(event.payload.clone())
        .map_err(|cause| error(event.seq, format!("invalid payload: {cause}")))
}
fn coverage(seq: u64, seen: u64, total: u64) -> Result<(), LedgerError> {
    if seen > total {
        return fail(seq, format!("coverage seen {seen} exceeds total {total}"));
    }
    Ok(())
}
fn hash(value: &Value) -> Result<String, serde_json::Error> {
    Ok(digest(serde_json::to_string(value)?))
}
fn digest(value: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(value.as_ref()))
}
fn error(seq: u64, message: impl Into<String>) -> LedgerError {
    LedgerError::Event {
        seq,
        message: message.into(),
    }
}
fn fail<T>(seq: u64, message: impl Into<String>) -> Result<T, LedgerError> {
    Err(error(seq, message))
}
