# HANDOFF — next session orientation

Design locked 2026-07-14 via grilling interview in the limux repo. The scaffold
and core wave are committed; docs remain the contract.

## Read first

1. `docs/spec.md` — buildable v1 spec. §10 is the definition of done.
2. `docs/adr/0001–0007` — every locked decision + why. Don't relitigate
   silently; new evidence → new ADR.
3. `CONTEXT.md` — vocabulary (god, worker, star/mesh, pointer injection,
   run-board, launcher table).

## State

- Wave 0 scaffold: `0d75e23`.
- Wave 1 core modules (tickets 02-06): `a6e0ff9` — spec parsing/dry-run,
  launcher table, generated worker protocols, typed Herdr client, and durable run
  board. Central gate green: build, fmt, Clippy `-D warnings`, 32 tests.
- `spawn --dry-run` is demoable. Real spawn, event hook, status, and kill are
  still ticket stubs awaiting wave 2.
- Local git only — **NOT on GitHub yet.** Publishing = create public repo
  `caioniehues/herdr-agent-team` + topic `herdr-plugin` (marketplace auto-lists
  in ~30 min). Ask Caio before pushing.
- Source logic to port lives in the limux fork:
  `~/Projects/cmux-kde/limux/rust/limux-cli/src/main.rs` — `build_agents_md`,
  `agent_launch_command` (copy, don't depend — ADR-0005).

## NEXT steps (in order)

1. ~~Verify the four spec §9 TODOs~~ — **ALL RESOLVED 2026-07-14** by live test
   inside herdr 0.7.3 (protocol 16, matches snapshot). Findings + exact payload
   recorded in spec §9. Test fixture: `tests/fixtures/event-logger-plugin/`
   (linked but disabled; re-enable with
   `herdr plugin enable herdr-agent-team.event-logger`). Headlines:
   - `HERDR_PLUGIN_EVENT_JSON` = `{"event":"pane_agent_status_changed","data":{…socket payload…}}`;
     dot form in `HERDR_PLUGIN_EVENT`, underscore form inside the JSON.
   - Mid-turn `pane run` into Claude Code queues cleanly, auto-submits after
     the turn.
   - Codex: `pane run` submits in one call; double-Enter only needed for
     `agent send` + immediate `send-keys Enter` (debounce). Rule: always
     `pane run`.
2. Run wave 2 in parallel: ticket 07 spawn happy path, ticket 08 event hook,
   ticket 09 status/kill. Serialize `src/main.rs` ownership: 07 edits it first;
   08/09 report wiring patches if their edits would conflict.
3. Run ticket 10 worktree-worker support after 07 lands.
4. **Messaging wave (tickets 12–15, added 2026-07-15 — ADR-0008, spec §11):**
   fix the live protocol defect (generated protocols brief `herdr agent
   send`, which types but never submits) via the `msg` verb +
   `queues_midturn` launcher field + outbox drained by the status hook.
   Order: 12 → 13 → 14; 15 after 08+13. Ticket 14 now blocks the DoD
   (spec §10 gained a `msg` round-trip check). Background research:
   `docs/research/native-teammate-parity-2026-07-15.md` +
   `docs/research/herdr-agent-messenger-2026-07-15.md`.
5. Run ticket 11 manifest actions and the live limux DoD from the god session
   with Caio watching.
6. Only then talk to Caio about publishing; never push or add the
   `herdr-plugin` topic without explicit approval.

## Context that doesn't fit the docs

- Marketplace survey (175 plugins, 2026-07-14) is applied: curated conclusions
  in `docs/marketplace-notes.md` (patterns to steal with source pointers,
  competitive watch, Caio's install list); raw verdicts in
  `docs/marketplace-survey-2026-07-14.json`. Spec §9 TODO #1 (event name) is
  resolved from it.
- Caio plans to run coordinator (god) sessions inside herdr from now on —
  which this plugin's ADR-0002 report path requires anyway.
- Herdr is closed-source. Compatibility contract: snapshot `herdr api schema
  --json` into the repo and diff on herdr updates (not yet done — worth adding
  as a small script + CI-less check).
