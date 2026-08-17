# codebase-autoclimb — design

`autoclimb` is an autonomous, proof-carrying hill-climb of a codebase toward
its own clarified intent. It descends from a slop scanner (a score, a queue,
an agent loop) and refounds it as a repository transformation system: infer
what the repository is trying to be, obtain authority for a clarified target,
move the repository monotonically toward it, and leave behind stronger
machinery for knowing that future changes are safe.

The one-line difference from upstream: upstream optimizes a score. Autoclimb
optimizes against a maintainer-ratified **RuleSet** — purpose, protected
surfaces, compatibility policy, verifier commands, risk ceiling, budget — and
treats scores as evidence, never as the objective. Score-chasing is the
failure mode ("cosmetically cleaner, conceptually worse") that
conscientiousness exists to prevent.

Construction order (settled after adversarial review, 2026-08-17): the missing
primitive is not the constitution — it is the **safe change transaction**:

```
exact input tree → bounded authority → patch → independent verification
→ exact output tree → measured delta
```

We build a one-change transaction monitor first and promote abstractions only
after the loop has consolidated a real duplication on this very repository.
The governance superstructure (question frontier, campaign DAG, protocol
library, steward) is the horizon, not day one.

## 0. What "an order of magnitude more conscientious" means operationally

| Slop scanner behaviour | Autoclimb behaviour |
|---|---|
| Emits findings | Emits snapshot-bound **facts** with evidence, denominators, freshness |
| One score to maximise | A RuleSet with hard floors that are never traded for elegance |
| `next` picks the top finding | A **Change** with a thesis, declared write set, risk class, and predicted effects |
| Agent "fixes it and resolves" | Every change carries **verification** on a ladder chosen by semantic risk class, not diff size |
| Asks nothing / asks constantly | Asks the maintainer ≤2 questions, only when the answer changes the next change's authority; conservative default otherwise |
| Forgets between sessions | Single append-only event ledger; predicted-vs-realized is measured on rescan |
| "Cleanup" is its own justification | Every change points back to a RuleSet objective; unlineaged cleanup is discarded |
| Trusts the agent's report | Agent claims can never satisfy verification — only orchestrator-run commands and orchestrator-computed git data count |

## 1. The transaction loop (build target)

```
                 target repository (untrusted, read-only by default)
                                   │
                              explore  ──►  Snapshot + Facts + EXPLORATION.md
                                   │        (denominators: seen/total/excluded)
                              frontier ──►  ≤2 Decisions + 1 proposed Change
                                   │
                             constitute ──►  RuleSet (canonical JSON, hashed;
                                   │         Markdown generated from it)
                                climb  ──►  one Change in one disposable lane:
                                   │         pin base → agent patch → path/write-set
                                   │         enforcement → verifier ladder →
                                   │         Verification packet → re-explore
                                   ▼
                            Verified (landing is a separate, human-visible step
                            until the loop has earned merge authority)
```

WIP limit is **one**. External-change policy is brutal at first: if HEAD, the
dirty digest, or the RuleSet hash differs from what the Change pinned, `climb`
refuses to continue. Selective invalidation comes later.

The inherited workspace (detectors, tree-sitter, graph, scoring, state, plan,
review, language crates) is the **evidence plane**. It stays as-is in role and
shrinks in authority: its findings become facts with provenance; its score
becomes one evidence stream; its ranked queue seeds proposed changes.

## 2. The IR (minimal, day one)

Six objects. Each absorbs what a fatter ontology would have split out; new
entities must displace an incumbent, not join it.

```
Snapshot     { repo_id, head, tree, dirty_digest, file_universe: {seen, total,
               excluded: [(what, why)]}, tool_versions, taken_at }
Fact         { id, snapshot, subject, predicate, value, kind: Observation|
               Contradiction|Unknown, evidence: [locator], denominator? }
Decision     { id, gates: [Branch], evidence: [FactId], why_not_inferable,
               recommendation, consequences, default_if_delegated,
               status: Open|Answered{chosen, raw_text}|Superseded }
RuleSet      { purpose, non_goals, allowed_paths, protected_paths,
               compatibility, verifier_commands, risk_ceiling: R0..R5,
               budget: {attempts, wall_secs, subprocesses}, hash }
Change       { id, thesis, lineage: RuleSetRef, base: Snapshot, brief_hash,
               write_set: [PathGlob], risk_class: R0..R5,
               predicted: [Fact], status: Planned|Lane|Verifying|Verified|Discarded }
Verification { change, base_tree, result_tree, patch_hash, ruleset_hash,
               levels_run: [(Level, Verdict, output_digest)],
               behaviour_changed, behaviour_preserved,
               residual_uncertainty, realized: [Fact], rollback }
```

Notes that carry the conscientiousness:

- **Snapshot is load-bearing.** A commit id alone does not describe dirty
  files, exclusions, tool versions, or the discovered file universe.
- Coverage-shaped facts must declare their universe: not "well tested" but
  `1138 / 1204 production symbols exercised; 3 excluded environments`.
  Unknowns stay visible; they are never converted into confidence.
- The word is **verification**, not proof. Builds and tests reduce
  uncertainty; they do not prove semantic preservation.
- Every agent invocation is logged as an `Attempt` event (backend, brief
  hash, budget, exit, transcript hash, produced tree) without becoming a
  projected entity.

Deferred until the loop demands them: Campaign (a DAG over Changes),
Observation (post-merge monitoring), the protocol DSL, quality-vector
weights, compiled architectural laws, multi-lane concurrency, agent roles
beyond cartographer/implementer. Deferral is recorded here so their later
introduction is a decision, not drift.

## 3. Ledger and state

One append-only `events.jsonl` — a single stream, because multiple streams
lose atomic cross-entity ordering (a crash could persist an answered decision
without its rule patch). Projections are rebuilt in memory on load; no
database. The loader enforces, failing closed:

1. Contiguous `seq`; unique event ids; order never derived from timestamps.
2. Schema version + payload hash + previous-event hash per line; only a torn
   final line is recoverable, corruption elsewhere is fatal.
3. `repo_id` matches (path and remote URL are not identifiers — this repo's
   origin still says `desloppify.git`).
4. Every event names an exact Snapshot whose objects exist locally.
5. Exhaustive legal state transitions; immutable fields stay immutable
   (change base, brief hash, write set, risk class, decision branches).
6. Agent processes never write the ledger; the orchestrator is the single
   writer.
7. Unknown event kinds fail closed.

Two state classes, kept separate:

- **Versioned authority** — `ruleset.json` (canonical, hashed) and answered
  decisions: tracked in git. `RULESET.md` is *generated* from the JSON with
  an embedded hash; independent editing of both is structurally impossible.
- **Local operational history** — `.autoclimb/events.jsonl`, lanes,
  transcripts, packets: gitignored, machine-local, never claimed as the
  repository's source of truth.

## 4. Risk classes and the verifier ladder

```
R0 docs / generated metadata            L0 parse · format · generated-file consistency
R1 local pure refactor                  L1 build · types · static analysis
R2 cross-module internal change         L2 focused existing tests
R3 public API / dependency change       L3 integration checks
R4 persistent state / deploy / concurrency  L4 API · schema · golden-output comparison
R5 auth / billing / privacy / destructive   L5+ property/mutation/fuzz · perf budgets ·
                                             security invariants · shadow/canary · post-merge
```

Required levels come from risk class, not line count: a 5,000-line
deterministic codemod is R1; a six-line authorization change is R5.
Verifiers are the same commands a human would run, named in the RuleSet.
Flaky checks are recorded per attempt and classified inconclusive —
retry-until-green is not verification. Verifier surfaces (tests, CI config,
exclusions, the orchestrator itself while dogfooding) are protected paths by
default: an agent that can weaken the judge has not improved the code.

## 5. Question frontier (kept sparse by construction)

A question is admissible only if its answer changes the **next change's**
allowed paths, compatibility behaviour, verifier set, risk ceiling, or
acceptance criterion. Rank ordinally:

```
priority = impact(1..4) × irreversibility(1..3) × branch_divergence(1..3)
           × uncertainty(1..3) ÷ answer_minutes(1|3|10)
```

Ask only when priority ≥ 8, the question blocks the next change, and the
answer is not inferable from ratified rules, observed behaviour, docs, or
consistent history. At most two questions survive any frontier pass; below
threshold, take the conservative default and record it as a delegated
decision. Never ask "what should this repository become?"

Answers compile without a taxonomy explosion: keep the raw text verbatim;
propose a JSON Merge Patch against the fixed RuleSet schema; accept only
known fields; every machine rule must trace to a presented branch or an
explicit sentence; ambiguity leaves the decision unresolved rather than
minting authority.

## 6. Agents

```
trait AgentBackend { fn run(&self, brief: &Brief, lane: &Lane, budget: &Budget) -> Attempt }
```

Backends: `codex` (adapter over the inherited subprocess runner), `claude`,
`manual`. Agents receive a brief pinned to a base snapshot, work in a
disposable git worktree lane, and return a tree. The orchestrator computes
the diff, enforces the write set and protected paths, runs the ladder, and
decides. Agents never merge, never push, never touch the ledger. Repository
content is untrusted input — nothing an agent reads may alter its authority,
output schema, budget, or verifier commands. Hard limits on attempts, wall
time, and subprocess count; eloquent claims are laundering, so every asserted
path, hash, call-site count, and command result is recomputed outside the
agent.

## 7. First dogfood transaction

Concept consolidation is the right first transformation class; the inherited
duplicate-code detector is the wrong trigger (it is integrated into 0/3
language plugins and emits nothing — itself a fact the twin must record).
The honest target is already visible in this repository:

`autoclimb-review/src/import.rs` (live, 3/3 CLI call sites) vs
`import_pipeline.rs` (competing, effectively dead implementation whose trust
check compares the supplied packet hash to itself instead of recomputing the
stored packet — and the live path bypasses trust entirely).

The transaction: inventory both representations and all call sites → one
question (may internally generated subjective scores become durable without
independent packet-hash verification? conservative default: no) → define one
canonical import API → migrate callers → verify (exact base tree, enumerated
mode behaviour, recomputed packet hashes, existing checks unchanged, 3/3
callers migrated, 1/2 implementations remaining, single-revert rollback) →
remove the loser. Hard-coded in Rust; the protocol DSL waits for the second
genuinely different protocol.

## 8. Semantic entropy (the objective, stated once)

Success is not a higher score. It is: fewer representations per concept,
fewer implicit state transitions, fewer dependency directions, fewer
side-effect origins, narrower public surfaces, better correspondence between
stated and executed architecture, faster deterministic feedback, less context
needed for a correct change. The qualitative test: can a fresh maintainer or
agent describe the system faithfully with substantially fewer concepts after
the change? Metric deltas are evidence of this, never the target — and
file-count, public-surface, and config deltas are recorded beside scanner
deltas precisely so deletion-and-exclusion gaming shows up.

## 9. Build plan (slices ≤ ~300 lines each)

1. **Run model + ledger** — Snapshot/Fact/Decision/RuleSet/Change/Verification
   in `autoclimb-types`; single locked JSONL loader in `autoclimb-state`
   reusing its atomic-write/locking discipline. No new crate until forced.
2. **Scan boundary** — extract finding-collection from the CLI's `run_scan`
   into a `ScanOutput` seam so explore can call it as a library.
3. **`explore`** — record cleanliness, HEAD/tree/tool versions; run the scan;
   translate findings into snapshot-bound facts; render `EXPLORATION.md`
   with seen/total/excluded. Reject stale state rather than importing it.
4. **Lanes** — disposable worktree creation, exact-HEAD checks,
   write-set/protected-path diff enforcement, cleanup; adapter over the
   inherited codex subprocess runner.
5. **`frontier`** — compact facts + inherited queue → one cartographer
   invocation → ≤2 typed decisions + 1 proposed change; narrative-only
   output rejected.
6. **`constitute`** — apply chosen branches or conservative defaults to the
   RuleSet; canonical JSON, generated Markdown, embedded hash.
7. **`climb`** — execute exactly one change end to end; stop at Verified.
8. **Dogfood** — run the review-import consolidation on this repo; promote
   only the abstractions the loop actually demanded.

## 10. Non-goals

- Not a GitHub Actions framework (a thin Action can wrap the CLI later).
- Not a general SWE agent; agents are pluggable substrates.
- Not a scoreboard product; the badge is inherited, not central.
- Not a fleet/multi-repo optimizer yet; the ledger is designed so fleet
  learning can read it later.
- No auto-merge until the transaction loop has a track record.
