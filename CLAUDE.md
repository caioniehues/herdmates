# CLAUDE.md — project context for Claude Code

Herdr plugin: spawn + run heterogeneous coding-agent teams (Claude, Codex, …)
under a coordinating "god" session. Pre-v1: docs are the contract, binary is
stubs.

## Read in this order

1. `HANDOFF.md` — current state + exact NEXT steps.
2. `docs/spec.md` — buildable v1 spec. §9 = open verification TODOs, §10 =
   definition of done.
3. `docs/adr/0001–0008` — locked decisions with the why. Don't relitigate
   silently; new evidence → new ADR, ask Caio first.
4. `CONTEXT.md` — vocabulary. Use these words exactly (god, worker, star/mesh,
   pointer injection, run-board, launcher table, status flip, msg verb,
   outbox, queues mid-turn).

## Hard rules

- **PUBLISHED 2026-07-15** (Caio's explicit go-ahead):
  `caioniehues/herdr-agent-team`, public, topic `herdr-plugin` — listed on
  the herdr marketplace. **Pushes to `main` are releases**: users install
  from this repo. Gate every push (fmt/clippy/tests), bump the manifest
  `version` for behavior changes, tag releases. Don't push without Caio's
  ask, per global rules.
- The herdr CLI (via `HERDR_BIN_PATH`) is the entire plugin API — no SDK.
  Ground truth for verbs: `herdr <cmd> --help` and
  `docs/herdr-api-schema.snapshot.json` (protocol 16 baseline; re-snapshot and
  diff after any `herdr update`).
- Port logic from limux-cli
  (`~/Projects/cmux-kde/limux/rust/limux-cli/src/main.rs`: `build_agents_md`,
  `agent_launch_command`) by **copying, not depending** (ADR-0005).
- Pane cwd is set at pane creation (`--cwd`), never via a `cd` in prompt text
  (ADR-0004 — split-brain trap).
- Report pointer injection into the god pane carries a file path only — never
  report content (ADR-0002).

## Verified facts (don't re-derive; authority tags per ADR-0010)

Herdr is **open source**: github.com/ogulcancelik/herdr (Rust core; Zig only
as vendored libghostty-vt). Local clone `~/Projects/herdr-upstream`. Evidence
hierarchy: live = behavior, source = attribution, preview = feature-detect
(ADR-0010). Reference layer: `docs/research/*2026-07-15*.md`.

- Manifest event `on = "pane.agent_status_changed"` valid `[live 2026-07-14]`,
  one of 21 hookable events `[source 2026-07-15]`: `HERDR_PLUGIN_EVENT_JSON` =
  `{"event":"pane_agent_status_changed","data":{type,pane_id,workspace_id,agent_status,agent}}` —
  dot form in `HERDR_PLUGIN_EVENT`, underscore form inside the JSON. `agent`
  optional; `title`/`display_agent`/`state_labels` may appear — tolerate
  unknown/absent optional fields.
- `pane run` = ONE request carrying text + Enter; herdr has NO paste-debounce
  `[source 2026-07-15]` — Enter-swallowing after `agent send` is agent-TUI
  behavior. Rule unchanged: always `pane run`, never split send-text/send-keys;
  `herdr agent wait --status working` as submission check (ADR-0006).
- `herdr agent send` writes literal text WITHOUT submitting — never brief it
  as a messaging channel. Workers message only via the plugin `msg` verb
  (ADR-0008, spec §11).
- Mid-turn `pane run` queues as a user message and auto-submits when the turn
  ends `[live: claude 2026-07-14, codex 2026-07-15]`. Queueing is implemented
  in the agent TUIs, not herdr `[source 2026-07-15]` — hence per-launcher
  `queues_midturn`; outbox covers launchers declaring false.
- Status enum exactly idle/working/blocked/done/unknown `[source 2026-07-15]`;
  `done` = idle + unseen attention state (`agent wait` rejects `done`,
  `wait agent-status` accepts it).
- `custom_status` is GONE from current upstream; metadata `tokens` are
  preview surface — schema-probe before any use (spec §8 step 3, §9).
- Shipped bug (priority issue): hooking only `agent_status_changed` misses
  `pane.moved` (new public pane id!) / `pane.exited` / `pane.closed` /
  `workspace.closed` / `worktree.removed` — run board can go silently stale.

## Environment note

Caio's machine has 10 marketplace plugins installed (ids + synergies: the
user-level `/herdr-plugins` skill, `~/.claude/skills/herdr-plugins/SKILL.md`).
Three of them hook `worktree.created` (tdi.worktree-setup, persiyanov.reviewr
auto-open, blurname.git-tab-name) — this plugin's spawn flow will fire that
same event per worker worktree, so test spawn WITH those installed; layout
races here are a feature-interaction bug, not a user config problem
(marketplace-notes.md pattern #3).

## Reference material in-repo

- `docs/marketplace-notes.md` — curated survey conclusions: patterns to steal
  (with source pointers), competitive watch list, race-avoidance convention.
- `docs/marketplace-survey-2026-07-14.json` — raw survey verdicts (69 deep
  dives) if the notes lack detail.

## Agent skills

Config for the mattpocock/skills engineering workflow.

### Issue tracker

Local markdown — specs and tickets live under `.scratch/<feature>/` in this
repo (no remote until publish; switch this file to GitHub Issues then).
See `docs/agents/issue-tracker.md`.

### Triage labels

Canonical five-role vocabulary, default strings (`needs-triage` /
`needs-info` / `ready-for-agent` / `ready-for-human` / `wontfix`).
See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `CONTEXT.md` + `docs/adr/` at the repo root.
See `docs/agents/domain.md`.

### Research rules

Always research external repos/libraries/docs via **ctx7** (find-docs skill)
first, upstream source second, live behavior decisive. Never assume — verify
inherited claims before building on them. Herdr is **open source**
(github.com/ogulcancelik/herdr). See `docs/agents/research.md`.

## Conventions

- Rust, `cargo fmt` + `clippy -D warnings` clean before commit.
- Every subcommand stub cites its spec section — keep that when implementing.
- Add regression tests alongside behavior; pure logic (spec parsing, AGENTS.md
  generation) stays separate from process-spawning code so it's testable.
