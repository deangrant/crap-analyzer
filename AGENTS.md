# Agent and contributor guidance

Structured conventions for AI agents and humans working in this
**crap-analyzer** workspace. For product overview and the `crap-rs` CLI, see
[README.md](README.md).

## Docs

- [README.md](README.md) — scoring model, `crap-rs` usage, crate layout
- [`Cargo.toml`](Cargo.toml) — virtual workspace (`crap-core`, `crap-rs`) and
  maximum `[workspace.lints]`
- [`clippy.toml`](clippy.toml) — complexity and line thresholds
- [`rustfmt.toml`](rustfmt.toml) — `max_width` 100
- [`deny.toml`](deny.toml) — cargo-deny policy

## Pipeline

Local entrypoint that mirrors CI (`fmt --check`, Clippy, deny, audit, test,
rustdoc). Use `--locked` when `Cargo.lock` is present (CI always does for
test; Clippy uses it when the lockfile exists):

```bash
./scripts/check.sh
```

Equivalent commands:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo deny check
cargo audit
cargo test --workspace --all-targets --all-features --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked
```

CI also runs line coverage (100%) and a CRAP dogfood gate (`--fail-above --threshold strict`). Needs `cargo-llvm-cov` and `llvm-tools-preview`:

```bash
cargo llvm-cov --workspace --all-features --locked \
  --lcov --output-path lcov.info --fail-under-lines 100
cargo run -p crap-rs --locked -- \
  --lcov lcov.info --path . --workspace --fail-above --threshold strict
```

Lean pre-push (fmt + Clippy only). Install once per clone:

```bash
git config core.hooksPath scripts/githooks
```

## Anti-slop

- No `#[allow]`. Suppressions must be `#[expect(..., reason = "...")]`.
- Keep functions under the [`clippy.toml`](clippy.toml) thresholds instead of
  silencing complexity.
- Keep `.rs` files at or under 500 lines.

## Skills

Canonical skills live under [`.agents/skills/`](.agents/skills/). Read the matching skill before changing that area.

- [`.agents/skills/rust-style-guide/`](.agents/skills/rust-style-guide/) — formatting, docs, naming, API conventions
- [`.agents/skills/rust-solid-design/`](.agents/skills/rust-solid-design/) — SOLID in Rust: traits, modules, DI
- [`.agents/skills/verify/`](.agents/skills/verify/) — full local vs CI (`check.sh`, llvm-cov 100%, CRAP strict)
- [`.agents/skills/crap-scoring/`](.agents/skills/crap-scoring/) — formula, bands vs gate, `--missing`, dogfood flags
- [`.agents/skills/complexity-lcov-join/`](.agents/skills/complexity-lcov-join/) — visitor attribution, empty spans, path ranking

## Commands

Slash commands live under [`.agents/commands/`](.agents/commands/). Cursor loads
[`.cursor/commands`](.cursor/commands) → `../.agents/commands`.

- `/verify` — [`.agents/commands/verify.md`](.agents/commands/verify.md)
- `/audit-rust-skills` — [`.agents/commands/audit-rust-skills.md`](.agents/commands/audit-rust-skills.md)

## Rules

Canonical rules live under [`.agents/rules/`](.agents/rules/) (Cursor loads
[`.cursor/rules`](.cursor/rules)). Enable the matching rule when working in
that area.

- [`.agents/rules/ai-slop-mitigation/`](.agents/rules/ai-slop-mitigation/) — concrete diffs, no filler (opt-in)
- [`.agents/rules/rust-agent-standards/`](.agents/rules/rust-agent-standards/) — style/SOLID, `#[expect]`, Clippy 8, 500-line files (`*.rs`)
- [`.agents/rules/crap-metric-hotspots/`](.agents/rules/crap-metric-hotspots/) — match scoring, path ranking, empty spans

## Hooks

- Config: [`.cursor/hooks.json`](.cursor/hooks.json)
- `afterFileEdit` → [`.agents/hooks/rustfmt.sh`](.agents/hooks/rustfmt.sh) formats edited `*.rs` with `rustfmt` (fail-open)
- `sessionStart` → [`.agents/hooks/session-context.sh`](.agents/hooks/session-context.sh) injects `/verify` and skill reminders (fail-open)
- `stop` → [`.agents/hooks/verify-on-stop.sh`](.agents/hooks/verify-on-stop.sh) runs [`.agents/hooks/run-verify.sh`](.agents/hooks/run-verify.sh) on relevant dirty trees (`loop_limit` 3, timeout 600s); follow-up on failure, skip docs-only, fail-open on crash
- Git pre-push: [`scripts/githooks/pre-push`](scripts/githooks/pre-push) (`git config core.hooksPath scripts/githooks`)
