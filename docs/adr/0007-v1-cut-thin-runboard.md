# ADR-0007: v1 cut — thin run-board; dashboard and lifecycle extras deferred

Status: accepted (2026-07-14)

## Context

Full ambition is a run-board (durable state, dashboard pane, lifecycle
management). Shipping everything first risks never shipping; the inbox/pane-
injection loop is the actual innovation and is demoable without a dashboard.

## Decision

v1 ships: `team spawn` (spec + shorthand), AGENTS.md generation, status-event
hook → inbox + god-pane pointer injection, worktrees + setup command, durable
run-state file, `team status`, `team kill`.

Deferred to v1.1+: ratatui dashboard pane (overlay), `team restart`/reassign,
run-history browsing, additional tested agents.

## Consequences

- First release in days; real dogfooding friction (on the limux fleet work)
  orders the v1.1 backlog instead of speculation.
- Definition of done for v1 is a live 2-worker heterogeneous team on the limux
  repo (see spec §10).
