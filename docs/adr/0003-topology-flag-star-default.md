# ADR-0003: Topology is a per-team flag; star by default, mesh opt-in

Status: accepted (2026-07-14)

## Context

Two useful shapes: **star** (workers ↔ god only — the user's proven daily
fleet pattern) and **mesh** (workers also message each other via `herdr agent
send`, the original limux agent-team value proposition and this plugin's
differentiator — no marketplace plugin does it). Uncontrolled peer chatter is
the known failure mode: crossed messages desync briefs and burn tokens.

## Decision

`topology = "star" | "mesh"` in the team spec, star default. One generator,
two worker-protocol templates: star emits identity + report protocol only;
mesh adds the peer table and message envelope. Each worker receives one
immutable protocol at `<run>/protocols/<worker>.md`.

## Consequences

- Default behavior is the manageable one; the flashy demo is deliberate.
- Mesh teams should pair with shared-cwd or explicit merge discipline (see
  ADR-0004); the generated mesh worker protocol must carry the anti-chatter policy
  knobs (when to message peers, size limits).
