---
name: complexity-lcov-join
description: >-
  Cyclomatic and cognitive attribution, LCOV path ranking, and empty-span
  missing policy. Use when editing the complexity visitor, coverage parse,
  or path join.
trigger: >-
  complexity visitor, cyclomatic, cognitive, match arms, closures, nested
  fn, LCOV path, MatchRank, empty span, coverage join
---

# Complexity and LCOV join

Known failure modes from this project's history. Read before editing
`crates/crap-rs/src/complexity/**`, `crates/crap-core/src/merge/**`, or
`crates/crap-core/src/coverage.rs`. Scoring flags:
[crap-scoring](../crap-scoring/SKILL.md).

## Cyclomatic

- Base 1 plus decision points (`if` / `for` / `while` / `loop` / `match`
  arms / `?` / `let … else` / match guards).
- Each match arm adds 1, including `_`.
- Closures fold into the enclosing function.
- Nested `fn` items are separate rows; join excludes the inner span from
  the outer coverage.

## Cognitive

- Nesting-weighted increments (Sonar-style).
- A `match` is **one** increment for the whole expression, not one per arm.
- `let … else` scores like `if` / `else`.
- Match guards are flat +1. `else` / `else if` are flat +1.
- A run of the same boolean operator counts once.
- Labeled `break` / `continue` add 1. `?` is free. Direct recursion is free.

## Visitor attribution

- Closures: fold into the parent (do not emit a closure row).
- Nested functions: emit separately.
- Macros: decisions inside unexpanded or opaque macros may be missed.
- Trait default methods: omit (llvm-cov often has no line hits).
- `#[cfg]`: skip unless the feature/host predicate is enabled. Unknown
  predicates skip the item.

## Empty instrumented spans

If a function span has no `DA` lines, do **not** treat that as 100%.
Apply `--missing` (default pessimistic = 0%).

## Path ranking (`MatchRank`)

LCOV `SF:` paths and source paths may be absolute or relative.

- Rank **forward** matches (`src` ends with key) **above** reverse
  suffix length. A relative `SF:src/foo.rs` must beat a longer reverse
  false friend.
- Equal basename ties: Cargo package name matches `{name}/src|tests|benches|examples`;
  Go import path matches a contiguous path suffix (including remapped filesystem keys).
- Absolute and relative spellings of the same suffix merge for join and
  nested-exclude grouping.
