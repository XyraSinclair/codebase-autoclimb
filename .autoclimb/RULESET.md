<!-- generated from ruleset.json; hash: sha256:016677d2c3ecb183f5188ae43602974081f9930304c84e4b1fd0ffc2ef53fb1f -->
<!-- NEVER HAND-EDIT: regenerate with `autoclimb constitute`. -->

# Repository RuleSet

`#[cfg(test)]` modules inside `src` files cannot be path-protected; verifier commands remain the enforcement surface for them.

```json
{
  "purpose": "Autonomous, proof-carrying hill-climb of codebases toward their clarified intent; this repository is autoclimb itself — evidence plane plus change-transaction loop, dogfooding on its own source.",
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
  "hash": "016677d2c3ecb183f5188ae43602974081f9930304c84e4b1fd0ffc2ef53fb1f"
}
```
