# codebase-autoclimb — design

`autoclimb` is an autonomous, proof-carrying hill-climb of a codebase toward
its own clarified intent. It descends from a slop scanner (a score, a queue,
an agent loop) and refounds it as a repository transformation system: infer
what the repository is trying to be, obtain authority for a clarified target,
move the whole repository monotonically toward it, and leave behind stronger
machinery for knowing that future changes are safe.

The one-line difference from upstream: upstream optimizes a score. Autoclimb
optimizes a **constitution** — a repository-specific, evidence-backed,
maintainer-ratified statement of purpose, invariants, laws, and quality
weights — and treats scores as evidence, never as the objective. Score-chasing
is the failure mode ("cosmetically cleaner, conceptually worse") that
conscientiousness exists to prevent.

## 0. What "an order of magnitude more conscientious" means operationally

| Slop scanner behaviour | Autoclimb behaviour |
|---|---|
| Emits findings | Emits **claims** with evidence, denominators, confidence, freshness |
| One score to maximise | A constitution: hard floors, laws, and a weighted quality vector that never trades floors for elegance |
| `next` picks the top finding | A **campaign DAG** picks a portfolio: prerequisites, unlocks, risk concentration, reviewer budget |
| Agent "fixes it and resolves" | Every tranche carries a **proof** on a verifier ladder chosen by semantic risk class, not diff size |
| Asks nothing / asks constantly | Asks the maintainer only at the **decision frontier**, as decision packets with a conservative default; then works under a delegation contract |
| Forgets between sessions except state.json | Append-only **ledger** of every claim, question, answer, tranche, proof, and observation; predicted-vs-realized is measured |
| "Cleanup" is its own justification | Every tranche points back to a constitution identifier; unlineaged cleanup is discarded |
| Leaves the repo cleaner | Leaves the repo **enforcing** its clarified intent (compiled laws: dependency checks, CI rules, generated tests) |

## 1. Planes

```
                 target repository (untrusted, read-only by default)
                                   │
   ┌───────────────────────────────┼──────────────────────────────┐
   ▼                               ▼                              ▼
evidence plane              history & intent plane        behaviour plane
(detectors, tree-sitter,     (git log, reverts, docs,      (build/test/lint
 dupes, graph, scores)        issues, ADRs, owners)         runners, traces)
   └───────────────────────────────┼──────────────────────────────┘
                                   ▼
                        twin: claims · invariants · contradictions · unknowns
                                   │
                          question frontier ── decision packets ── answers
                                   │
                              constitution + delegation contract
                                   │
                    campaign (DAG of tranches with proof obligations)
                                   │
        ┌───────── climb loop ─────┼────────────────────────────────┐
        │  select tranche → lane (git worktree) → implement (agent) │
        │  → verifier ladder → proof packet → land / discard        │
        │  → observe (predicted vs realized) → invalidate → replan  │
        └───────────────────────────────────────────────────────────┘
                                   │
                        steward (drift prevention, low cost)
```

The **evidence plane is the inherited workspace** (`autoclimb-detectors`,
`-treesitter`, `-graph`, `-scoring`, `-state`, `-plan`, `-review`, language
crates). It stays as-is in role and shrinks in authority: its findings become
claims with provenance; its score becomes one quality-vector component; its
`plan` becomes the seed for campaign tranches.

Everything else is new and lives in one crate, `autoclimb-core`, plus a thin
CLI surface. The orchestrator is deterministic Rust; agents are subprocesses
that emit patches and claims, never exercise ambient authority.

## 2. The IR (canonical objects)

Every object is an event-sourced entity: it has an id, provenance
(`who`, `when`, `from_commit`), and is written as an append-only event under
`.autoclimb/ledger/<stream>.jsonl`. Projections are rebuilt in memory on
load; no database. Facts are separate from interpretation.

```
Claim        { id, subject, predicate, value, evidence: [EvidenceRef],
               confidence: 0..1, fresh_at: commit, affected_by: [PathGlob|Symbol] }
EvidenceRef  { kind: Detector|Test|Trace|Doc|GitHistory|Human|Agent,
               locator, denominator?: {seen, total, excluded: [(what, why)]} }
Invariant    { id, scope, statement, severity: Floor|Law|Preference,
               verifier: VerifierRef?, phase: Draft|Ratified|Compiled }
Contradiction{ id, claims: [ClaimId], consequence, resolutions: [String] }
Unknown      { id, scope, why_unknown, cheapest_probe, decision_risk: R0..R5 }

Question     { id, gates: [Branch], evidence: [ClaimId], coverage,
               why_not_inferable, recommendation, consequences: {branch → text},
               default_if_delegated: Branch, authority: Owner, expires: Option<Date> }
Answer       { question, chosen: Branch, by, text, compiled: [Constraint] }
Constraint   { key: dotted.path, value, source: AnswerId | Inferred, until: Option<Date> }

Constitution { purpose, non_goals, invariants: [InvariantId], laws: [InvariantId],
               quality_vector: {dimension → weight | Floor}, compat_policy,
               protected_surfaces: [PathGlob], delegation: Delegation }
Delegation   { write_surfaces, merge_risk_ceiling: R0..R5, budget: {agent_minutes, usd?},
               max_concurrent_tranches, protected_branches, escalate_on: [Condition] }

Campaign     { id, mode: Stabilize|Consolidate|Refound, targets: [InvariantId],
               forbidden_regressions: [ClaimId], tranches: [TrancheId],
               edges: [(from, to, kind: Requires|Unlocks|Conflicts)],
               completion: Predicate }
Tranche      { id, thesis, protocol: ProtocolId, step, lineage: [ConstitutionRef],
               risk_class: R0..R5, required_levels: [L0..L9], deps,
               concepts_touched: [String], predicted: {claims_after: [Claim]},
               status: Planned|Lane|Implementing|Verifying|Packet|Landed|Discarded }
Proof        { tranche, base_commit, precondition_hashes, levels_run: [(Level, Verdict, EvidenceRef)],
               behaviour_changed: [String], behaviour_preserved: [String],
               residual_uncertainty: [String], rollback: String }
Observation  { tranche, predicted: [Claim], realized: [Claim], deltas: [String] }
```

Denominators are mandatory on any coverage-shaped claim: not "well tested",
but `1138 / 1204 production symbols exercised; 3 excluded environments`.
Unknowns stay visible; they are never silently converted into confidence.

## 3. State machine

```
explore ──► frontier ──► constitute ──► plan ──► climb ⟲ ──► steward
   ▲                                              │
   └────────────── invalidate on merge/external commit/failed prediction
```

- **explore** (read-only): build the twin. Sources: the inherited scan
  (findings → claims), git history mining (churn, reverts, abandoned
  migrations, scars), docs/ADRs (asserted vs. observed behaviour), build/test
  command discovery, dependency graph, duplicated-concept inventory,
  side-effect sites, public surface. Output: `.autoclimb/twin/` projections
  and `EXPLORATION.md` (what we can and cannot see, with denominators).
- **frontier**: generate candidate questions from contradictions and
  high-`decision_risk` unknowns; keep those whose branches change the target
  architecture or risk profile; rank by expected regret removed per
  maintainer-minute; write `QUESTIONS.md` as decision packets. Answers are
  written inline (human) or by an agent under delegated authority; both are
  compiled to `Constraint`s.
- **constitute**: compile claims + answers into `CONSTITUTION.md` (human
  readable) and `constitution.json` (machine). Where a law can be enforced
  mechanically, emit its verifier (dependency-direction rule, forbidden import,
  CI check, generated test) — the finished repo enforces intent.
- **plan**: architects (independent agent runs) propose target states in
  Stabilize / Consolidate / Refound modes; adversaries attempt to falsify their
  assumptions; the steward selects on the value/risk/reversibility/review-cost
  frontier and emits a campaign DAG. Early tranches raise epistemic leverage
  (reproducible bootstrap, fast tests, characterization tests, seams) — earned
  autonomy: consequential transformations are only admissible in subsystems
  that are observable, testable, and reversible.
- **climb** (loop, WIP-limited): pick the highest-value admissible tranche;
  open a git worktree lane pinned to a base commit; run the protocol step with
  an agent backend; run the verifier ladder to the tranche's required level;
  assemble the proof packet; land (commit/PR) or discard; observe predicted
  vs. realized; invalidate affected claims; replan.
- **steward**: the campaign machinery contracts to a cheap drift monitor
  (compiled laws + periodic re-explore).

Escalation to a human happens only on: evidence contradicting the
constitution, a genuinely irreversible decision, unobtainable proof, budget or
risk ceiling breach, ambiguous authority, or external invalidation.

## 4. Risk classes and the verifier ladder

```
R0 docs / generated metadata            L0 parse · format · generated-file consistency
R1 local pure refactor                  L1 build · types · static analysis
R2 cross-module internal change         L2 focused unit tests
R3 public API / dependency change       L3 integration tests
R4 persistent state / deploy / concurrency  L4 API · schema · golden-output comparison
R5 auth / billing / privacy / destructive   L5 property · mutation · fuzz · fault injection
                                        L6 performance and cost budgets
                                        L7 security and custody invariants
                                        L8 shadow · canary · data reconciliation
                                        L9 post-merge observation
```

Required levels come from risk class, not line count: a 5,000-line
deterministic codemod is R1; a six-line authorization change is R5. Diff
budgets are expressed in concepts, contracts, and state transitions touched.
Verifiers are configured in the constitution (`verify.build`, `verify.test`,
`verify.lint`, plus repo-specific commands) and are the same commands a human
would run — no bespoke test frameworks.

## 5. Protocols

A universal prompt cannot safely handle everything; a library of typed,
composable transformation protocols can approach it. Each protocol declares
applicability, required evidence, preconditions, ordered steps with allowed
temporary violations, proof obligations per step, rollback, cutover criteria,
and legacy-removal criteria. Protocols are data (`protocols/*.toml`), the
engine is generic, and agents implement one *step* at a time under the step's
brief.

First protocol, end to end: **concept consolidation** —
inventory representations → infer current semantics → ask ≤2 consequential
questions → define canonical representation → characterization tests →
compatibility adapters → migrate producers/consumers → prove equivalence →
remove legacy → install a rule preventing recurrence. It exercises nearly the
whole architecture from the inherited duplicate-code evidence without needing
production access.

Rewrite admissibility is itself a protocol: a large rewrite is admissible only
when externally visible behaviour is enumerable, the old system can act as an
oracle, state is exportable/dual-runnable, consumers are known, performance is
measurable, and rollback is real. Otherwise strangler migration.

## 6. Agent backends and authority

```
trait AgentBackend { fn run(&self, brief: &Brief, lane: &Lane, budget: &Budget) -> Transcript }
```

Backends: `codex` (subprocess `codex exec`), `claude` (subprocess `claude -p`),
`manual` (write the brief to the lane, wait for a human/agent to complete and
`autoclimb tranche done`). Agents receive a brief pinned to a base commit and
return a working tree plus a claims file; the orchestrator computes the diff,
checks precondition hashes, and runs the ladder. Agents never merge, never
push, never touch protected surfaces; the delegation contract is enforced by
the orchestrator, not by prompt.

Roles (cartographer, historian, interviewer, architects, adversaries,
implementers, verifiers, steward, observer) are briefs over the same backend
trait, instantiated only when expected value exceeds cost. Mechanical work
never convenes a committee.

## 7. Objective: lower semantic entropy

Success is not a higher score. It is: fewer representations per concept, fewer
implicit state transitions, fewer dependency directions, fewer side-effect
origins, narrower public surfaces, better correspondence between stated and
executed architecture, faster deterministic feedback, less context needed for
a correct change. Each tranche maps to the concepts it contracts, separates, or
makes explicit. The qualitative test: can a fresh maintainer or agent describe
the system faithfully with substantially fewer concepts after the campaign?

## 8. Layout

```
crates/autoclimb-core       IR, ledger, twin, frontier, constitution, campaign, ladder, protocols
crates/autoclimb-agents     AgentBackend impls (codex, claude, manual)
crates/autoclimb-cli        `autoclimb` binary — inherited commands + explore/frontier/constitute/plan/climb/steward
crates/autoclimb-<evidence> inherited evidence plane (detectors, treesitter, graph, scoring, state, plan, review, langs)
protocols/*.toml            transformation protocols
docs/                       USAGE, LLM_RUNBOOK, PROTOCOLS
```

Target repository state lives under `<repo>/.autoclimb/`:
`ledger/*.jsonl` (append-only), `twin/` (projections), `QUESTIONS.md`,
`CONSTITUTION.md` + `constitution.json`, `campaign.json`, `lanes/`,
`packets/<tranche>.md`, and the inherited `state.json`/`config.json`.

## 9. Milestones

1. **Rename & refound** — workspace renamed, dead upstream snapshot removed, this design landed.
2. **IR + ledger** — `autoclimb-core` types, JSONL event store, projections, invariants enforced at load.
3. **explore** — twin from scan + git history + docs + command discovery; `EXPLORATION.md` with denominators.
4. **frontier + constitute** — question generation (agent), decision packets, answer compilation, `CONSTITUTION.md`, compiled verifiers for mechanically enforceable laws.
5. **plan + climb** — campaign DAG, lanes, backends, verifier ladder, proof packets, observation, replan; concept-consolidation protocol end to end.
6. **steward + dogfood** — run autoclimb on itself; measure predicted vs realized; publish.

## 10. Non-goals

- Not a GitHub Actions framework (a thin Action/App can wrap the CLI later).
- Not a general SWE agent; agents are pluggable substrates.
- Not a scoreboard product; the badge is inherited, not central.
- Not a fleet/multi-repo optimizer yet (the ledger is designed so fleet learning can read it later).
