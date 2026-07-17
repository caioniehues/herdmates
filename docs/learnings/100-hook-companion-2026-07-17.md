# Learnings — #100 hook companion (2026-07-17)

Task close per standing rule. Commit a523efd local main, #100 closed,
v1 complete (stages 0–5), not pushed.

## Live-verified facts

- **Hook payload field sets are seat-dependent — three shapes:**
  task events from a TEAMMATE session carry `team_name`+`teammate_name`;
  the same events from the LEAD session carry NEITHER (only
  `session_id`); `TeammateIdle` adds `permission_mode` and drops task
  fields. #91's "team_name deprecated, now session-derived" note means
  exactly this: consumers must derive team from `session_id` (teams-config
  `leadSessionId` scan — the `session-<prefix>` naming shortcut breaks on
  user-named teams).
- Hook config in settings.json live-reloads per event — no session
  restart to register/unregister (M1 fact, reconfirmed at dogfood).
- Claude Code tasks vanish on completion: `TaskUpdate(deleted)` after
  completion returns "Task not found" — cleanup of probe tasks is
  automatic.

## Design learnings

- "Event-driven" for an ephemeral-hook architecture = spool append as
  wake signal + existing poll as fallback. Consumers stat file length
  (cheap, no read/parse in the TUI); gather stays the single source of
  displayed truth.
- Exit-2 safety shipped as double-dead: opt-in config absent-by-default
  AND no blocking predicate in the binary — "cannot exit 2" is proven by
  test, not by configuration discipline.
- Ticket-framing correction recorded in the close comment: hooks close
  task/idle EVENT loss, not #88's message/inbox loss (no message-level
  hook event exists).

## Coordination learnings

- **Dogfood from a different seat than the builder captured from.**
  M1's payload capture ran in the builder's teammate session and was
  correct — and still missed the lead-payload shape entirely. The
  coordinator firing the same events from the lead seat exposed it in
  one probe. Generalization: when capturing an external schema, vary the
  observer's role, not just the event type.
- M1-as-blocking-ACK-gate (observation 10) held this time: builder
  stopped, waited, and the payload-capture round-trip caught the design
  into place before any I/O code existed.
- Two-axis review (standards/spec) before commit caught what the DONE
  report's own gate could not: the diff satisfied its tests while
  missing the ticket's live-surface requirement ("waking the signal
  engine"). Spec-axis review needs the ticket text, not the brief —
  the brief had already (incorrectly) narrowed scope to the recorder.
