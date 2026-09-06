#!/usr/bin/env bash
# sessionStart: inject short workspace context (fail-open).
set -eu

cat >/dev/null || true

printf '%s\n' '{
  "additional_context": "crap-analyzer: /verify is full local CI (./scripts/check.sh, llvm-cov --fail-under-lines 100, crap-rs --fail-above --threshold strict). The stop hook runs the same gates on relevant dirty trees. Read rust-style-guide and rust-solid-design before editing Rust."
}'
exit 0
