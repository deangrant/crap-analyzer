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
  scored via `--missing`); package names strengthen ranking
  (`{name}/src|lib|…` or a unique path component).

## Package walk

- Resolve packages from `package.json` (`project_resolve`).
- Without `--workspace` / `-p`, a `package.json` root is that package only
  (workspaces are **not** expanded).
- `--workspace` expands `workspaces` array or `workspaces.packages` globs
  (e.g. `packages/*`).
- `-p` selects by package `name`.
- Walk `.ts` / `.tsx`; skip `node_modules`, `.git`, `dist`, `build`,
  `coverage`, and nested `package.json` roots in `Target.skip`.

## Complexity attribution

Custom Rust scanner over source text (not the TypeScript compiler).

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

Limits: approximate by design. Generics, decorators, and unusual TSX may
under- or over-count. Prefer fixing the Rust visitor over adding a Node
or AST-crate dependency.
