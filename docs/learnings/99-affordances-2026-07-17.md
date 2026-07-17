# Learnings — #99 affordances: jump + confirmed nudge (2026-07-17)

Task close per standing rule. Commit b17a01d local main, #99 closed,
not pushed.

## Live-verified facts

- **An empty inbox read after a write is NOT a failed write.** Claude
  Code's runtime drained + delivered the nudge entry within ~1.5s of the
  atomic rename. Verifying an inbox write needs a fast poll (50ms caught
  28 non-empty snapshots) or an end-to-end delivery signal — a single
  read-after-write races the drain and reads as "nothing happened".
- The live entry schema (#89 capture) held exactly:
  `{from, text, timestamp ISO, msgV:1, msg_id uuid, type:"message",
  read:false}` — entry accepted, not pruned, delivered.
- ratatui pty captures fragment text into cursor-positioned cell runs —
  grepping a raw capture for whole strings false-negatives. Assert
  behavior (state machine effects, files written) or use TestBackend
  buffer asserts, not screen-scrape greps.

## Design learnings

- Sidecar lock via `OpenOptions::create_new`, NOT fs4/flock on the
  target file: Claude Code's own reader must see a consistent file
  through the rename; the lock protocol is a separate-path convention,
  not an OS lock on the inbox itself.
- Existing entries round-trip as raw `serde_json::Value` — unknown/
  future fields on entries we didn't write survive verbatim.
- Non-lead jump is structurally blocked: team-config schema has no
  per-member session id (same gap as #97/#98 agent_status).
  `pane_id: Option<String>` degrades to hidden; zero rework when the
  schema gains it.

## Coordination learnings

- **Milestone cadence collapsed under ponytail-ultra**: builder ran
  M1→M3 without check-ins (TDD held; reporting didn't). Delivery-
  compression skills bleed into process compliance — process steps that
  must survive them need to be gated ("STOP after M1, wait for lead
  ACK"), not scheduled. Observation 10 in the global skill log.
