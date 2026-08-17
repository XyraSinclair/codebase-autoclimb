<!-- generated from ruleset.json; hash: sha256:98a903281e11d9a05f861d142ced229bd89076953ebaab47f547acd3234dd15f -->
<!-- NEVER HAND-EDIT: regenerate with `autoclimb constitute`. -->

# Repository RuleSet

`#[cfg(test)]` modules inside `src` files cannot be path-protected; verifier commands remain the enforcement surface for them.

```json
{
  "purpose": "UNRATIFIED: describe what this repository is for",
  "non_goals": [],
  "allowed_paths": [
    "**"
  ],
  "protected_paths": [
    "**/*_test.*",
    "**/.github/**",
    "**/Cargo.lock",
    "**/Cargo.toml",
    "**/tests/**",
    ".autoclimb/**",
    ".github/workflows/ci.yml",
    ".github/workflows/integration.yml",
    ".github/workflows/release.yml"
  ],
  "compatibility": {},
  "verifier_commands": [
    "cargo build --workspace",
    "cargo test --workspace",
    "cargo clippy --workspace --all-targets -- -D warnings",
    "cargo fmt --all -- --check"
  ],
  "risk_ceiling": "R2",
  "budget": {
    "attempts": 2,
    "wall_secs": 1800,
    "subprocesses": 4
  },
  "hash": "98a903281e11d9a05f861d142ced229bd89076953ebaab47f547acd3234dd15f"
}
```
