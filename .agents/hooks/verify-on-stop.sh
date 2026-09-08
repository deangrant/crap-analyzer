#!/usr/bin/env bash
# stop: full verify on relevant dirty trees; follow up on failure (fail-open).
set -eu

allow_stop() {
  printf '%s\n' '{}'
  exit 0
}

input=$(cat || true)

status=""
loop_count="0"
if command -v python3 >/dev/null 2>&1; then
  parsed=$(printf '%s' "$input" | python3 -c '
import json, sys
try:
    p = json.load(sys.stdin)
except Exception:
    print("\t0")
    raise SystemExit(0)
status = p.get("status") or ""
loop = p.get("loop_count")
if loop is None:
    loop = 0
print(f"{status}\t{loop}")
' || true)
  status=$(printf '%s' "$parsed" | cut -f1)
  loop_count=$(printf '%s' "$parsed" | cut -f2)
fi

if [ "${status}" != "completed" ]; then
  allow_stop
fi

case "${loop_count}" in
  ''|*[!0-9]*) loop_count=0 ;;
esac
if [ "${loop_count}" -ge 3 ]; then
  allow_stop
fi

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

relevant=0
if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  while IFS= read -r line; do
    [ -z "${line}" ] && continue
    path=${line:3}
    path=${path##* -> }
    case "${path}" in
      *.rs|Cargo.toml|Cargo.lock|clippy.toml|deny.toml|rustfmt.toml|scripts/check.sh)
        relevant=1
        break
        ;;
      .github/workflows/*|scripts/githooks/*)
        relevant=1
        break
        ;;
    esac
  done <<EOF
$(git status --porcelain 2>/dev/null || true)
EOF
else
  relevant=1
fi

if [ "${relevant}" -eq 0 ]; then
  allow_stop
fi

tmp=$(mktemp)
set +e
bash "${root}/.agents/hooks/run-verify.sh" >"${tmp}" 2>&1
code=$?
set -e

if [ "${code}" -eq 0 ]; then
  rm -f "${tmp}"
  allow_stop
fi

body=$(tail -c 8192 "${tmp}" 2>/dev/null || true)
rm -f "${tmp}"

if command -v python3 >/dev/null 2>&1; then
  VERIFY_LOG="${body}" python3 -c '
import json, os
log = os.environ.get("VERIFY_LOG", "")
msg = (
    "Local verify failed (./scripts/check.sh, llvm-cov --fail-under-lines 100, "
    "crap-rs --all-features --fail-above --threshold strict). Fix the failures, "
    "re-run the same gates, then stop.\n\n"
    + log
)
print(json.dumps({"followup_message": msg}))
'
else
  printf '%s\n' '{"followup_message":"Local verify failed. Fix the failures, re-run /verify, then stop."}'
fi
exit 0
