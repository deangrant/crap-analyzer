---
name: verify
description: >-
  Run full local CI parity: scripts/check.sh, llvm-cov 100% lines, then
  crap-rs --fail-above --threshold strict. Use when the user runs /verify,
  asks to match CI locally, or when the stop hook re-enters after a red gate.
trigger: >-
  verify, local CI, pipeline parity, stop hook, llvm-cov, fail-under-lines,
  dogfood, check.sh
---

# Verify (local vs CI)

Read this skill before running `/verify` or fixing a stop-hook follow-up.
The stop hook runs [`.agents/hooks/run-verify.sh`](../../hooks/run-verify.sh),
which is this same procedure.

For threshold and band semantics, also read
[crap-scoring](../crap-scoring/SKILL.md).

## Procedure

From the workspace root, in order. Stop at the first failure; fix it; restart
from the failed step (or the top).

1. Pipeline (fmt, Clippy, deny, audit, test, rustdoc, workspace, 500-line cap):

   ```bash
   ./scripts/check.sh
   ```

2. Line coverage (needs `cargo-llvm-cov` and `llvm-tools-preview`):

   ```bash
   cargo llvm-cov --workspace --all-features --locked \
     --lcov --output-path lcov.info \
     --fail-under-lines 100
   ```

3. CRAP dogfood (`strict` is 8):

   ```bash
   cargo run -p crap-rs --locked -- \
     --coverage lcov.info --path . --workspace --all-features \
     --fail-above --threshold strict
   ```

## Rules

- Do not claim the change is done until all three steps exit 0.
- Do not skip coverage because `check.sh` is green.
- Do not change `--fail-under-lines` or `--threshold` to pass.
- If llvm-cov is missing, install it; do not invent a weaker substitute.
- After a stop-hook follow-up, fix the cited failures, then stop again so
  the hook re-runs this procedure.
