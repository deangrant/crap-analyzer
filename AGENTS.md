# Contributor Guidance

Guidance for AI agents and humans working in this repository.

**New task?** Glance → route → skill → change → verify.

This file is the entry point for repository conventions. Keep detailed architecture,
implementation guidance, and task-specific instructions in the referenced files rather
than duplicating them here.

Canonical agent assets live under [`.agents/`](.agents/); [`.cursor/rules`](.cursor/rules),
[`.cursor/commands`](.cursor/commands), and [`.cursor/hooks.json`](.cursor/hooks.json)
symlink or point there for editor integration.

## Repository at a glance

crap-score is a virtual Cargo workspace that scores functions by combining
complexity with line coverage. The score is a change-risk signal, not a quality
KPI.

| Package | Path | Role |
| ------- | ---- | ---- |
| Core | `crates/crap-core` | LCOV parse, path join, score, risk, and report |
| Rust frontend | `crates/crap-rs` | Cargo targets, walk, complexity, and the `crap-rs` CLI |
| Go frontend | `crates/crap-go` | Go modules, coverprofile, complexity, and the `crap-go` CLI |
| TypeScript frontend | `crates/crap-ts` | package.json, LCOV, complexity, and the `crap-ts` CLI |
| Python frontend | `crates/crap-py` | pyproject.toml, LCOV, complexity, and the `crap-py` CLI |

**Hard invariants (never violate):**

- `crap-core` parses LCOV only on disk. Frontends may load their native format into
  `FileCoverage` via `run_with_coverage` (`crap-rs` / `crap-ts` / `crap-py` LCOV via `run`,
  `crap-go` coverprofile).
- Frontends implement `Language`; scoring stays in `crap-core`.
- Risk bands classify the score. They never change the `--fail-above` gate.
- Empty spans and missing paths use `--missing`. Do not treat a missing join as 100% coverage.
- Workspace members are `crap-core`, `crap-rs`, `crap-go`, `crap-ts`, and `crap-py` only.
- No `#[allow]`. Suppressions must be `#[expect(..., reason = "...")]`.

**Runtime:** Rust toolchain `1.94.0`. Lean pipeline: `./scripts/check.sh`. Full local vs
CI: `/verify` (see [verify](.agents/skills/verify/SKILL.md)).

## Instruction Precedence

When instructions conflict, apply the most specific applicable instruction:

1. Repository-level `AGENTS.md`
2. Applicable files under `.agents/rules/`
3. Applicable files under `.agents/skills/`
4. Relevant documentation under `.agents/docs/`
5. Existing local implementation conventions

More specific guidance takes precedence over general guidance.

Rules define repository constraints and invariants. Skills provide task-specific
implementation guidance. Documentation provides architectural and product context.

This repo has no `alwaysApply: true` rules. Cursor injects rules by file glob or when
you opt them in. This file tells you **when to load** skills and docs; rules state
**what you must not do**. If a skill suggests something a rule forbids, the rule wins.

## Operating Principles

- Make the smallest change that correctly solves the task.
- Preserve existing crate boundaries and public contracts unless the task requires changing them.
- Prefer existing workspace crates, utilities, and patterns before introducing new abstractions or dependencies.
- Do not refactor unrelated code while completing a task.
- Do not weaken, bypass, or remove repository rules to make a change easier.
- Keep changes focused, reviewable, and consistent with the surrounding code.
- Prefer concrete diffs over speculative refactors. Match existing style; skip filler.
- Treat configuration, public CLI flags, and report contracts as significant; inspect their conventions before modifying them.

## Common mistakes

- Reading a risk band as the `--fail-above` gate — bands classify; the gate decides pass/fail.
- Treating empty spans or missing coverage paths as 100% coverage — apply `--missing`.
- Adding a coverprofile (or other non-LCOV) parser in `crap-core`, or documenting on-disk leftovers such as `cargo-crap`.
- Using `#[allow]` or silencing Clippy complexity instead of shrinking the function.
- Claiming `/verify` or CI passed without running the commands.
- Changing walk, symlink, skip-list, fail-closed collect, or CLI surface in
  one frontend without checking the same contract in the others — see the
  [Frontend alignment checklist](.agents/docs/ARCHITECTURE.md#frontend-alignment-checklist)
  (`dry-rs:ignore-file` ports stay separate on purpose).

## Workflow

Before changing code:

1. Inspect the repository status and relevant diff.
2. Identify the crate(s), files, and architectural area affected.
3. Read the applicable rules.
4. Read the matching skill(s) before modifying that area.
5. Check [ARCHITECTURE.md](.agents/docs/ARCHITECTURE.md) when the change crosses crate or pipeline boundaries.
6. Make the smallest appropriate change.
7. Run the narrowest relevant tests and checks first.
8. Run `/verify` before considering the change complete when practical.
   Procedure: [verify](.agents/skills/verify/SKILL.md).
9. Review the final diff for unintended changes.

Do not read every skill or document by default. Load only the guidance relevant to
the task being performed.

## Task routing

Read the matching skill **before** editing that area. Load only what the task needs.

| If you are changing… | Read first |
| -------------------- | ---------- |
| Scoring, bands, gate, `--missing`, dogfood flags | [`crap-scoring`](.agents/skills/crap-scoring/) |
| Visitor, empty spans, path ranking, LCOV join | [`complexity-lcov-join`](.agents/skills/complexity-lcov-join/) |
| Go coverprofile, visitor, build tags, module walk | [`complexity-go`](.agents/skills/complexity-go/) |
| TypeScript LCOV, visitor, package.json walk | [`complexity-ts`](.agents/skills/complexity-ts/) |
| Python LCOV, visitor, pyproject.toml walk | [`complexity-py`](.agents/skills/complexity-py/) |
| Rust style, docs, naming, API conventions | [`rust-style-guide`](.agents/skills/rust-style-guide/) |
| Traits, modules, dependency direction | [`rust-solid-design`](.agents/skills/rust-solid-design/) |
| Local vs CI verify or a stop-hook follow-up | [`verify`](.agents/skills/verify/) |

Cross-crate or pipeline changes: read
[ARCHITECTURE.md](.agents/docs/ARCHITECTURE.md) and every affected skill.

## Repository Documentation

- [README.md](README.md) — scoring model, `crap-rs` / `crap-go` usage, and crate layout
- [`.agents/docs/ARCHITECTURE.md`](.agents/docs/ARCHITECTURE.md) — crate boundaries, analysis pipeline, module maps, and invariants
- [`Cargo.toml`](Cargo.toml) — virtual workspace members and maximum `[workspace.lints]`
- [`clippy.toml`](clippy.toml) — complexity and line thresholds
- [`rustfmt.toml`](rustfmt.toml) — `max_width` 100
- [`deny.toml`](deny.toml) — cargo-deny policy
- [DeepWiki](https://deepwiki.com/deangrant/crap-score) — indexed project wiki for additional architecture, API, and pipeline context

## Rules

Canonical repository rules live under [`.agents/rules/`](.agents/rules/). The directory
is symlinked from [`.cursor/rules`](.cursor/rules).

Read the file-scoped rules that match the files you are changing, and any opt-in
rule the task needs.

### File-scoped

- [`.agents/rules/rust-agent-standards/`](.agents/rules/rust-agent-standards/) — style/SOLID, `#[expect]`, Clippy 8, 500-line files (`*.rs`)
- [`.agents/rules/crap-metric-hotspots/`](.agents/rules/crap-metric-hotspots/) — match scoring, path ranking, empty spans (Rust/Go complexity, merge, coverage)

### Opt-in

- [`.agents/rules/ai-slop-mitigation/`](.agents/rules/ai-slop-mitigation/) — concrete diffs, no filler

## Skills

Canonical skills live under [`.agents/skills/`](.agents/skills/).

Read the matching skill before changing the corresponding area. Skills are
task-specific guidance and should not be loaded unless relevant.

- [`.agents/skills/rust-style-guide/`](.agents/skills/rust-style-guide/) — formatting, docs, naming, API conventions
- [`.agents/skills/rust-solid-design/`](.agents/skills/rust-solid-design/) — SOLID in Rust: traits, modules, DI
- [`.agents/skills/verify/`](.agents/skills/verify/) — full local vs CI (`check.sh`, llvm-cov 100%, CRAP strict)
- [`.agents/skills/crap-scoring/`](.agents/skills/crap-scoring/) — formula, bands vs gate, `--missing`, dogfood flags
- [`.agents/skills/complexity-lcov-join/`](.agents/skills/complexity-lcov-join/) — visitor attribution, empty spans, path ranking
- [`.agents/skills/complexity-go/`](.agents/skills/complexity-go/) — Go coverprofile, visitor, build tags, module walk
- [`.agents/skills/complexity-ts/`](.agents/skills/complexity-ts/) — TypeScript LCOV, visitor, package.json walk
- [`.agents/skills/complexity-py/`](.agents/skills/complexity-py/) — Python LCOV, visitor, pyproject.toml walk

## Commands

Canonical slash commands live under [`.agents/commands/`](.agents/commands/).
The directory is symlinked from [`.cursor/commands`](.cursor/commands).

Prefer repository commands over manually recreating equivalent workflows.

- `/verify` — local CI verification: `check.sh`, llvm-cov 100% lines, CRAP `--threshold strict`
- `/audit-rust-skills` — style and SOLID audit that Clippy and rustfmt do not catch

## Hooks

Hook configuration lives in [`.cursor/hooks.json`](.cursor/hooks.json).

Hooks may auto-format after edit and may re-run verify on stop. They do not
guarantee correctness. Always run `/verify` explicitly before claiming done.

- `afterFileEdit` → [`.agents/hooks/rustfmt.sh`](.agents/hooks/rustfmt.sh) — formats edited `*.rs` with rustfmt (fail-open)
- `sessionStart` → [`.agents/hooks/session-context.sh`](.agents/hooks/session-context.sh) — injects `/verify` and skill reminders (fail-open)
- `stop` → [`.agents/hooks/verify-on-stop.sh`](.agents/hooks/verify-on-stop.sh) — runs [`.agents/hooks/run-verify.sh`](.agents/hooks/run-verify.sh) on relevant dirty trees (`loop_limit` 3, timeout 600s)

Git pre-push (not a Cursor hook): [`scripts/githooks/pre-push`](scripts/githooks/pre-push)
(`git config core.hooksPath scripts/githooks`). Fmt and Clippy only.

## Definition of done

A change is complete when:

1. Only intended files changed (review `git diff`).
2. Applicable rules and skills were followed for touched crates.
3. Narrow tests for touched paths passed (see table below).
4. `/verify` was run when the change is merge-ready, or you explicitly report what was skipped and why.
5. README or ARCHITECTURE were updated if behavior or crate contracts changed.

| Touched area | Minimum verification |
| ------------ | -------------------- |
| Any `.rs` / workspace code | `./scripts/check.sh` (or the equivalent failing step while iterating) |
| Scoring, join, or visitor | above + focused tests for the touched crate |
| Merge-ready claim | `/verify` (`check.sh`, llvm-cov 100% lines, CRAP `--threshold strict`) |

- Do not claim a check passed unless it was actually run and passed.
- If verification cannot be completed, clearly state what was not run and why.
