# Autoclimb

This repository is the active implementation of `autoclimb`.

The canonical CLI is the Rust workspace at the repository root.

Forked from peteromallet/desloppify (MIT); the mechanical detector plane descends from its Python implementation, rewritten in Rust.

## Quick Start

Use the repo-local launcher when you want to guarantee you are running this
checkout and not anything installed elsewhere on your machine. It works from
any current working directory:

```bash
scripts/autoclimb-local --help
scripts/autoclimb-local scan --path /absolute/path/to/repo
scripts/autoclimb-local queue --path /absolute/path/to/repo
scripts/autoclimb-local plan show --path /absolute/path/to/repo
```

That wrapper always goes through this checkout's Rust workspace, so it avoids
stale binaries installed elsewhere.

If you prefer an installed binary from this checkout:

```bash
cargo install --path crates/autoclimb-cli --force
autoclimb --help
```

## Read This First

- [docs/USAGE.md](docs/USAGE.md): operator guide and command workflow
- [docs/LLM_RUNBOOK.md](docs/LLM_RUNBOOK.md): copy-paste runbook for another LLM

## Safe First Pass On Another Codebase

Use the tool in this order:

```bash
scripts/autoclimb-local scan --path /path/to/repo
scripts/autoclimb-local status --path /path/to/repo
scripts/autoclimb-local queue --path /path/to/repo
scripts/autoclimb-local plan show --path /path/to/repo
scripts/autoclimb-local next --path /path/to/repo
```

Persist excludes first if the repo has vendored, generated, build, or archived
trees:

```bash
scripts/autoclimb-local exclude add --path /path/to/repo node_modules
scripts/autoclimb-local exclude add --path /path/to/repo dist
scripts/autoclimb-local exclude add --path /path/to/repo build
scripts/autoclimb-local exclude add --path /path/to/repo vendor
scripts/autoclimb-local exclude add --path /path/to/repo archive
```

Autoclimb writes state to the target repo under `.autoclimb/`. The most
important files are `.autoclimb/state.json` and `.autoclimb/config.json`.

## LLM Review

Use review only after a fresh scan.

```bash
scripts/autoclimb-local review --prepare --path /path/to/repo
scripts/autoclimb-local review --run-batches --backend codex --mode findings_only --path /path/to/repo
```

`review --run-batches` currently supports the in-process Codex runner only.
Use `--mode trusted` only when you intentionally want the batch import to apply
subjective assessments instead of findings-only diagnostics.

If you want to hand operation to another LLM, use
[docs/LLM_RUNBOOK.md](docs/LLM_RUNBOOK.md) instead of improvising the prompt.

## Common Commands

```bash
scripts/autoclimb-local show --path /path/to/repo --tier 1
scripts/autoclimb-local resolve --path /path/to/repo --status fixed finding_id_here
scripts/autoclimb-local fix --path /path/to/repo --dry-run
scripts/autoclimb-local move --path /path/to/repo --dry-run src/old.rs src/new.rs
scripts/autoclimb-local langs
```

For local development on this repository:

```bash
cargo run -p autoclimb-cli -- --help
cargo test --workspace
cargo build --release -p autoclimb-cli
```

## Repository Layout

- `crates/`: active Rust workspace
- `scripts/autoclimb-local`: local launcher that pins execution to this repo
- `docs/USAGE.md`: operator guide
- `docs/LLM_RUNBOOK.md`: copy-paste delegation runbook for other LLMs
