# Verify

Read and follow the **verify** skill at
[`.agents/skills/verify/SKILL.md`](../skills/verify/SKILL.md).

Run the full local vs CI procedure from the workspace root: `./scripts/check.sh`,
then `cargo llvm-cov` with `--fail-under-lines 100`, then `crap-rs` with
`--fail-above --threshold strict`. Fix failures and loop until all three
exit 0. Do not weaken the gates.
