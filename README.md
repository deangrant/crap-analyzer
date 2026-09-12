# crap-score

crap-score scores each function by combining complexity with line
coverage. The Change Risk Anti-Patterns (CRAP) score is a
**change-risk signal**: it rises when a function is hard to follow and
lightly exercised by tests. It is not a quality grade, a programmer
rating, or a management KPI.

For Rust, produce LCOV with `cargo llvm-cov` and run `crap-rs`. For Go,
produce a coverprofile with `go test -coverprofile` and run `crap-go`.
For TypeScript, produce LCOV with vitest, c8, jest, or nyc and run
`crap-ts`. For Python, produce LCOV with coverage.py and run `crap-py`.
Frontends share scoring, risk bands, and the pass/fail gate in
`crap-core`.

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
- For Rust analysis: [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) to produce LCOV
- For Go analysis: a Go toolchain only to generate `cover.out` (`go test`);
  `crap-go` itself is a Rust binary and does not invoke `go`
- For TypeScript analysis: a JS test runner that emits LCOV; `crap-ts` is a
  Rust binary and does not invoke Node
- For Python analysis: coverage.py (or another tool) that emits LCOV;
  `crap-py` is a Rust binary and does not invoke Python

`crap-core` parses **LCOV only** on disk. `crap-go` parses coverprofile in
the frontend and passes `FileCoverage` into `run_with_coverage`. Do not add
a second on-disk parser in core.

## Install

```bash
cargo install --path crates/crap-rs
cargo install --path crates/crap-go
cargo install --path crates/crap-ts
cargo install --path crates/crap-py
```

## Usage

### Rust (`crap-rs`)

```bash
cargo llvm-cov --lcov --output-path lcov.info
crap-rs
```

Workspace or selected packages:

```bash
cargo llvm-cov --workspace --lcov --output-path lcov.info
crap-rs --workspace
crap-rs -p crap-rs --summary
```

CI gate (exit 1 after the report if any function exceeds the threshold):

```bash
crap-rs --fail-above
crap-rs --fail-above --threshold 30
```

This repository runs the same gate in CI at `--threshold strict`
(see [`.github/workflows/coverage.yml`](.github/workflows/coverage.yml)).

Pass the same `--features`, `--all-features`, and `--no-default-features`
flags you used for `cargo llvm-cov`. Feature-gated items are skipped
unless those features are enabled.

### Go (`crap-go`)

```bash
go test -coverprofile=cover.out ./...
crap-go --workspace
```

`--coverage` defaults to `cover.out`. Scoring, bands, `--missing`, and
`--fail-above` match `crap-rs`. Use `--tags` for custom build tags treated
as enabled. No Go helper or tree-sitter; the analyzer walks `go.mod`
packages and a custom Rust visitor.

### TypeScript (`crap-ts`)

```bash
# e.g. vitest / c8 / jest with an lcov reporter → lcov.info
crap-ts --path . --coverage lcov.info
crap-ts --workspace --fail-above --threshold strict
```

`--coverage` defaults to `lcov.info`. LCOV `SF:` paths must resolve to
`.ts` / `.tsx` sources (enable source-map remapping when covering emitted
JavaScript). `--workspace` expands npm/yarn `workspaces` globs (`*`, `**`,
`!`) or, when those are absent, `pnpm-workspace.yaml` `packages`; without
`--workspace`, a `package.json` root is that package only. `.ts` / `.tsx`
only; hand-rolled Rust visitor (no Node, no tree-sitter).

### Python (`crap-py`)

```bash
coverage run -m pytest
coverage lcov -o lcov.info
crap-py --path . --coverage lcov.info
crap-py --workspace --fail-above --threshold strict
```

`--coverage` defaults to `lcov.info`. LCOV `SF:` paths must resolve to
`.py` sources. `--workspace` expands `[tool.uv.workspace].members` globs
(`*`, `**`, `!`); without `--workspace`, a `pyproject.toml` root is that
project only. `.py` only (no `.pyi`); hand-rolled indent-aware Rust
visitor (no Python runtime, no tree-sitter).

## Flags

Shared flags work the same on the CLIs unless noted.

| Flag | Role |
| ---- | ---- |
| `--coverage <file>` | Coverage file. `crap-rs` / `crap-ts` / `crap-py`: LCOV (default `lcov.info`; `crap-rs` alias `--lcov`); must contain at least one `DA:` line-hit record. `crap-go`: coverprofile (default `cover.out`); must contain a `mode:` line and at least one data line. |
| `--path <dir>` | Walk this tree (default `.`). Rust: a workspace root is analyzed per member; a member package root is that package only. Go: a module root (`go.mod`) is analyzed per package. TypeScript: a `package.json` root is that package only unless `--workspace` / `-p`. Python: a `pyproject.toml` root is that project only unless `--workspace` / `-p`. |
| `--metric` | `cyclomatic` (default) or `cognitive` |
| `--threshold` | Flag scores strictly above this. Number, `strict` (8), or `lenient` (25). Default `15` for both metrics. Independent of the risk band. |
| `--format` | `text` (default table) or `json` (versioned envelope, `schema_version` 4). `result.threshold_cleared` is true when no function exceeds `--threshold` (renamed from `passed` in schema 3); omitting `--fail-above` still reports `threshold_cleared` / per-function `exceeds` from the threshold, but `result.gate_failed` stays false and the process exits 0. `result.gate_failed` / exit 1 require `--fail-above`. Per-function `coverage_join` is `measured`, `missing`, or `ambiguous`; unresolved path ties also appear in `result.summary.ambiguous` and a stderr / text-footer warning. |
| `--workspace` | Every workspace/module member |
| `-p, --package <name>` | One member; repeatable; conflicts with `--workspace`. Go: import path. TypeScript: package `name`. Python: project name from `pyproject.toml`. |
| `--summary` | Counts and worst offender; text only; conflicts with `--format json` |
| `--fail-above` | Exit 1 when any function exceeds the threshold (not the risk band) |
| `--missing` | No coverage data, an empty span, or an unresolved path tie: `pessimistic` (default, 0%), `optimistic` (100%), or `skip`. Package names strengthen path ranking and break equal basename ties (Cargo/TS `{name}/src` or `lib/…` or a unique path component; Go import-path path suffix). Leftover ties stay scored via this policy but are labeled `ambiguous`. |
| `--features`, `--all-features`, `--no-default-features` | `crap-rs` only: same feature universe as the `cargo llvm-cov` run that produced the LCOV file |
| `--tags <list>` | `crap-go` only: build tags treated as enabled (comma-separated) |

## Exit codes

| Code | Meaning |
| ---- | ------- |
| `0` | Analysis finished; the gate did not trip |
| `1` | Analysis finished; `--fail-above` tripped |
| `2` | Usage, I/O, metadata, collect, or report error. Unreadable or structurally invalid source is a collect error; JSON serialize failures after scoring are a report error. |

## Crates

| Crate | Role |
| ----- | ---- |
| [`crap-core`](crates/crap-core) | Language-agnostic LCOV parse, path join, score, risk, and report |
| [`crap-rs`](crates/crap-rs) | Rust discovery, complexity, and the `crap-rs` CLI |
| [`crap-go`](crates/crap-go) | Go discovery, coverprofile, complexity, and the `crap-go` CLI |
| [`crap-ts`](crates/crap-ts) | TypeScript discovery, LCOV, complexity, and the `crap-ts` CLI |
| [`crap-py`](crates/crap-py) | Python discovery, LCOV, complexity, and the `crap-py` CLI |

`crap-rs`, `crap-go`, `crap-ts`, and `crap-py` implement `Language`. Rust,
TypeScript, and Python call `crap_core::run` (LCOV on disk). Go parses
coverprofile and calls `run_with_coverage`.

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
- Trait default methods are omitted even when LCOV has line hits for the
  trait item (llvm-cov often has no usable span; the visitor never emits
  them).
- Harness attrs whose last path segment is `test` or `bench` (including
  `#[tokio::test]`) are skipped. Inline `mod tests` / `mod test` helpers are
  also omitted (same intent as skipping `*_tests.rs` / `tests.rs` filenames).
  Criterion and custom libtest harnesses are not detected; other ungated
  helper modules (for example `mod helpers`) can still be scored.
- One unreadable or unparseable source file fails the whole collect (exit
  2), same fail-closed contract for Rust, Go, and TypeScript.
- Nested coverage exclude cost grows with functions and instrumented lines
  per file; extreme per-file function counts may be slow.
- The full LCOV or coverprofile is held in memory for join (Go expands
  coverprofile blocks across every line); large monorepo coverage files
  can use substantial RAM. Text and JSON reports stream to stdout (one
  function at a time for JSON); scored entries still live in memory.
- File and directory symlinks are followed only when the target stays
  under the walk root; cycles are skipped. The walk root must
  canonicalize (otherwise the run fails); files that fail canonicalize
  are skipped. That policy is intentional and tested: residual risk is
  local filesystem analysis under `--path`, not a sandbox — the same
  trust class as the host tree itself.
- Unknown `#[cfg]` predicates skip the item. Skipped items are not gated.
- `#[cfg]` uses the host (`target_os` / `target_arch` via
  `std::env::consts`, plus `target_family`, `target_pointer_width`,
  `unix` / `windows`, `debug_assertions`). Unlisted rustc cfg keys still
  skip. Cross-compile LCOV can disagree; there is no `--target` flag.
  Local `/verify` and CI dogfood pair `cargo llvm-cov --all-features`
  with `crap-rs --all-features` so the feature universe matches.
- `$CARGO` is used only when it names an existing file (Cargo’s usual
  override); otherwise `crap-rs` runs `cargo` from `PATH`. That override may
  be any executable path — intentional for Cargo toolchains, accepted under
  **local trust**, not a sandbox. A hostile `$CARGO` can run arbitrary code
  at `cargo metadata` time (same class as trusting the build host).
- `crap-go` complexity is an approximate text scanner (not `go/ast`) —
  an accepted product Limit. Scores can diverge from `go/ast`-based
  tools; treat them as a change-risk signal, not an authoritative
  complexity audit. Unclosed literals/comments or unbalanced
  `{}`/`()`/`[]` fail the run. Structural balance is not language
  validity: nonsense tokens with balanced braces still collect.
  Valid but unusual syntax (generics edge cases) may still under-count.
  Coverprofile shared-line statements are
  merged pessimistically (any zero-hit block on a line leaves that line
  uncovered). Nested `go.mod` modules are remapped and analyzed with the
  parent module. Host build tags cover common GOOS names (`linux`,
  `windows`, `darwin`, BSD family, …); other OS names still need
  `--tags`.
- `crap-ts` complexity is an approximate text scanner (not `tsc`) — an
  accepted product Limit. Scores can diverge from `tsc`-based tools
  (under- or over-count on unusual syntax); treat them as a change-risk
  signal, not an authoritative complexity audit. Unclosed
  literals/comments or unbalanced `{}`/`()`/`[]` fail the run.
  Structural balance is not language validity: nonsense tokens with
  balanced braces still collect. Valid but unusual syntax (generics,
  decorators, TSX) may under- or over-count. `package.json` is strict
  JSON via `serde_json` (JSONC comments/trailing commas/duplicate fields
  are unsupported);
  `pnpm-workspace.yaml` entries strip outer quotes only (no escape
  decode). Cover `.ts` / `.tsx` paths in LCOV (source-map remapping when
  needed). Path join uses the package directory relative to the
  workspace (`join_key`); reports still show the npm package name.
- `crap-py` complexity is an approximate indent-aware text scanner (not
  the CPython AST) — an accepted product Limit. Scores can diverge from
  AST-based tools; treat them as a change-risk signal, not an
  authoritative complexity audit. Unclosed literals/comments or
  unbalanced `{}`/`()`/`[]` fail the run. Structural balance is not
  language validity. Valid but unusual syntax (line continuations,
  exotic f-strings) may under- or over-count. `pyproject.toml` names
  come from `[project].name` or `[tool.poetry].name`. `--workspace`
  expands `[tool.uv.workspace].members` only. Path join uses the
  project directory relative to the workspace (`join_key`); reports
  still show the project name.

## Develop

Lean pipeline (fmt, Clippy, deny, audit, test, rustdoc):

```bash
./scripts/check.sh
```

Full local vs CI (also 100% lines and CRAP `--threshold strict`):
[`.agents/skills/verify/SKILL.md`](.agents/skills/verify/SKILL.md), or
`/verify`.

Fuzz smoke (`fuzz/` + [`.github/workflows/fuzz.yml`](.github/workflows/fuzz.yml))
uses nightly `cargo-fuzz` and is **outside** the workspace `/verify` loop:

```bash
cargo +nightly fuzz run lcov_parse -- -max_total_time=30
```

JSON `result.threshold_cleared` reflects threshold breaches even without
`--fail-above`; use `--fail-above` when you need exit 1 /
`result.gate_failed`.

## Agents and docs

| Doc | Purpose |
| --- | ------- |
| [`AGENTS.md`](AGENTS.md) | Contributor guidance, skills, and commands |
| [`.agents/docs/ARCHITECTURE.md`](.agents/docs/ARCHITECTURE.md) | Crate boundaries and analysis pipeline |
| [DeepWiki](https://deepwiki.com/deangrant/crap-score) | Indexed project wiki |

## License

[MIT](LICENSE) © 2026 Dean Grant
