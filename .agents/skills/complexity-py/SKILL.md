---
name: complexity-py
description: >-
  Python LCOV join, pyproject.toml / uv workspace walk, and the hand-rolled
  indent-aware complexity visitor for crap-py. Use when editing crap-py
  complexity, walk, or project resolve.
trigger: >-
  crap-py, python, pyproject.toml, uv workspace, Python visitor, LCOV
---

# Python complexity and LCOV

Read before editing `crates/crap-py/**`. Scoring bands and gate:
[crap-scoring](../crap-scoring/SKILL.md). Shared join / `--missing`:
[complexity-lcov-join](../complexity-lcov-join/SKILL.md). Rust style and
SOLID: [rust-style-guide](../rust-style-guide/SKILL.md) and
[rust-solid-design](../rust-solid-design/SKILL.md).

## Boundaries

- `crap-py` is Rust-only. Do not add a Python helper, tree-sitter, or
  rustpython.
- Coverage is **LCOV on disk** via `crap_core::run`. Do **not** add a
  `.coverage` / coverage.py JSON parser to `crap-core` or the frontend
  for v1.
- Render language string is `"python"`.
- Sources are `.py` only (skip `.pyi`). No notebooks in v1.

## Coverage

- `--coverage` defaults to `lcov.info`.
- Users produce LCOV with coverage.py (`coverage lcov -o lcov.info`).
- LCOV `SF:` paths must resolve to `.py` sources.
- Empty spans and missing path joins use core `--missing`.
- Unresolved equal-rank ties are `coverage_join: ambiguous` (still
  scored via `--missing`); PathIndex uses `join_key` (project directory
  relative to the workspace) when set, else the project `name`.
  Reports still show the project `name` as `crate_name`.

## Project walk

- Resolve projects from `pyproject.toml` (`project_resolve`).
- Name from `[project].name` (PEP 621); fall back to
  `[tool.poetry].name`.
- Without `--workspace` / `-p`, a `pyproject.toml` root is that project
  only (uv workspaces are **not** expanded).
- `--workspace` expands `[tool.uv.workspace].members` globs (`*`, `**`,
  exact paths, and `!` exclusions). If there is no members table,
  returns the root project only.
- `-p` selects by project name.
- Walk `.py`; skip `.venv`, `venv`, `__pycache__`, `.git`, `dist`,
  `build`, `.tox`, `htmlcov`, `coverage`, `.eggs`, `*.egg-info`, and
  nested `pyproject.toml` roots in `Target.skip`.
  Walk root must canonicalize; files that fail canonicalize are skipped.

## Complexity attribution

Custom Rust scanner over source text (not the CPython AST). Indentation
defines suites.

**Structural integrity** ([`complexity/structure.rs`](../../../crates/crap-py/src/complexity/structure.rs)):
before attribution, fail the file on unclosed strings/comments or
unbalanced `{}` / `()` / `[]` (after skipping noise), or mixed tabs/spaces
in leading indentation. That is a collect error (exit 2), matching the
CLI contract.

**Visitor** ([`complexity/visitor.rs`](../../../crates/crap-py/src/complexity/visitor.rs)):

- Emit `def` / `async def` and methods (`Class.method`).
- Nested named defs are separate rows; the outer body masks the inner
  span before counting.
- Lambdas are **not** separate rows.
- Skip comments and string literals (including prefixes / triples) while
  scanning.
- Indent width: spaces count 1; tabs expand to the next multiple of 8
  (PEP 8). Mixing tabs and spaces in leading whitespace is a collect
  error.

**Cyclomatic** (base 1): `if`, `elif`, `for`, `while`, `except`, `case`,
`and`, `or`. Do not count `try` / `with` / `match` / `else` themselves.

**Cognitive**: nesting-weighted `if` / `for` / `while` / `try` / `with` /
`match`; flat `elif` / `else` / `except`; a run of the same `and` or
`or` counts once.

Limits: approximate by design — accepted product Limit; text scanner, not
CPython AST. Scores can diverge from AST-based tools; treat them as a
change-risk signal for reviewers, not an authoritative complexity audit.
Structural failures fail the run; valid line continuations and unusual
f-strings may still under- or over-count. Prefer fixing the Rust visitor
over adding a Python runtime or AST-crate dependency.
