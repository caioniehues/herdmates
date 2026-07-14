# ADR-0008: Worker messaging via plugin `msg` verb; outbox delivery for non-queueing agents

Status: accepted (2026-07-15)

## Context

Review on 2026-07-15 (native-teammate parity study + aashishd/herdr-agent-messenger
deep dive) found a live defect: the generated worker protocols (star reply path
and the whole mesh envelope, `src/agents_md.rs`) brief workers to message via
`herdr agent send`. Herdr's own help is explicit — "agent send writes literal
text; use pane run when you want command text plus Enter" — and ADR-0006's live
verification established `pane run` as the only reliable submit. As briefed,
every worker reply and mesh message lands in the recipient's composer and never
submits. Star file reports still work (status-flip hook), but the interactive
channel is dead.

Three further facts shape the fix:

- `pane run` targets pane ids; `agent send` was the only name-addressed verb.
  Briefing raw herdr primitives couples every generated protocol to herdr CLI
  semantics and repeats the plumbing in every protocol file.
- Mid-turn `pane run` queueing is verified only for Claude Code (spec §9).
  Codex is unverified; herdr-agent-messenger's PROTOCOL.md warns that
  non-Claude harnesses may not queue mid-turn input and its skill forbids
  immediate delivery to them.
- The capability bar is Claude Code's native teammate mailbox: send anytime,
  delivery deferred until the recipient can process it. herdr-agent-messenger
  fakes this with a sender-side poll of `herdr pane list` every 3 s for up to
  300 s — the sender's shell blocks for the whole wait.

## Decision

1. **`msg` verb.** New subcommand: `herdr-agent-team msg <god|worker-name>
   <text>` — resolves the target name to a pane id via the active run's
   `run.toml`, delivers with one `herdr pane run`, and verifies submission per
   the launcher policy (`herdr agent wait --status working`, one empty
   `pane run` retry on timeout — same discipline as ADR-0006). Generated
   worker protocols brief **only this verb**, never raw herdr primitives.
2. **`queues_midturn` launcher field.** Boolean per launcher-table entry.
   `true` (claude — live-verified): `msg` delivers immediately, any time.
   `false` or unknown (codex until verified): `msg` delivers immediately only
   if the target's agent status is idle/done; otherwise it enqueues.
3. **Outbox + hook drain.** Queued messages are files:
   `<run>/outbox/<target>/<seq>.msg`. The existing
   `pane.agent_status_changed` hook (spec §5) drains a target's outbox in
   sequence order when that target flips to idle or done. No daemon, no
   sender-side polling, sender returns instantly.

## Consequences

- `src/agents_md.rs` templates rewritten: star "God contact" section and mesh
  peer table / envelope instructions all say `msg`; the peer table no longer
  needs to expose pane ids to workers.
- `src/hook.rs` gains outbox drain alongside report-pointer injection; drained
  deliveries append to `inbox/events.jsonl` for audit.
- Launcher table schema grows `queues_midturn`; new spec §9 TODO to
  live-verify codex mid-turn `pane run` (until then codex ships `false` —
  conservative, only costs latency).
- Delivery policy becomes fixable in one place; a future herdr primitive
  change touches the `msg` implementation, not every generated protocol.
- Supersedes the raw-primitive briefing embedded in ADR-0003's mesh envelope
  description (envelope format itself unchanged — it just travels via `msg`).
