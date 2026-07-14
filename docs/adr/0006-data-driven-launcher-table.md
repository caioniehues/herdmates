# ADR-0006: Data-driven launcher table; claude + codex tested in v1

Status: accepted (2026-07-14; amended for verified pane-run submission)

## Context

Herdr detects 14 agent CLIs, but each teammate needs launch argv, a submission
verification policy, and an "does it read the repository's authored AGENTS.md
natively?" answer. Only claude and codex can be live-verified on the author's
machine. Live verification established that `herdr pane run <pane> <prompt>`
injects and submits reliably in one operation. The plugin does not expose split
send-text/send-keys flows. Submission may still be verified via
`herdr agent wait --status working`. Shipping untested launchers invites bad
first impressions in an empty niche.

## Decision

Agent definitions live in a TOML launcher table in
`$HERDR_PLUGIN_CONFIG_DIR/agents.toml` (command argv, submit-verification flag,
AGENTS.md capability). v1 ships tested `claude` and `codex` entries. New agents
are config entries — community PRs, no code.

Submission itself is pane-run-only and is not launcher-specific: the prompt is
sent once with `pane run`. With `submit_verify = true`, the plugin waits for
`working`; on timeout, it sends one empty `pane run` to submit the existing
composer without duplicating the prompt, then verifies again. It never pairs
send-text with send-keys.

## Consequences

- Codex is a first-class teammate, not a special-case submission path.
- Launcher configuration cannot request unsafe or unverified key-injection
  sequences.
- The README advertises "add your agent via config", a contribution hook.
- Untested entries are the contributor's responsibility; the shipped table
  stays honest.
