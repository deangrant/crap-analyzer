---
name: complexity-ts
description: >-
  TypeScript LCOV join, package.json / workspace walk, and the hand-rolled
  complexity visitor for crap-ts. Use when editing crap-ts complexity, walk,
  or project resolve.
trigger: >-
  crap-ts, typescript, tsx, package.json, workspaces, TS visitor, LCOV
---

# TypeScript complexity and LCOV

Read before editing `crates/crap-ts/**`. Scoring bands and gate:
[crap-scoring](../crap-scoring/SKILL.md). Shared join / `--missing`:
[complexity-lcov-join](../complexity-lcov-join/SKILL.md). Rust style and
SOLID: [rust-style-guide](../rust-style-guide/SKILL.md) and
[rust-solid-design](../rust-solid-design/SKILL.md).

## Boundaries

- `crap-ts` is Rust-only. Do not add a Node helper, tree-sitter, or Oxc/SWC.
- Coverage is **LCOV on disk** via `crap_core::run`. Do **not** add an
  Istanbul / V8 JSON parser to `crap-core` or the frontend for v1.
- Render language string is `"typescript"`.
- Sources are `.ts` / `.tsx` only (skip `.d.ts`). No `.js` / `.jsx` in v1.

## Coverage

- `--coverage` defaults to `lcov.info`.
- Users produce LCOV with vitest, c8, jest, or nyc (`lcov` reporter).
- LCOV `SF:` paths must resolve to `.ts` / `.tsx` sources. Enable source-map
  remapping when covering emitted JavaScript under `dist/`.
- Empty spans and missing path joins use core `--missing`.
- Unresolved equal-rank ties are `coverage_join: ambiguous` (still
  scored via `--missing`); PathIndex uses `join_key` (package directory
  relative to the workspace) when set, else the npm package `name`.
  Reports still show the npm `name` as `crate_name`.

## Package walk

- Resolve packages from `package.json` (`project_resolve`).
  `package.json` is parsed with `serde_json` (full JSON string decoding;
  JSONC comments, trailing commas, and duplicate fields are unsupported).
- Without `--workspace` / `-p`, a `package.json` root is that package only
  (workspaces are **not** expanded).
- `--workspace` expands `workspaces` array or `workspaces.packages` globs
  (`*`, `**`, exact paths, and `!` exclusions). If `package.json` has no
  workspaces, falls back to `pnpm-workspace.yaml` `packages`.
  YAML list entries strip matching outer quotes only; escapes inside
  quoted paths are not decoded.
- `-p` selects by package `name`.
- Walk `.ts` / `.tsx`; skip `node_modules`, `.git`, `dist`, `build`,
  `coverage`, and nested `package.json` roots in `Target.skip`.
  Walk root must canonicalize; files that fail canonicalize are skipped.

## Complexity attribution

Custom Rust scanner over source text (not the TypeScript compiler).

**Structural integrity** ([`complexity/structure.rs`](../../../crates/crap-ts/src/complexity/structure.rs)):
before attribution, fail the file on unclosed strings/comments/templates
or unbalanced `{}` / `()` / `[]` (after skipping noise). That is a collect
error (exit 2), matching the CLI contract.

**Visitor** ([`complexity/visitor.rs`](../../../crates/crap-ts/src/complexity/visitor.rs)):

- Emit named `function` / `async function`, class methods
  (`Class.method`, `get` / `set`), and brace-bodied assigned
  `function` / arrow expressions (`const foo = () => { ... }`).
- Expression-body arrows and anonymous callbacks are **not** separate rows.
- Nested named functions are separate rows; the outer body masks the inner
  span before counting.
- Skip `declare` / overload signatures without `{` bodies.
- Skip comments, strings, templates (including `${...}`), regex literals,
  and JSX tags while scanning.

**Cyclomatic** (base 1): `if`, `for`, `while`, `do`, `case`, `catch`,
ternary `?`, `&&`, `||`, `??`. Do not count `switch` / `try` themselves.

**Cognitive**: nesting-weighted `if` / `for` / `while` / `do` / `switch` /
`catch`; flat `else`; a run of the same `&&` / `||` / `??` counts once.

Limits: approximate by design — accepted product Limit; text scanner, not
`tsc`. Scores can diverge from `tsc`-based tools; treat them as a
change-risk signal for reviewers, not an authoritative complexity audit.
Structural failures fail the run; valid generics, decorators, and unusual
TSX may still under- or over-count. Structural balance is not language
validity — nonsense tokens with balanced braces still collect. Prefer
fixing the Rust visitor over adding a Node or AST-crate dependency.
