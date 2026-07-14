# ADR-0006: Data-driven launcher table; claude + codex tested in v1

Status: accepted (2026-07-14)

## Context

Herdr detects 14 agent CLIs, but each teammate needs launch argv, submit-key
behavior, and an "does it read AGENTS.md natively?" answer. Only claude and
codex can be live-verified on the author's machine. Known vendor quirk: the
codex TUI often needs two Enters to submit injected text; submission must be
verified via `herdr agent wait --status working`. Shipping untested launchers
invites bad first impressions in an empty niche.

## Decision

Agent definitions live in a TOML launcher table in
`$HERDR_PLUGIN_CONFIG_DIR/agents.toml` (command argv, submit keys,
submit-verify flag, AGENTS.md capability). v1 ships tested `claude` and
`codex` entries. New agents are config entries — community PRs, no code.

## Consequences

- Codex is a first-class teammate, not a special case — its quirks are table
  data.
- The README advertises "add your agent via config", a contribution hook.
- Untested entries are the contributor's responsibility; the shipped table
  stays honest.
