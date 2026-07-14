# ADR-0005: Rust; standalone repo; copy from limux-cli, don't depend on it

Status: accepted (2026-07-14)

## Context

Herdr plugins are argv commands in any language; `[[build]]` runs on the
user's machine at install. The AGENTS.md generator and agent-launcher logic
already exist in Rust inside the limux fork (`build_agents_md` in
`rust/limux-cli/src/main.rs`). Marketplace listing requires a public GitHub
repo (or subdir) with the `herdr-plugin` topic; tying the listing to a fork of
manaflow's cmux would confuse identity and bloat installs.

## Decision

- **Rust**, single binary, `cargo build --release` as the manifest build step.
- **Standalone public repo** `caioniehues/herdr-agent-team`, MIT.
- Port logic from limux-cli by **copying**, not by depending on the fork.
  Extract a shared generator crate only when a second consumer (a limux
  backend) is real — no speculative abstraction.
- Keep a thin backend seam (trait over the substrate verbs: create workspace,
  launch, send, notify) so herdr/tmux/limux targets stay possible without
  rework.

## Consequences

- Install needs the user's `cargo` — table stakes for herdr's audience.
- Temporary duplication of ~one module between repos, by design.
