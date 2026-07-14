# herdr-agent-team — v1 specification

Distilled from the design interview on 2026-07-14 (grilling session in the limux
repo). Decisions are recorded as ADRs in [adr/](adr/); this document is the
buildable behavior spec.

## 1. Overview

A Rust binary packaged as a Herdr plugin. It orchestrates a **team** of coding
agents (Claude Code, Codex, extensible via config) inside Herdr workspaces,
coordinated by a **god agent** — the user's main interactive agent session.

The plugin has two halves:

1. **CLI half** (invoked by the god or the human): `team spawn`, `team status`,
   `team kill`.
2. **Event half** (invoked by Herdr): a manifest `[[events]]` hook that fires on
   agent status transitions and delivers reports to the god.

There is no daemon. Durable state lives in files under
`$HERDR_PLUGIN_STATE_DIR`.

## 2. Team spec file — `herdr-team.toml`

Lives in the project repo (versionable). The `--agents` CLI shorthand generates
a throwaway spec with defaults.

```toml
# herdr-team.toml
name = "limux-wave3"
topology = "star"            # "star" (default) | "mesh"
cwd = "."                    # team root; worktrees are created relative to the repo here

# Optional: run in each freshly created worktree before the agent launches.
# Encodes project-specific worktree preflight (symlinks, skip-worktree, deps).
setup = ["./scripts/worktree-setup.sh"]

[god]
# How to reach the god session. v1: the pane the spawn command runs from,
# overridable with an explicit herdr agent/pane target.
target = "self"              # "self" | explicit herdr agent name / pane id

[[workers]]
name = "builder-1"
agent = "claude"             # key into the launcher table
role = "builder"             # free text, goes into AGENTS.md + brief
worktree = true              # default: true for role=builder, false otherwise
branch = "feat/wave3-builder-1"   # worktree branch (required when worktree=true)
brief = "briefs/builder-1.md"     # path to brief file, injected at launch

[[workers]]
name = "reviewer-1"
agent = "codex"
role = "reviewer"
worktree = false
brief = "briefs/reviewer-1.md"
```

Validation: unique worker names; `agent` must exist in the launcher table;
`branch` required iff `worktree = true`.

## 3. Launcher table (data-driven agent roster)

Lives in `$HERDR_PLUGIN_CONFIG_DIR/agents.toml`. Ships with tested entries for
`claude` and `codex`; users add agents by config, not code.

```toml
[claude]
command = ["claude"]                 # argv, launched via herdr pane run
submit = ["Enter"]                   # keys to submit injected text
submit_verify = true                 # verify via `herdr agent wait --status working`
reads_agents_md = "pointer"          # needs a pointer line in the launch prompt

[codex]
command = ["codex"]
submit = ["Enter", "Enter"]          # codex TUI often needs two Enters
submit_verify = true
reads_agents_md = "native"           # codex reads AGENTS.md from cwd natively
```

## 4. `team spawn` behavior

Given a spec (file or shorthand):

1. **Preflight**: validate spec; check each worker's agent CLI exists on PATH;
   check `herdr` reachable (`HERDR_BIN_PATH`).
2. **Run dir**: create `$HERDR_PLUGIN_STATE_DIR/runs/<team>-<timestamp>/` with
   `run.toml` (resolved spec + live state) and `inbox/`.
3. Per worker:
   a. If `worktree = true`: `herdr worktree create` (branch from spec), then run
      the team `setup` command inside it.
   b. `herdr workspace create --cwd <dir> --label <worker-name>`.
   c. Launch agent CLI via `herdr pane run` in that workspace. **cwd is set at
      pane creation, never via a `cd` in the prompt.**
   d. Inject launch prompt: one line — read your brief at `<abs path>`, read
      `AGENTS.md` (pointer form for agents that need it), then submit per the
      launcher table (`submit` keys, verified with
      `herdr agent wait --status working`).
4. **Generate `AGENTS.md`** in the team cwd (and each worktree):
   - **star**: identity block (who you are, your role), report protocol (write
     report file to `<run>/inbox/<worker>.md`, then print the completion
     sentinel), how the god reaches you.
   - **mesh**: all of the above plus the peer table (name → workspace → how to
     message: `herdr agent send <name> "<agent-msg>…"`) and the message envelope
     format.
5. Record every worker's herdr agent id/name in `run.toml`.

## 5. Report flow (push, not poll)

- Manifest event hook on agent status change (socket event
  `pane.agent_status_changed`; exact manifest `on =` name to be verified against
  the herdr docs during build — see spec TODOs).
- Hook receives `HERDR_PLUGIN_EVENT_JSON`; plugin matches the pane against
  active runs (ignores non-team panes — cheap exit).
- On a team worker flipping `blocked` or `done`:
  1. Append an entry to `<run>/inbox/events.jsonl` (durable).
  2. Inject **one line** into the god's pane:
     `[team <name>] <worker> is <status> — report: <abs path>` — pointer only,
     never report content (keeps god context lean).
- Workers are briefed to write their actual report to
  `<run>/inbox/<worker>.md` *before* going idle/done.

## 6. `team status` / `team kill`

- `status`: read `run.toml` + live `herdr agent list` — table of worker, agent
  kind, herdr status, last report time. `--json` for the god.
- `kill`: close team workspaces (`herdr workspace close`), optionally
  `--remove-worktrees` (refuses if worktree dirty — salvage rule), mark run
  ended in `run.toml`.

## 7. Manifest surface (v1)

- `[[actions]]`: `spawn` (context: workspace), `status`, `kill` — thin wrappers
  over the binary for keybinding/palette use. The god calls the binary directly.
- `[[events]]`: agent status change → `<binary> on-agent-status`.
- No `[[panes]]` in v1 (dashboard is v1.1+), no link handlers.

## 8. Out of scope for v1 (roadmap)

- Dashboard pane (ratatui, overlay placement).
- `team restart` / reassign work.
- Run history browsing.
- opencode/gemini tested launchers (config entries welcome, untested).
- limux backend (extract shared generator crate only when that becomes real).

## 9. Build-time verification TODOs

- [x] Confirm the manifest `[[events]] on =` vocabulary for agent status
      transitions — RESOLVED 2026-07-14 by marketplace survey: shipped plugins
      `cobanov/herdr-ntfysh` and `horn553/herdr-ntfy` both hook
      `on = "pane.agent_status_changed"` in their manifests. Steal their
      payload handling as reference when implementing.
- [x] Confirm `HERDR_PLUGIN_EVENT_JSON` payload shape — RESOLVED 2026-07-14 by
      live test (herdr 0.7.3, protocol 16) with the linked fixture plugin
      `tests/fixtures/event-logger-plugin/`:

      ```json
      HERDR_PLUGIN_EVENT=pane.agent_status_changed
      HERDR_PLUGIN_EVENT_JSON={"event":"pane_agent_status_changed","data":{"type":"pane_agent_status_changed","pane_id":"wG:p2","workspace_id":"wG","agent_status":"idle","agent":"claude"}}
      ```

      Note the naming split: `HERDR_PLUGIN_EVENT` uses the dot form (manifest
      vocabulary); the JSON `event`/`data.type` use the underscore form (socket
      `EventKind`). `data` matches the socket schema's
      `pane_agent_status_changed` payload; nullable fields (`custom_status`,
      `display_agent`, `title`, `state_labels`) are omitted when null. Bonus:
      `HERDR_PLUGIN_CONTEXT_JSON` carries workspace/tab/focused-pane context,
      and `HERDR_PANE_ID`/`HERDR_WORKSPACE_ID`/`HERDR_TAB_ID` are set to the
      event's pane.
- [x] Live-verify inject-into-claude-pane mid-turn — RESOLVED 2026-07-14:
      `herdr pane run <pane> "<pointer line>"` into a working Claude Code pane
      lands as a queued message ("Press up to edit queued messages"), then
      auto-submits as a normal user turn when the current turn ends. No lost
      input, no interleaving into the active turn.
- [x] Live-verify codex double-Enter — RESOLVED 2026-07-14 (codex TUI,
      gpt-5.6-sol): `pane run` submits reliably in one call — use it. The
      double-Enter lore is real but specific to the split path: `agent send`
      followed *immediately* by `send-keys Enter` gets the Enter swallowed
      (paste-detection debounce) and the text sits in the composer; a second
      Enter submits. `agent send` + ~1 s delay + Enter also works. Plugin rule:
      always `pane run`, never send-text/send-keys pairs; keep
      `herdr agent wait --status working` as the submission check anyway
      (ADR-0006).

## 10. Definition of done (v1)

Spawn a real 2-worker team (claude builder in a worktree + codex reviewer,
star topology) on the limux repo; both receive briefs and start; a completed
worker's status flip lands a pointer line in the god pane within seconds; the
report file exists at the pointer path; `team kill` tears down cleanly and
preserves the dirty worktree.
