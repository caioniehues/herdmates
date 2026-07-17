# Herdmates

A [Herdr](https://herdr.dev) plugin: **Claude Code teammates, native in herdr.**

Three surfaces:

1. **Shim (teammux)** — a fake `tmux` executable + `TMUX` env inside herdr panes
   that translates Claude Code's split-pane invocations into `herdr pane` calls,
   so native teammates land as real, steerable herdr panes. *Gated on the recon
   spike (ADR-0012) — not yet built.*
2. **Agent board** — hooks pump teammate state from the documented Claude Code team
   files (`~/.claude/teams/*/config.json`, `inboxes/*.json`) into herdr sidebar
   tokens via `pane report-metadata`; the sidebar becomes the board with zero
   rendering code. D1 (sidebar-token) is built — see
   [Agent board setup](#agent-board-setup-d1) below; D2 (rich TUI) comes later.
3. **Focus pane** — renders the human's single next action and decision queue from
   `~/.local/share/herdmates/focus.md`. ADHD-harness pattern (one thing at a time).
   *Not yet built.*

See `docs/adr/0012-pivot-to-herdmates.md` for the full pivot rationale and
verified facts about the Claude Code native team surface.

## Install

```bash
herdr plugin install caioniehues/herdmates
```

Local development:

```bash
cargo install --path . --root "$HOME/.local"
herdr plugin link .
```

The manifest's `command`s use a bare `herdmates` argv[0] (issue #101: herdr
spawns argv directly, with no shell, so a relative path like
`target/release/herdmates` never resolves) — `cargo install` is what puts a
resolvable `herdmates` on `PATH` (`~/.local/bin`, same place `herdr` itself
lives). A plain `cargo build --release` is not enough on its own.

### Upgrade or relink after manifest changes

Herdr caches `herdr-plugin.toml` when a plugin is linked. After any manifest
change, relink to force a fresh read:

```bash
herdr plugin unlink caioniehues.herdmates
herdr plugin link /absolute/path/to/herdmates
```

## Agent board setup (D1)

A hook already runs on every relevant pane/status event (`herdr-plugin.toml`'s
existing `on-agent-status` handlers) and publishes native-team state as herdr
sidebar tokens — no extra wiring needed once the plugin is linked. The
sidebar *rendering*, though, is your own `~/.config/herdr/config.toml`, not
something this plugin can install for you: merge
[`docs/sidebar-rows.toml`](docs/sidebar-rows.toml)'s `[ui.sidebar.agents]`
table into it, then `herdr server reload-config`.

herdmates publishes two tokens per native team lead, under source id
`herdmates-board`: `$task` (the lead's current task, when known) and
`$status` (`active`/`idle`). Reference them in your `rows` config alongside
herdr's own builtins — `state_icon`, `agent`, `state_text` (**not**
`state_label` — that name doesn't exist and typoing it fails silently, see
below).

A few things worth knowing before you edit sidebar config, verified live
against herdr 0.7.4 during the issue #84 prototype spike:

- **Invalid tokens fail silently.** `herdr server reload-config` returns
  status `"partial"` on an unknown token name and keeps the *old* sidebar
  layout running — no crash, no visible error. If a config edit "does
  nothing," that's the symptom; run `herdr config check` and re-verify every
  name.
- **Keep token values telegraphic.** `ui.sidebar_width` defaults to 26
  columns (max 36 via `ui.sidebar_max_width`) — the practical visible width
  for a value is roughly 20 characters, well under the 80-character wire
  limit herdmates enforces (`src/tokens.rs`, `MAX_TOKEN_VALUE_CHARS`). Don't
  rely on truncation to make a value fit.
- **Absent tokens omit the row**, not a blank line — safe to always include
  a `$task` row even for panes that never populate it.
- **Agent-less panes never appear in the sidebar**, regardless of rows
  config — only panes with a detected agent session are listed at all.

Full context and rationale: `docs/sidebar-rows.toml`'s own comments, and the
issue #84 prototype-spike write-up.

## Documentation map

- `docs/adr/0012-pivot-to-herdmates.md` — pivot decision and verified upstream facts
- `docs/adr/` — all architecture decisions with the why
- `docs/sidebar-rows.toml` — copy-paste `[ui.sidebar.agents]` config for the D1 board
- `CONTEXT.md` — domain glossary (pivot vocabulary first; legacy terms below)
- `herdr-plugin.toml` — the Herdr plugin manifest

---

## Legacy: v1.x team orchestration (frozen at v1.1.0)

> **Status: frozen.** The orchestration surface below was shipped in v1.1.0
> (the tombstone release) and receives no further investment (ADR-0012). The code
> remains in-tree; removal is a 2.0.0-scope decision.

The original v1 plugin spawned and ran **heterogeneous coding-agent teams** —
Claude Code and Codex side by side — under a single coordinating "god" agent
session, with push-based status reporting instead of polling.

### What v1 did

- **`team spawn`** — reads a `herdr-team.toml` spec (or `--agents claude,codex`
  shorthand), creates one Herdr workspace per worker, launches each agent CLI in
  its pane (per-worker git worktree optional), and writes one immutable worker
  protocol at `<run>/protocols/<worker>.md`.
- **Push reporting** — manifest event hook fires on agent status transitions;
  the plugin writes a report pointer into `<run>/inbox/` and injects a one-line
  wake-up into the god session's pane. No polling.
- **`team status` / `team kill`** — run-state inspection and teardown.
- **`msg` verb** — `herdmates msg <god|worker> "<text>"` resolves the target,
  delivers with one `herdr pane run`, and queues mid-turn messages to a per-target
  outbox drained by the status hook when the target goes idle.
- **God CLI** — `wait`, `inbox`, `report` for observing durable lifecycle truth
  with bounded timeouts.

### Legacy keybinding (if still using v1)

```toml
[[keys.command]]
key = "prefix+b"
type = "plugin_action"
command = "caioniehues.herdmates.open-board"
description = "open agent-team control deck"
```

### Legacy documentation

- [docs/spec.md](docs/spec.md) — full v1 specification
- [CONTEXT.md](CONTEXT.md) — domain glossary (legacy terms in the lower section)

## License

MIT
