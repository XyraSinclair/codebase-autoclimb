# Autoclimb Usage

This repository contains the active Rust implementation of `autoclimb`.

Use the checkout-local launcher when you want to guarantee that you are running
this repo's Rust CLI instead of an older binary:

```bash
scripts/autoclimb-local --help
```

That launcher works from any current working directory. It always executes the
Rust workspace in this checkout.

## Which Entry Point To Use

Use one of these two entry points:

```bash
scripts/autoclimb-local ...
```

```bash
cargo install --path crates/autoclimb-cli --force
autoclimb ...
```

Avoid these paths unless you intentionally want legacy behavior:

- Do not assume a preexisting bare `autoclimb` on your machine points at this checkout.

## What Autoclimb Writes

Autoclimb writes project state under the target repo's `.autoclimb/` folder.

- `.autoclimb/state.json`: latest scan state and finding history
- `.autoclimb/config.json`: persisted config such as excludes
- `.autoclimb/`: review packets and related workflow artifacts

If you scan the same project repeatedly, keep that directory. It is how the tool
tracks fixed findings, reopeners, and plan state over time.

## Safe First Pass On A New Codebase

Run the tool in this order:

```bash
AUTOCLIMB=scripts/autoclimb-local
TARGET=/absolute/path/to/codebase

$AUTOCLIMB scan --path "$TARGET"
$AUTOCLIMB status --path "$TARGET"
$AUTOCLIMB queue --path "$TARGET"
$AUTOCLIMB plan show --path "$TARGET"
$AUTOCLIMB next --path "$TARGET"
```

What each command is for:

- `scan`: collect findings and refresh `.autoclimb/state.json`
- `status`: see overall scores and project summary
- `queue`: inspect prioritized work items
- `plan show`: inspect the current living plan
- `next`: ask the tool for the next recommended action

`plan` and actionable `review` commands require an existing completed scan.
Start with `scan`, not `review`.

## Excluding Noise Before You Scan

Persist exclusions for vendored, generated, build, archive, or migration trees:

```bash
AUTOCLIMB=scripts/autoclimb-local
TARGET=/absolute/path/to/codebase

$AUTOCLIMB exclude add --path "$TARGET" node_modules
$AUTOCLIMB exclude add --path "$TARGET" dist
$AUTOCLIMB exclude add --path "$TARGET" build
$AUTOCLIMB exclude add --path "$TARGET" vendor
$AUTOCLIMB exclude add --path "$TARGET" archive
$AUTOCLIMB exclude list --path "$TARGET"
```

You can also pass one-off exclusions during a scan:

```bash
$AUTOCLIMB scan --path "$TARGET" --exclude node_modules --exclude dist
```

Prefer persisted excludes for real codebases so later scans and LLM review use
the same scope.

## Common Investigation Commands

Inspect findings:

```bash
$AUTOCLIMB show --path "$TARGET"
$AUTOCLIMB show --path "$TARGET" --tier 1
$AUTOCLIMB show --path "$TARGET" --detector long_function
$AUTOCLIMB show --path "$TARGET" --file src/
```

Resolve a finding after you intentionally fixed or dismissed it:

```bash
$AUTOCLIMB resolve --path "$TARGET" --status fixed finding_id_here
$AUTOCLIMB resolve --path "$TARGET" --status wontfix finding_id_here --note "Intentional tradeoff"
```

Run a single detector when debugging tool behavior:

```bash
$AUTOCLIMB detect --path "$TARGET" long_function
```

Generate artifacts for inspection:

```bash
$AUTOCLIMB tree --path "$TARGET"
$AUTOCLIMB viz --path "$TARGET" --output "$TARGET/autoclimb-report.html"
```

Check current language support:

```bash
$AUTOCLIMB langs
```

## Mutating Commands

Treat these as opt-in:

```bash
$AUTOCLIMB fix --path "$TARGET" --dry-run
$AUTOCLIMB move --path "$TARGET" --dry-run src/old.rs src/new.rs
```

Only run the non-dry-run form after you inspect the proposed changes.

## LLM Review Workflow

Use review only after a fresh scan:

```bash
$AUTOCLIMB review --prepare --path "$TARGET"
$AUTOCLIMB review --run-batches --backend codex --mode findings_only --path "$TARGET"
```

Important review constraints:

- `review --run-batches` currently supports the in-process Codex backend only.
- Non-Codex reviewers should go through `review --external-start --runner ...`.
- `--mode findings_only` is the safest default.
- `--mode trusted` applies subjective assessments into the persisted score
  surface; scores are durable only when the packet hash is independently
  verified against the stored blind-packet hash, else they import as
  provisional.
- `--force-review-rerun` is only for intentionally stale contexts.

If you want to hand review to another tool instead of the built-in Codex path:

```bash
$AUTOCLIMB review --external-start --runner claude --path "$TARGET"
```

Follow the generated instructions to submit the result back with
`review --external-submit ... --import`.

## Polyglot Repositories

Auto-detect is convenient, but mixed-language monorepos often need explicit
scoping.

Use one of these patterns:

```bash
$AUTOCLIMB scan --path "$TARGET" --lang rust
```

```bash
$AUTOCLIMB scan --path "$TARGET/services/api" --lang rust
$AUTOCLIMB scan --path "$TARGET/web" --lang typescript
```

If a repo has multiple independent language roots, scan each root separately
instead of assuming one top-level auto-detect pass captures everything well.

## What Not To Do

- Do not start with `review`.
- Do not use `--mode trusted` casually.
- Do not scan vendored or archived trees unless you mean to.
- Do not assume `fix` or `move` are safe without `--dry-run`.

## The Transaction Loop

The evidence plane above (scan/status/queue) measures. The transaction loop
*changes* the repository under proof. One full cycle:

```bash
$AUTOCLIMB explore --path "$TARGET"       # snapshot + facts -> ledger, EXPLORATION.md
$AUTOCLIMB frontier --path "$TARGET" --offline --input .autoclimb/frontier-input.json
$AUTOCLIMB decide --path "$TARGET" --decision <id> --branch <branch>
$AUTOCLIMB constitute --path "$TARGET"    # ratify the RuleSet (verifiers, protected paths, budgets)
$AUTOCLIMB climb --path "$TARGET"         # execute exactly ONE change transaction
```

What `climb` guarantees:

- **Brutal preconditions.** It refuses unless: repo clean, HEAD equals the
  planning-time snapshot, RuleSet hash equals the planning-time hash, exactly
  one Planned change, no open decisions, no stuck lane. Any drift means the
  plan is stale; retire it with `climb --discard-planned <change_id>` and
  re-plan.
- **Bounded authority.** The implementer works in a disposable git worktree
  lane and may touch only the change's declared write set; protected paths
  win over the write set. Violations discard the lane and print every
  offending path.
- **Independent verification.** The RuleSet's verifier commands run in the
  lane by the orchestrator, not the agent; agent claims never satisfy
  verification. First failure stops the ladder; later levels are recorded
  Inconclusive, never claimed.
- **Stop at Verified.** `climb` never lands. It prints the result tree hash,
  lane path, and diff command. Landing is the operator's git step: apply the
  lane files to main, commit, and confirm `git rev-parse "HEAD^{tree}"`
  equals the recorded result tree. Then remove the lane worktree.

Everything appends to `.autoclimb/events.jsonl`, a hash-chained single-writer
ledger; `.autoclimb/ruleset.json` + `RULESET.md` are the tracked authority.
Failed lanes are retained for inspection; `climb --discard-stuck` removes them
(it refuses to discard a Verified lane).

## LLM Handoff

If another LLM will operate this tool for you, hand it
`docs/LLM_RUNBOOK.md` from this repo. That file contains a copy-paste prompt
and an explicit execution policy for safe first use on another codebase.
