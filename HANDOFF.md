# HANDOFF — next session orientation

Scaffold committed 2026-07-14 (design locked same day via grilling interview in
the limux repo). You are picking up a **pre-v1 scaffold**: docs are the
contract, binary is stubs.

## Read first

1. `docs/spec.md` — buildable v1 spec. §10 is the definition of done.
2. `docs/adr/0001–0007` — every locked decision + why. Don't relitigate
   silently; new evidence → new ADR.
3. `CONTEXT.md` — vocabulary (god, worker, star/mesh, pointer injection,
   run-board, launcher table).

## State

- `cargo build --release` compiles; every subcommand is an explicit todo
  pointing at its spec section.
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
2. Implement `spawn` happy path (spec §4) against a throwaway 2-worker spec.
3. Event hook `on-agent-status` (spec §5).
4. `status` / `kill` (spec §6).
5. Live DoD run on the limux repo (spec §10), then talk to Caio about
   publishing.

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
