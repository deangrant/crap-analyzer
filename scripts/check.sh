#!/usr/bin/env bash
# Mirror CI: workspace, conventions, fmt, clippy, deny, audit, test, rustdoc.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

count="$(.github/scripts/package-count.sh)"
if [ "${count}" = "0" ]; then
    echo "No workspace packages; nothing to check."
    exit 0
fi

python3 .github/scripts/check-workspace.py

over="$(find . -name '*.rs' ! -path '*/target/*' ! -path '*/.git/*' \
    -print0 | xargs -0 wc -l \
    | awk '$NF != "total" && $1 > 500 { print $1 " " $2 }')"
if [ -n "${over}" ]; then
    echo "These files exceed the 500-line cap:"
    echo "${over}"
    exit 1
fi

locked=()
if [ -f Cargo.lock ]; then
    locked+=(--locked)
fi

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features "${locked[@]}" -- -D warnings
cargo deny check
cargo audit
cargo test --workspace --all-targets --all-features "${locked[@]}"
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps "${locked[@]}"
