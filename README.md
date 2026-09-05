# cargo-crap

A Cargo subcommand that scores each Rust function by combining cyclomatic
complexity with automated test coverage. The score is a **change-risk
signal**: it is high when a function is both hard to follow and lightly
exercised by tests. It is not a quality grade, a programmer rating, or a
management KPI.

```text
CRAP(m) = CC² × (1 − cov/100)³ + CC
```

`CC` is McCabe complexity (1 + decision points). `cov` is the percent of
instrumented lines in that function that tests hit. At 100% coverage the
score equals complexity — risk is acknowledged, not erased. At 0% coverage
the score is `CC² + CC`. A function with complexity 31 or more cannot
score 30 or below at any coverage; simplify it.

The usual gate is **30**. Use `--threshold` for a stricter line. A score
at or below the threshold does not mean simple functions should go
untested; the usual line just highlights the riskiest ones.

## Install

```bash
cargo install --path crates/cargo-crap
```

Then run it as `cargo crap` or `cargo-crap`.

## Workflow

```bash
cargo llvm-cov --lcov --output-path lcov.info
cargo crap --lcov lcov.info
```

Workspace or selected packages:

```bash
cargo llvm-cov --workspace --lcov --output-path lcov.info
cargo crap --workspace --lcov lcov.info
cargo crap -p cargo-crap --lcov lcov.info --summary
```

CI gate (exit 1 after the report if anything is over the threshold):

```bash
cargo crap --lcov lcov.info --fail-above --threshold 30
```

If a function is flagged: add automated tests when coverage is low;
extract or simplify when complexity stays high even when covered.

## Coverage needed to stay at or under 30

| Cyclomatic complexity | Coverage |
| --- | --- |
| 0–5 | 0% |
| 6–10 | ~42% |
| 11–15 | ~57% |
| 16–20 | ~71% |
| 21–25 | ~80% |
| 26–30 | 100% |
| 31+ | Refactor |

## Flags

- `--lcov <file>` — LCOV from `cargo llvm-cov` (required)
- `--path <dir>` — walk this tree (default `.`); ignored with `--workspace` / `-p`
- `--threshold <n>` — flag scores strictly above this (default `30`)
- `--workspace` — every Cargo workspace member
- `-p, --package <name>` — one member; repeatable; conflicts with `--workspace`
- `--summary` — counts and worst offender; no table
- `--fail-above` — exit 1 when any function exceeds the threshold
- `--missing` — no LCOV data, or an AST span with no instrumented lines:
  `pessimistic` (default, 0%), `optimistic` (100%), or `skip`

Exit codes: `0` finished and clean, `1` finished and the gate tripped,
`2` usage or analysis error.

## Limits

Line coverage is not proof that tests assert anything useful. Some
complex functions are legitimate. The score does not measure coupling or
cohesion. Closures count toward the enclosing function; decisions inside
unexpanded or opaque macros may be missed.

## License

[MIT](LICENSE) © 2026 Dean Grant
