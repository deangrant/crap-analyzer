# crap-analyzer

A virtual workspace that scores each function by combining complexity with
automated test coverage. The score is a **change-risk signal**: it is high
when a function is both hard to follow and lightly exercised by tests. It
is not a quality grade, a programmer rating, or a management KPI.

```text
CRAP(m) = CC² × (1 − cov/100)³ + CC
```

`CC` is the selected complexity metric. The default is McCabe cyclomatic
complexity (1 + decision points). `--metric cognitive` uses nesting-weighted
cognitive complexity instead. `cov` is the percent of instrumented lines
in that function that tests hit. At 100% coverage the score equals
complexity — risk is acknowledged, not erased. At 0% coverage the score
is `CC² + CC`. A cyclomatic value of 31 or more cannot score 30 or below
at any coverage; simplify it.

The default **gate** is **15** for both metrics. Named presets sit on
the band boundaries: `--threshold strict` is 8 and `--threshold lenient`
is 25. Use `--threshold <n>` for any other non-negative number. A score
at or below the gate does not mean simple functions should go untested;
the line just highlights the ones that exceed the active threshold.

**Bands and the gate are two axes.** Risk bands (Low ≤ 8, Acceptable ≤
15, Moderate ≤ 25, High > 25) classify the score via a fixed
`classify_risk` mapping. They never change. The gate is a separate
pass/fail line: a function exceeds when its score is strictly above the
active threshold. The shared 8 / 15 / 25 numbers are a calibration
convention, not empirically derived values. A function can sit in
Moderate (score 20) and still pass a lenient gate (threshold 25). Never
read “risk level: Moderate” as “exceeds threshold,” or vice versa.

## Crates

- [`crap-core`](crates/crap-core) — scoring, LCOV parse, join, and report
- [`crap-rs`](crates/crap-rs) — Rust discovery, complexity, and the `crap-rs` CLI

## Install

```bash
cargo install --path crates/crap-rs
```

## Workflow

```bash
cargo llvm-cov --lcov --output-path lcov.info
crap-rs --lcov lcov.info
```

Workspace or selected packages:

```bash
cargo llvm-cov --workspace --lcov --output-path lcov.info
crap-rs --workspace --lcov lcov.info
crap-rs -p crap-rs --lcov lcov.info --summary
```

CI gate (exit 1 after the report if anything is over the threshold):

```bash
crap-rs --lcov lcov.info --fail-above
crap-rs --lcov lcov.info --fail-above --threshold 30
```

If a function is flagged: add automated tests when coverage is below
90%; extract or simplify when coverage is 90% or more and complexity
still keeps the score over the threshold.

## Coverage needed to stay at or under 30 (cyclomatic)

This table is an example at a **gate of 30**, not the default. The default
gate is 15; `strict` is 8 and `lenient` is 25.

| Cyclomatic complexity | Coverage |
| --- | --- |
| 1–5 | 0% |
| 6–10 | ~13–42% |
| 11–15 | ~46–59% |
| 16–20 | ~62–71% |
| 21–25 | ~73–80% |
| 26–30 | ~82–100% |
| 31+ | Refactor |

Each range is the coverage needed at the low and high CC of the band.
The formula is the source of truth.

## Flags

- `--lcov <file>` — LCOV from `cargo llvm-cov` (required); must contain at
  least one `DA:` line-hit record
- `--path <dir>` — walk this tree (default `.`). A `Cargo.toml` workspace
  is analyzed per member (same isolation as `--workspace`)
- `--metric` — `cyclomatic` (default) or `cognitive`
- `--threshold` — flag scores strictly above this. Number, `strict` (8),
  or `lenient` (25). Default `15` for both metrics. Independent of the
  risk band.
- `--format` — `text` (default table) or `json` (versioned envelope)
- `--workspace` — every Cargo workspace member
- `-p, --package <name>` — one member; repeatable; conflicts with `--workspace`
- `--summary` — counts and worst offender; text only; no table
- `--fail-above` — exit 1 when any function exceeds the threshold (not
  the risk band)
- `--missing` — no LCOV data, an empty span, or an unresolved path tie:
  `pessimistic` (default, 0%), `optimistic` (100%), or `skip`. A package
  name (`--workspace` / `-p`) breaks equal `src/lib.rs` suffix ties
- `--features`, `--all-features`, `--no-default-features` — same feature
  universe as the `cargo llvm-cov` run that produced the LCOV file

Exit codes: `0` finished and clean, `1` finished and the gate tripped,
`2` usage or analysis error (including when every source file fails to parse).

## Architecture

Language-agnostic work lives in `crap-core`: LCOV parse, join, score, and
the table. A frontend implements `Language` (`resolve_targets` and
`collect_functions`) and calls `crap_core::run`. Today that frontend is
`crap-rs`. A later `crap-go` or `crap-ts` crate would depend on
`crap-core`, implement the same trait, and ship its own binary.

Coverage input stays **LCOV**. Other tools should convert first
(`gocov`, `c8 --reporter=lcov`) rather than adding a second parser.

A later `crap` meta-binary could dispatch on `--lang`; it is not part of
this workspace yet.

## Limits

- Line coverage is not proof that tests assert anything useful.
- Some complex functions are legitimate. The score does not measure
  coupling or cohesion.
- Decisions inside unexpanded or opaque macros may be missed.
- Cognitive does not add for direct recursion.
- Closures count toward the enclosing function.
- Async and `try` blocks do not add; inner decisions still count.
- Cyclomatic: each match arm adds 1, including `_` and other catch-alls;
  `let … else` and each match guard add 1.
- Cognitive: nesting-weighted increments; a `match` is one increment
  (not per arm); `let … else` is scored like `if` / `else`; each match
  guard is flat +1; `else` / `else if` are flat +1; a run of the same
  boolean operator counts once; labeled `break` / `continue` add 1; `?`
  is free.
- Nested function coverage excludes the inner span.
- Feature-gated items are skipped unless those features are enabled
  (pass the same `--features` flags used for `cargo llvm-cov`).
- File and directory symlinks are followed; cycles are skipped.
- `$CARGO` is used only when it names an existing file (Cargo’s usual
  override); otherwise `crap-rs` runs `cargo` from `PATH`.

## License

[MIT](LICENSE) © 2026 Dean Grant
