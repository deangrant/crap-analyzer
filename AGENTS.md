# Agent and contributor guidance

Structured conventions for AI agents and humans working in this repository. For
bootstrap and tooling overview, see [README.md](README.md).

## Docs

- [README.md](README.md) — scaffolding purpose, crate bootstrap, lint/CI notes
- [`Cargo.toml`](Cargo.toml) — virtual workspace and maximum `[workspace.lints]`
- [`clippy.toml`](clippy.toml) — complexity and line thresholds
- [`rustfmt.toml`](rustfmt.toml) — `max_width` 100
- [`deny.toml`](deny.toml) — cargo-deny policy

## Pipeline

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo deny check
cargo audit
cargo test --workspace
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

## Rules

Canonical rules live under [`.agents/rules/`](.agents/rules/) (Cursor loads
[`.cursor/rules`](.cursor/rules)). Enable the matching rule when working in
that area.

- [`.agents/rules/ai-slop-mitigation/`](.agents/rules/ai-slop-mitigation/) — concrete diffs, no filler (opt-in)

## Hooks

- Config: [`.cursor/hooks.json`](.cursor/hooks.json)
- `afterFileEdit` → [`.agents/hooks/rustfmt.sh`](.agents/hooks/rustfmt.sh) formats edited `*.rs` with `rustfmt` (fail-open)
