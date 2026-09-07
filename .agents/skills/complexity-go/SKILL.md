---
name: complexity-go
description: >-
  Go coverprofile → FileCoverage, build-tag skip, module walk, and the Rust
  complexity visitor for crap-go. Use when editing crap-go complexity,
  coverprofile, build tags, or module walk.
trigger: >-
  crap-go, coverprofile, cover.out, go:build, build tags, Go visitor,
  module walk, go.mod
---

# Go complexity and coverprofile

Read before editing `crates/crap-go/**`. Scoring bands and gate:
[crap-scoring](../crap-scoring/SKILL.md). Shared join / `--missing`:
[complexity-lcov-join](../complexity-lcov-join/SKILL.md) (path ranking and
empty spans apply after coverprofile is in `FileCoverage`).

## Boundaries

- `crap-go` is Rust-only. Do not add a Go helper binary or tree-sitter.
- Parse coverprofile in the frontend. Do **not** add a coverprofile parser
  to `crap-core`.
- Call `crap_core::run_with_coverage` with `FileCoverage` maps. Use
  `crap_core::run` only for on-disk LCOV.
- Render language string is `"go"`.

## Coverprofile → `FileCoverage`

[`coverprofile.rs`](../../../crates/crap-go/src/coverprofile.rs):

- Require a `mode:` header (`set`, `count`, or `atomic`) and at least one
  data line.
- Data line shape:
  `file.go:startLine.startCol,endLine.endCol stmts hits`.
- Expand each block across `startLine..=endLine` into `FileCoverage.lines`.
- `set`: hit → store `max(1)`. `count` / `atomic`: saturating-add hits.
- Normalize `\` to `/` in paths.
- After parse, remap import-path keys (`module/pkg/file.go`) to filesystem
  paths under the enclosing `go.mod` via `remap_import_paths` before
  `run_with_coverage`. Non-module keys stay unchanged.

Empty spans and missing path joins still use `--missing` in core.

## Build tags

[`build_tag.rs`](../../../crates/crap-go/src/build_tag.rs):

- Evaluate leading `//go:build` (preferred) or legacy `// +build` lines.
- Skip the whole file when the constraint is false for `--tags` plus host
  tags (`unix`, `linux`, `windows`, `darwin`).
- Unknown custom tags are false unless listed in `--tags`.
- No constraint → do not skip.

## Module walk

- Resolve packages from `go.mod` (`module_resolve`).
- Walk `.go` files; skip `vendor`, `.git`, `testdata`, and nested module
  roots.
- `--workspace` / `-p` select packages; a module-root `--path` analyzes
  every package under that module.

## Complexity attribution

Custom Rust scanner over source text (not `go/ast`, not tree-sitter).

**Visitor** ([`complexity/visitor.rs`](../../../crates/crap-go/src/complexity/visitor.rs)):

- Emit named `func` items and methods (`Type.Name`).
- Function literals / closures are **not** separate rows.
- Nested named funcs are separate rows; the outer body masks the inner
  span before counting.
- Skip comments and string / rune / raw literals while scanning.

**Cyclomatic** (base 1): `if`, `for`, `case`, `default`, `&&`,
`||`. Do not count `switch` / `select` themselves (branch labels only).

**Cognitive**: nesting-weighted `if` / `for` / `switch` / `select`; flat
`else`; a run of the same `&&` or `||` counts once. No per-`case`
increment.

Limits: imperfect Go grammar coverage (generics edge cases, unusual
syntax). Prefer fixing the Rust visitor over adding a Go runtime dependency.
