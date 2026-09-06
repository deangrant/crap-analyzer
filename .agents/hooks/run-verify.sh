#!/usr/bin/env bash
# Same three gates as .agents/skills/verify/SKILL.md.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

./scripts/check.sh

locked=()
if [ -f Cargo.lock ]; then
  locked+=(--locked)
fi

cargo llvm-cov --workspace --all-features "${locked[@]}" \
  --lcov --output-path lcov.info \
  --fail-under-lines 100

cargo run -p crap-rs "${locked[@]}" -- \
  --lcov lcov.info --path . --workspace \
  --fail-above --threshold strict
