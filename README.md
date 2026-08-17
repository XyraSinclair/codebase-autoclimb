# codebase-autoclimb

`autoclimb` is an autonomous, proof-carrying hill-climb of a codebase toward
its own clarified intent.

Where a code-health scanner emits findings and chases a score, autoclimb runs
**safe change transactions**: it snapshots the exact repository state, records
evidence with denominators, asks the maintainer at most a couple of questions
whose answers actually change what it may do, ratifies that authority as a
hashed RuleSet, and then executes one bounded change at a time in a disposable
worktree — verified by independently re-run commands, never by the agent's own
report. [DESIGN.md](DESIGN.md) is the founding document; read it first.

Forked from [peteromallet/desloppify](https://github.com/peteromallet/desloppify)
(MIT); the mechanical detector plane descends from its Python implementation,
rewritten in Rust. That inherited plane — detectors, tree-sitter extraction,
scoring, review packets — survives here as the **evidence plane**: its findings
become snapshot-bound facts, and its score becomes one evidence stream among
several, never the objective.

## Status

The evidence plane works today (below). The transaction loop —
`explore` → `frontier` → `constitute` → `climb` — is being built in the
slices listed in DESIGN.md §9, dogfooding on this repository itself.

## Running the evidence plane

Use the repo-local launcher to guarantee you are running this checkout:

```bash
scripts/autoclimb-local scan --path /path/to/repo     # mechanical detectors
scripts/autoclimb-local status --path /path/to/repo   # score dashboard
scripts/autoclimb-local queue --path /path/to/repo    # prioritized work queue
scripts/autoclimb-local next --path /path/to/repo     # top suggestion
```

Or install a binary from this checkout:

```bash
cargo install --path crates/autoclimb-cli --force
autoclimb --help
```

Exclude vendored/generated trees before the first scan
(`scripts/autoclimb-local exclude add --path /path/to/repo vendor` etc.).
State lands in the target repo under `.autoclimb/` (local operational state;
gitignore it).

LLM review of the scanned repo (after a fresh scan):

```bash
scripts/autoclimb-local review --prepare --path /path/to/repo
scripts/autoclimb-local review --run-batches --backend codex --mode findings_only --path /path/to/repo
```

## Docs

- [DESIGN.md](DESIGN.md) — the founding design: transaction loop, minimal IR,
  ledger invariants, verifier ladder, question frontier, build plan
- [docs/USAGE.md](docs/USAGE.md) — operator guide for the evidence plane
- [docs/LLM_RUNBOOK.md](docs/LLM_RUNBOOK.md) — copy-paste runbook for
  delegating evidence-plane operation to another LLM

## Development

```bash
cargo run -p autoclimb-cli -- --help
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

`crates/` is the whole workspace: `autoclimb-types` (IR), `autoclimb-state`
(persistence + ledger), `autoclimb-cli` (binary), and the evidence-plane
crates (`-detectors`, `-treesitter`, `-graph`, `-scoring`, `-review`,
`-plan`, `-lang-*`, ...).
