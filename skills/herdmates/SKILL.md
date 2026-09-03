---
name: herdmates
description: "Host and observe Claude Code agent teams in herdr via the herdmates plugin. Use when the user mentions herdmates, teammux, the recorder, or asks to launch, record, or hook a Claude Code team running in herdr panes — or when YOU are a team lead wondering why a teammate is stuck, whether anything is deadlocked, or who's on the team (herdmates why/deadlocks/roster). Not for general herdr pane control (use the herdr skill) and not for orchestrating work yourself — native Claude Code teams own spawn, messaging, and lifecycle."
---

# Herdmates

Herdmates makes herdr the visible home of Claude Code's native agent
teams: every teammate lands as a real herdr pane (the teammux shim),
and the signal engine + recorder + hook companion observe the
documented team files without a parallel UI (ADR-0015 — the native
lead is mission control; this plugin feeds it, never replaces it). It
**never re-implements native-team features** — spawn, mailboxes,
membership, and lifecycle belong to Claude Code; herdmates reads
`~/.claude/teams/{team}/` and drives the herdr CLI.

For general pane/agent control (splits, `pane run`, `agent prompt`), use
the `herdr` skill — or print the one bundled with the running binary:
`herdr --skill` (herdr ≥ 0.8.0). This skill covers only the herdmates
surfaces.

## Preconditions — check before any command

```bash
command -v herdmates            # binary on PATH (~/.local/bin)
test -n "${HERDR_PANE_ID:-}"    # inside a herdr-managed pane (pane surfaces only)
```

Anything misbehaving (hooks not firing, stale binary)? Run
`herdmates doctor` first — one line per check (binary/PATH, the three
team hooks in `~/.claude/settings.json`, state dir, herdr reachability
+ protocol match, the teams-env hint), non-zero exit if any check fails.

- No binary → the plugin is not installed. Install with
  `herdr plugin install caioniehues/herdmates` (runs
  `cargo install --path . --root "$HOME/.local"`; a plain `cargo build`
  is NOT enough — hooks resolve the bare `herdmates` name via PATH).
- Native teams are experimental: spawning teammates requires
  `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` in the lead session's
  environment. Without it there are no team files to observe.
- Outside herdr, `teammux-launch` is pointless: Claude Code falls back
  to in-process teammates. Say so and stop rather than launching anyway.

## Launch a team lead (teammux shim)

```bash
herdmates teammux-launch [claude args...]           # takeover (default): THIS pane becomes the lead
herdmates teammux-launch --split [claude args...]   # split a new pane for the lead instead
```

All arguments pass through to `claude` (`--resume` works). The launcher
puts a fake `tmux` on PATH plus a fake `TMUX` so Claude Code's
split-pane teammate mode (`teammateMode: tmux`) drives herdr instead:
each spawned teammate opens as a first-class herdr pane beside the lead.

Takeover requires unix exec; in environments without it, use `--split`.
`teammux-launch` is the only way in; the lead's own Claude session
spawns and steers teammates itself — herdmates never does.

## Observe a team

There is no standalone board or focus surface (deleted, ADR-0015) —
observation is ambient (sidebar tokens) and durable (the recorder),
feeding the native lead rather than a parallel UI:

- **Sidebar tokens** — the plugin's event hooks publish `$task`/`$status`
  per team lead (source id `herdmates-board`); rendering needs the
  `[ui.sidebar.agents]` table from `docs/sidebar-rows.toml` merged into
  `~/.config/herdr/config.toml`, then `herdr server reload-config`.
  Invalid token names fail SILENTLY (`"partial"` reload keeps the old
  layout) — after any config edit, verify with `herdr config check`.
- **Recorder** — `herdmates record --team <name> [--interval-secs N]
  [--log-path P]`: append-only JSONL of classified deltas at
  `~/.local/state/herdmates/recorder/{team}.jsonl`. Deltas only
  (baseline / transition / task_delta / hook signal), never every-tick
  spam. The log grows without bound — there is no built-in rotation;
  delete or archive old team logs yourself.

Badges/deltas come from the signal engine (single source of
teammate-state truth; strict precedence permission-prompt > blocked >
stalled > turn-complete). Doctrine: **never display a wrong reason** —
a teammate the engine cannot classify is reason-less "waiting", and
that is correct behavior, not a bug to work around.

## Ask about your own team (lead-facing verbs)

If you are the **team lead** (you spawned teammates via `teammux-launch`
and this is that session), three verbs answer the questions the ambient
surfaces above can't — no `--team` flag needed, they resolve the team
from *this session's own id* (`CLAUDE_CODE_SESSION_ID` matched against
the team's `leadSessionId`; ADR-0015 brain layer, pull path). Run from a
teammate session instead of the lead and they fail loudly (`not
registered as any team's lead`) rather than guessing — that's correct,
not a bug.

- **`herdmates why <agent-name-or-id>`** — reach for this when one
  teammate seems stuck and you want the reason, not just a status: full
  waiting-reason detail (permission-prompt / blocked-on-dependency /
  stalled tier / turn-complete / reason-less waiting), plus every raw
  fact behind it (transcript staleness, unread inbox, owned-task
  blocking). Non-lead teammates always show `agent_status: Unknown`:
  herdr holds no agent state for a teammate pane (its process runs as a
  version-numbered binary herdr's detector cannot name, and the pane's
  agent-session identity already belongs to herdr's own claude
  integration, which refuses a substitute) — so `needs-you`/`blocked`
  can never show for one. Their liveness comes from their own transcript
  instead. "no transcript resolved" means ownership could not be proved,
  not that the command failed.
- **`herdmates deadlocks`** — reach for this when the team feels wedged
  but every teammate looks idle: finds tasks stuck `in_progress` for
  ≥10 minutes with unfinished work still blocked behind them —
  suspected instances of the documented native bug where a task
  finishes but its status file never flips to `completed`. Always
  labeled a *suspicion*, never asserted as fact — verify before treating
  it as a confirmed deadlock.
- **`herdmates roster`** — reach for this for a quick "who's on this
  team and what state are they in" before deciding who to check on;
  each line carries a coarse 5-state label (`online` / `needs-you` /
  `blocked` / `waiting` / `stale`) plus the underlying engine reason.

  **It also tells you which teammates to reap, and that is the reason to
  run it habitually.** A teammate is a persistent Claude Code session:
  finishing its task does not end it. One measured on this machine sat
  alive 14 hours after reporting back, holding a pane and a roster row.
  A teammate whose own transcript has been silent past the stalled
  threshold gets `finished? reap it: TaskStop <name>`, and a summary
  line counts them. Either give it more work or `TaskStop` it —
  **do not close its pane**, which kills a live session you can still
  message. Nothing reaps a teammate automatically and nothing should;
  the roster row stays truthful either way.

All three print dense plain text meant for a model to read (no
tables/color) and degrade honestly on missing data — never a wrong
reason, never a guessed team.

## Hooks (already wired — do not re-register)

The plugin manifest registers `herdmates hook <event>` for Claude Code's
three team hook events (`TeammateIdle` / `TaskCreated` /
`TaskCompleted`); they append to a per-team spool that the recorder
consumes. Exit-2 gating capability exists but ships default-off.

**Stale-binary trap:** hooks run whatever `herdmates` is on PATH. After
any source change, `cargo install --path . --root "$HOME/.local"` again
or live hooks keep running the old binary silently. No herdr-side reload
is needed for a binary-only change (hooks re-exec fresh each event);
`herdr plugin link .` is required only after editing
`herdr-plugin.toml` (herdr caches the manifest at link time).

## Writes

Herdmates performs no team-file writes today — the only prior write
path (a confirmed inbox nudge from the TUI board) was deleted with the
board (ADR-0015); inbox-write is a break-glass-only fallback for when
the lead itself is down, not a routine surface. Everything herdmates
does is read-only by doctrine.

## Failure handling

| symptom | meaning | action |
| --- | --- | --- |
| `command -v herdmates` empty | not installed, or install used `cargo build` | `herdr plugin install caioniehues/herdmates`, re-check PATH |
| no team files under `~/.claude/teams/` | no live team, or `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` unset in the lead, or the team's session ended (config is removed at session end) | verify the env var in the lead pane |
| recorder log not growing while agents work | hooks running a stale or missing binary | reinstall the binary, confirm the manifest hooks fire (`~/.local/state/herdmates`) |
| sidebar rows unchanged after config edit | silent `"partial"` reload from an invalid token name | `herdr config check`; token is `state_text`, not `state_label` |
| teammates spawn in-process instead of panes | launched without the shim, or outside herdr | relaunch via `teammux-launch` from inside a herdr pane |

## Version discipline

Shim behavior was live-verified against Claude Code 2.1.237 and herdr
0.8.0-preview.2026-08-18 (protocol 20) on 2026-08-20, herdmates v3.0.0
(evidence: `docs/research/shim-e2e-2026-08-20.md`; original 2.1.211 /
0.7.4 evidence under `docs/research/`). Both dependencies move fast —
after upgrading either, re-verify the shim end-to-end (spawn one
teammate, confirm it lands as a pane) before trusting it, and re-snapshot
`docs/herdr-api-schema.snapshot.json` after any herdr update. When live
behavior disagrees with this file, live behavior wins (ADR-0010); update
this file.
