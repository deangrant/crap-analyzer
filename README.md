# crap-analyzer

crap-analyzer scores each Rust function by combining complexity with LCOV
line coverage. The Change Risk Anti-Patterns (CRAP) score is a
**change-risk signal**: it rises when a function is hard to follow and
lightly exercised by tests. It is not a quality grade, a programmer
rating, or a management KPI.

You produce LCOV with `cargo llvm-cov`. The `crap-rs` CLI reads that file
and your sources, then prints a table or a JSON report.

## Score

```text
CRAP(m) = CC² × (1 − cov/100)³ + CC
```

`CC` is the selected complexity metric. The default is McCabe cyclomatic
complexity (1 + decision points). `--metric cognitive` uses nesting-weighted
cognitive complexity instead. `cov` is the percent of instrumented lines
in that function that tests hit.

At 100% coverage the score equals complexity. Risk is acknowledged, not
erased. At 0% coverage the score is `CC² + CC`. A cyclomatic value of 16
or more cannot stay at or under the default gate of 15 at any coverage;
simplify that function.

### Bands and the gate

These are two axes. Do not treat one as the other.

| Axis | Meaning | Values |
| ---- | ------- | ------ |
| Risk band | Fixed `classify_risk` label | Low ≤ 8, Acceptable ≤ 15, Moderate ≤ 25, High > 25 |
| Gate | `--fail-above` trip line | Default 15; `strict` = 8; `lenient` = 25; or any number ≥ 0 |

A function **exceeds** when its score is **strictly above** the active
threshold. A Moderate function (score 20) still passes a lenient gate
(25). Never read “risk level: Moderate” as “exceeds threshold.”

The shared 8 / 15 / 25 numbers are a calibration convention. They are not
empirically derived cutoffs.

### Coverage needed to stay at or under 15 (cyclomatic)

This table uses the **default gate of 15**. `--threshold strict` is 8;
`--threshold lenient` is 25. The formula is the source of truth.

| Cyclomatic complexity | Coverage |
| --------------------- | -------- |
| 1–3 | 0% |
| 4–6 | ~12–37% |
| 7–9 | ~45–58% |
| 10–12 | ~63–73% |
| 13–15 | ~77–100% |
| 16+ | Refactor |

Each range is the coverage needed at the low and high `CC` of that band.

If a function is flagged: add automated tests when coverage is below 90%.
Extract or simplify when coverage is 90% or more and complexity still
keeps the score over the threshold.

## Requirements

- Rust toolchain **1.94.0** ([`rust-toolchain.toml`](rust-toolchain.toml))
- [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) to produce LCOV

Coverage input is **LCOV only**. Convert other formats first. Do not add a
second parser.

## Install

```bash
cargo install --path crates/crap-rs
```

## Usage

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

CI gate (exit 1 after the report if any function exceeds the threshold):

```bash
crap-rs --lcov lcov.info --fail-above
crap-rs --lcov lcov.info --fail-above --threshold 30
```

This repository runs the same gate in CI at `--threshold strict`
(see [`.github/workflows/coverage.yml`](.github/workflows/coverage.yml)).

Pass the same `--features`, `--all-features`, and `--no-default-features`
flags you used for `cargo llvm-cov`. Feature-gated items are skipped
unless those features are enabled.

## Flags

| Flag | Role |
| ---- | ---- |
| `--lcov <file>` | LCOV from `cargo llvm-cov` (required). Must contain at least one `DA:` line-hit record. |
| `--path <dir>` | Walk this tree (default `.`). A workspace root is analyzed per member. A member package root is that package only; `-p` is not required. |
| `--metric` | `cyclomatic` (default) or `cognitive` |
| `--threshold` | Flag scores strictly above this. Number, `strict` (8), or `lenient` (25). Default `15` for both metrics. Independent of the risk band. |
| `--format` | `text` (default table) or `json` (versioned envelope, `schema_version` 1) |
| `--workspace` | Every Cargo workspace member |
| `-p, --package <name>` | One member; repeatable; conflicts with `--workspace` |
| `--summary` | Counts and worst offender; text only; no table |
| `--fail-above` | Exit 1 when any function exceeds the threshold (not the risk band) |
| `--missing` | No LCOV data, an empty span, or an unresolved path tie: `pessimistic` (default, 0%), `optimistic` (100%), or `skip`. A package name (`--workspace` / `-p`) breaks equal `src/lib.rs` suffix ties. |
| `--features`, `--all-features`, `--no-default-features` | Same feature universe as the `cargo llvm-cov` run that produced the LCOV file |

## Exit codes

| Code | Meaning |
| ---- | ------- |
| `0` | Analysis finished; the gate did not trip |
| `1` | Analysis finished; `--fail-above` tripped |
| `2` | Usage, I/O, metadata, or collect error. Any unparseable source file is a collect error. |

## Crates

| Crate | Role |
| ----- | ---- |
| [`crap-core`](crates/crap-core) | Language-agnostic LCOV parse, path join, score, risk, and report |
| [`crap-rs`](crates/crap-rs) | Rust discovery, complexity, and the `crap-rs` CLI |

`crap-rs` implements `Language` and calls `crap_core::run`. A later
language crate would do the same. That crate is not in this workspace.

Module maps and pipeline: [`.agents/docs/ARCHITECTURE.md`](.agents/docs/ARCHITECTURE.md).

## Metric rules

| Topic | Cyclomatic | Cognitive |
| ----- | ---------- | --------- |
| Base | 1 + each decision point | Nesting-weighted increments |
| `match` | Each arm adds 1, including `_` | One increment for the whole `match` |
| `let … else` | +1 | Scored like `if` / `else` |
| Match guard | +1 | Flat +1 |
| `?` | +1 (error / early-return branch) | Free |
| Closures | Count toward the enclosing function | Same |
| Nested `fn` | Separate row; join excludes the inner span from the outer coverage | Same |
| Recursion | Not a decision point | Direct recursion does not add |
| `else` / `else if` | Part of the `if` | Flat +1 |
| Boolean operators | Each short-circuit `and` / `or` is a decision | A run of the same operator counts once |
| Labeled `break` / `continue` | Not extra | +1 |
| `async` / `try` blocks | Do not add; inner decisions still count | Same |

Decisions inside unexpanded or opaque macros may be missed.

## Limits

- Line coverage is not proof that tests assert anything useful.
- Some complex functions are legitimate. The score does not measure
  coupling or cohesion.
- Trait default methods are omitted (llvm-cov often has no line hits).
- Unknown `#[cfg]` predicates skip the item. Skipped items are not gated.
- `#[cfg]` uses the host (`target_os`, `target_arch`, `target_family`,
  `target_pointer_width`, `unix` / `windows`). Cross-compile LCOV can
  disagree; there is no `--target` flag.
- File and directory symlinks are followed only when the target stays
  under the walk root; cycles are skipped.
- `$CARGO` is used only when it names an existing file (Cargo’s usual
  override); otherwise `crap-rs` runs `cargo` from `PATH`.

## Develop

Lean pipeline (fmt, Clippy, deny, audit, test, rustdoc):

```bash
./scripts/check.sh
```

Full local vs CI (also 100% lines and CRAP `--threshold strict`):
[`.agents/skills/verify/SKILL.md`](.agents/skills/verify/SKILL.md), or
`/verify`.

## Agents and docs

| Doc | Purpose |
| --- | ------- |
| [`AGENTS.md`](AGENTS.md) | Contributor guidance, skills, and commands |
| [`.agents/docs/ARCHITECTURE.md`](.agents/docs/ARCHITECTURE.md) | Crate boundaries and analysis pipeline |
| [DeepWiki](https://deepwiki.com/deangrant/crap-analyzer) | Indexed project wiki |

## License

[MIT](LICENSE) © 2026 Dean Grant
