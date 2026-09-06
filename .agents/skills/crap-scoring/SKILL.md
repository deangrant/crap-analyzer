---
name: crap-scoring
description: >-
  CRAP formula, risk bands vs the pass/fail gate, named thresholds, --missing
  policy, and the CI dogfood contract. Use when changing scoring, flags,
  reports, coverage join, or the coverage workflow.
trigger: >-
  CRAP, dogfood, threshold, strict, lenient, risk band, --fail-above,
  --missing, classify_risk
---

# CRAP scoring

Product overview: [README.md](../../../README.md). Verify loop:
[verify](../verify/SKILL.md).

## Formula

```text
CRAP(m) = CC² × (1 − cov/100)³ + CC
```

`CC` is cyclomatic (default) or cognitive (`--metric cognitive`). `cov` is
the percent of instrumented lines in the function that tests hit. At 100%
coverage the score equals complexity. At 0% it is `CC² + CC`.

## Bands vs gate

These are two axes. Do not conflate them.

| Axis | Meaning | Values |
| ---- | ------- | ------ |
| Risk band | Fixed `classify_risk` label | Low ≤ 8, Acceptable ≤ 15, Moderate ≤ 25, High > 25 |
| Gate | `--fail-above` trip line | Default 15; `strict` = 8; `lenient` = 25; or any ≥ 0 number |

A function **exceeds** when its score is **strictly above** the active
threshold. Moderate (score 20) can still pass a lenient gate (25).

## `--missing`

No LCOV data, an empty instrumented span, or an unresolved path tie:

- `pessimistic` (default) — treat as 0%
- `optimistic` — treat as 100%
- `skip` — drop the row

A package name (`--workspace` / `-p`) breaks equal `src/lib.rs` suffix ties.

## CI / dogfood contract

[`.github/workflows/coverage.yml`](../../../.github/workflows/coverage.yml):

1. `cargo llvm-cov … --fail-under-lines 100`
2. `crap-rs --lcov lcov.info --path . --workspace --fail-above --threshold strict`

Do not weaken those flags to green a job. Remediate coverage or complexity.
