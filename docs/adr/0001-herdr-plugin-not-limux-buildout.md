# ADR-0001: Ship agent-team as a Herdr plugin, not a limux build-out

Status: accepted (2026-07-14)

## Context

The `agent-team` concept (spawn heterogeneous agent workspaces + generate an
AGENTS.md peer protocol) originated in the limux fork. Limux's live control
bridge exposes only 18 verbs (no split/resize/focus on the wire), has no
blocking waits, no agent-status machine, and no worktree integration — months of
build-out before it could carry orchestration. Herdr already ships all of that
(status machine idle/working/blocked/done, `wait agent-status`, native
worktrees, 175-plugin marketplace) and its plugin API is the entire CLI via
`HERDR_BIN_PATH` — no SDK, no sandbox.

Marketplace scan (2026-07-14): only two orchestration plugins exist, both
Pi-agent-only. Heterogeneous agent teams are an empty niche.

## Decision

Build `herdr-agent-team` as a standalone Herdr plugin. Limux continues as a GUI
product on its own merits; orchestration value ships on Herdr's control plane
now.

## Consequences

- Days to v1 instead of months; herdr's sync primitives come free.
- Dependency on a closed-source binary; mitigation: snapshot `herdr api schema`
  _(correction 2026-07-15: herdr is open source — github.com/ogulcancelik/herdr;
  schema-snapshot discipline kept, see docs/agents/research.md)_
  and diff on herdr updates (compatibility contract).
- Limux may later become a second backend — see ADR-0005 for the extraction
  rule.
