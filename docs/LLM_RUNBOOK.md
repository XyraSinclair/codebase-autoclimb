# LLM Runbook

Use this runbook when delegating `autoclimb` to another LLM.

## Copy-Paste Prompt

```text
You are operating the Rust implementation of autoclimb from this repository checkout.

Use only this entry point:
scripts/autoclimb-local

Do not assume a bare `autoclimb` on the machine points to the correct version.

Target codebase: /absolute/path/to/codebase

Execution policy:
1. Persist excludes for generated, vendored, build, archive, and migration directories if they exist.
2. Start with `scan`, then `status`, then `queue`, then `plan show`, then `next`.
3. Do not start with `review`.
4. Do not run mutating commands until you have shown me the findings and proposed next actions.
5. If review is requested, use:
   `scripts/autoclimb-local review --prepare --path /absolute/path/to/codebase`
   `scripts/autoclimb-local review --run-batches --backend codex --mode findings_only --path /absolute/path/to/codebase`
6. Do not use `--mode trusted` unless I explicitly ask you to apply subjective assessments to scores.
7. Use `--dry-run` first for `fix` and `move`.
8. Summarize what you ran, what you found, and the highest-priority findings with file paths.

If the repository is polyglot or auto-detect looks wrong, use explicit `--lang`
or scan separate language roots individually.
```

## Minimal Command Sequence

```bash
AUTOCLIMB=scripts/autoclimb-local
TARGET=/absolute/path/to/codebase

$AUTOCLIMB exclude add --path "$TARGET" node_modules
$AUTOCLIMB exclude add --path "$TARGET" dist
$AUTOCLIMB exclude add --path "$TARGET" build
$AUTOCLIMB exclude add --path "$TARGET" vendor
$AUTOCLIMB exclude add --path "$TARGET" archive

$AUTOCLIMB scan --path "$TARGET"
$AUTOCLIMB status --path "$TARGET"
$AUTOCLIMB queue --path "$TARGET"
$AUTOCLIMB plan show --path "$TARGET"
$AUTOCLIMB next --path "$TARGET"
```

Remove any exclude lines that do not make sense for the target repo. Add more
if the repo contains generated or third-party trees under other names.

## Review Policy

Use review only after a fresh scan.

Safe default:

```bash
$AUTOCLIMB review --prepare --path "$TARGET"
$AUTOCLIMB review --run-batches --backend codex --mode findings_only --path "$TARGET"
```

Important constraints:

- `review --run-batches` is for Codex only right now.
- Other reviewers should use `review --external-start --runner ...`.
- `--mode trusted` mutates persisted subjective score surfaces; durable
  scores require an independently verified packet hash, else they import as
  provisional.
- `--force-review-rerun` is only for intentional stale reruns.

## Mutating Commands

Before any mutating step, the LLM should show the command and run the dry-run
form first:

```bash
$AUTOCLIMB fix --path "$TARGET" --dry-run
$AUTOCLIMB move --path "$TARGET" --dry-run src/old_file.ts src/new_file.ts
```

The non-dry-run command should only happen after human approval.

## Expected Files In The Target Repo

Autoclimb stores its working state under:

- `.autoclimb/state.json`
- `.autoclimb/config.json`
- `.autoclimb/` review artifacts

Those files should usually be kept between scans so the tool can track history.

## Red Flags

Stop and reassess if any of these happen:

- The LLM starts with `review` before a scan exists.
- The LLM uses bare `autoclimb` without proving it came from this checkout.
- The LLM scans vendored, generated, or archived trees as if they were live code.
- The LLM uses `--mode trusted` without explicit approval.
- The LLM runs `fix` or `move` without `--dry-run`.

## Transaction Loop (advanced)

For autonomous repository change under proof, use the transaction loop instead
of ad-hoc edits: `explore` -> `frontier` -> `decide` -> `constitute` ->
`climb`. See "The Transaction Loop" in `docs/USAGE.md` and DESIGN.md.
Non-negotiables for an LLM operator:

- `climb` executes ONE change per transaction and stops at Verified; landing
  is a separate, human-reviewable git step.
- Never edit `.autoclimb/events.jsonl` by hand; it is a hash-chained ledger
  and the loader fails closed on tampering.
- If `climb` refuses (stale HEAD, stale RuleSet hash, open decisions), the
  refusal is correct: re-plan, do not force.
