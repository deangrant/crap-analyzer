#!/usr/bin/env bash
# afterFileEdit: format edited Rust files with rustfmt (fail-open).
set -eu

input=$(cat || true)
file=""
if command -v node >/dev/null 2>&1; then
  file=$(printf '%s' "$input" | node -e '
    let raw = "";
    process.stdin.on("data", (c) => (raw += c));
    process.stdin.on("end", () => {
      try {
        const p = JSON.parse(raw).file_path;
        if (typeof p === "string" && p) process.stdout.write(p);
      } catch {}
    });
  ' || true)
fi
if [ -z "${file:-}" ] && command -v python3 >/dev/null 2>&1; then
  file=$(printf '%s' "$input" | python3 -c '
import json, sys
try:
    p = json.load(sys.stdin).get("file_path") or ""
    if isinstance(p, str):
        sys.stdout.write(p)
except Exception:
    pass
' || true)
fi

[ -z "${file:-}" ] && exit 0
[ ! -f "$file" ] && exit 0
case "$file" in
  *.rs) ;;
  *) exit 0 ;;
esac

rustfmt_bin=""
if command -v rustfmt >/dev/null 2>&1; then
  rustfmt_bin=$(command -v rustfmt)
elif command -v rustup >/dev/null 2>&1; then
  rustfmt_bin=$(rustup which rustfmt 2>/dev/null || true)
elif [ -x "${HOME}/.cargo/bin/rustfmt" ]; then
  rustfmt_bin="${HOME}/.cargo/bin/rustfmt"
fi

if [ -n "$rustfmt_bin" ]; then
  "$rustfmt_bin" "$file" || true
fi
exit 0
