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
   rendering code. D1 (sidebar-token) is the current target; D2 (rich TUI) comes
   later.
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
cargo build --release
herdr plugin link .
```

### Upgrade or relink after manifest changes

Herdr caches `herdr-plugin.toml` when a plugin is linked. After any manifest
change, relink to force a fresh read:

```bash
herdr plugin unlink caioniehues.herdmates
herdr plugin link /absolute/path/to/herdmates
```

## Documentation map

- `docs/adr/0012-pivot-to-herdmates.md` — pivot decision and verified upstream facts
- `docs/adr/` — all architecture decisions with the why
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
