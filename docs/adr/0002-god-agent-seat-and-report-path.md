# ADR-0002: God-agent-led run-board; reports via inbox file + pane pointer injection

Status: accepted (2026-07-14)

## Context

Someone must own lifecycle, briefs, and judgment. Candidates: human-led
dashboard, a privileged coordinator agent ("god"), or a neutral surface any
caller drives. The user's standing workflow is administrator-pattern: a main
Claude Code session plans/briefs/synthesizes while workers execute. Today that
coordinator polls report files and juggles `agent wait` loops.

The god runs in an interactive TUI — it cannot subscribe to events; something
must wake it. Verified: the herdr socket schema exposes
`pane.agent_status_changed` (enum idle/working/blocked/done/unknown), so push
is possible.

## Decision

- The user's main interactive agent session is the **god**. The plugin is its
  tool; the plugin never spawns a god (model borrowed from herdr-orchestrate).
- Report delivery is push, two-part: on a worker's status flip the event hook
  (1) appends to the run's durable `inbox/events.jsonl`, and (2) injects ONE
  line into the god's pane: `[team <name>] <worker> is <status> — report:
  <abs path>`. Pointer only — report content never enters the god's context.
- Workers write their real report to `<run>/inbox/<worker>.md` before going
  idle/done (briefed contract).

## Consequences

- God must run inside a herdr pane (injection needs a pane target).
- Injection lands as a queued user message in Claude Code — safe mid-turn
  (live-verify listed in spec TODOs).
- A future human dashboard is an observer/override, not the seat of judgment.
