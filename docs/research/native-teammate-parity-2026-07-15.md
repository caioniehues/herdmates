# Research: native-teammate parity review (2026-07-15)

Question: what does this plugin need for herdr-coordinated pane workers to
match the capability of Claude Code's native Agent-tool teammates? Sources:
repo docs/ADRs, live herdr 0.7.3 CLI surface, `src/agents_md.rs` /
`src/hook.rs`, and the herdr-agent-messenger study
([herdr-agent-messenger-2026-07-15.md](herdr-agent-messenger-2026-07-15.md)).
Outcome: ADR-0008, spec §11, tickets 12–15.

## Capability matrix

| Capability | Native teammates | Herdr team (pre-ADR-0008) | Verdict |
|---|---|---|---|
| Spawn + isolation | worktree flag | worktrees + setup + launcher table, heterogeneous roster | herdr richer |
| God → worker | SendMessage, auto-delivered | `pane run` pointer injection; queues mid-turn (claude verified) | parity for claude; codex mid-turn unverified |
| Worker → god mid-task | SendMessage anytime | briefed on `agent send` — **types but never submits** | was broken; fixed by `msg` verb |
| Message queuing | mailbox, deferred delivery | none | closed by outbox + hook drain |
| Worker ↔ worker | SendMessage | mesh envelope over `agent send` — same defect | fixed by `msg` verb |
| Shared task board | TaskCreate/List/Update, deps, claiming | none (run-board = lifecycle) | deferred, roadmap §8 |
| Plan-approval / shutdown protocols | typed request/response | none | cheap future envelope extension |
| Resume dead worker | SendMessage resumes transcript | none — but herdr tracks `agent_session_id`/`agent_session_path` | closeable; roadmap §8 (`resume_command`) |
| Watch / steer live | none (invisible) | `agent read`, `agent attach --takeover`, visible panes | **herdr wins** |
| Durable reports | ephemeral final message | inbox files + events.jsonl | **herdr wins** |
| Sync wait | TaskOutput block | `herdr wait agent-status --status done`; `herdr wait output --match <sentinel> --regex` | parity — sentinel already in protocol |
| Completion notify | task-notification | status hook → pointer injection | parity (push, verified) |

## Key findings

1. **Defect (fixed by ADR-0008/ticket 14):** generated protocols briefed
   `herdr agent send` for replies and mesh messages. Herdr help: "agent send
   writes literal text; use pane run when you want command text plus Enter."
   Three independent confirmations (herdr help, ADR-0006 live verify,
   messenger's source). Star file reports unaffected; interactive channel
   dead as briefed.
2. **The status hook is a free mailbox daemon.** Native teammates' core
   advantage is queued delivery. Messenger fakes it with a sender-blocking
   poll. We already own a push channel (`pane.agent_status_changed`) — outbox
   files drained on flip-to-idle give mailbox semantics with zero daemon and
   zero polling. Nothing on the marketplace does this.
3. **Mid-turn queueability is per-agent data**, not a global truth →
   `queues_midturn` launcher field (spec §3), codex conservative `false`
   until live-verified (spec §9 TODO).
4. **Never brief raw herdr primitives** — protocols teach one plugin verb;
   plumbing changes then touch one implementation, not every generated file.
5. **Useful herdr surface beyond current spec usage:** `herdr wait output
   --match --regex` (sentinel waits for the god), `herdr wait agent-status`
   accepts `done` where `herdr agent wait` help does not list it (client
   should standardize on `wait agent-status`), `pane report-metadata
   --custom-status` (worker progress pings — coexistence with the harness
   integrations' own reporting needs live verify, spec §9),
   `agent read`/`agent attach --takeover` (god-side inspection),
   `pane split --env` (state passing without prompt-text quoting hazards).
6. **Where herdr beats native** (keep as README selling points): visible,
   steerable, durable workers; heterogeneous rosters; reports that survive
   the coordinator's session.
