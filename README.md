# herdr-agent-team

A [Herdr](https://herdr.dev) plugin that spawns and runs **heterogeneous coding-agent
teams** — Claude Code and Codex side by side as first-class teammates — under a
single coordinating "god" agent session, with push-based status reporting instead
of polling.

> **Status: v1 complete.** All spec §10 acceptance checks passed live on a
> real 2-worker heterogeneous team (2026-07-15): worktree isolation, brief
> injection, push reporting into the god pane, a worker→god `msg` round-trip,
> and clean teardown preserving the dirty worktree.

## What it does (v1)

- **`team spawn`** — reads a `herdr-team.toml` spec (or `--agents claude,codex`
  shorthand), creates one Herdr workspace per worker, launches each agent CLI in
  its pane (per-worker git worktree optional, with a project `setup` command),
  and creates one immutable worker protocol at
  `<run>/protocols/<worker>.md`. Repository-authored `AGENTS.md` files remain
  untouched.
- **Push reporting** — a manifest event hook fires on agent status transitions
  (`idle/working/blocked/done`); the plugin writes a report pointer into the
  team's inbox directory and injects a one-line wake-up into the god session's
  pane. The god never polls.
- **`team status` / `team kill`** — run-state inspection and teardown, backed by
  a durable run-state file in the plugin state dir.
- **`msg` verb** — the one messaging channel workers are ever briefed on:
  `herdr-agent-team msg <god|worker> "<text>"` resolves the target from the
  run, delivers with a single verified `pane run`, and — for agents that can't
  safely receive mid-turn — queues to a per-target **outbox** that the status
  hook drains the moment the target goes idle. Sender never blocks, no daemon.
- **God CLI** — `wait` observes durable lifecycle/report truth with bounded
  timeouts; `inbox` and `report` track unread worker reports; `msg all` and
  comma-separated targets reuse the same delivery/outbox policy.
- **Star or mesh topology** — per-team flag. Star (default): workers report only
  to the god. Mesh: workers also get a peer table and message each other through
  the same `msg` verb with a structured envelope.

## Team control deck

Open the native board as a durable tab with the `open-board` plugin action (or
open the `board` pane with an `overlay` placement for a quick popup). It polls
the newest active run by default, keeping collection outside its render tick.
Use `j`/`k` to select a worker; `m` sends a message, `g` acknowledges attention,
`K` kills only that worker, `o` opens its `report:` link, and `p` adopts a pane.

Example Herdr keybinding:

```toml
[[keys.command]]
key = "prefix+b"
type = "plugin_action"
command = "caioniehues.agent-team.open-board"
description = "open agent-team control deck"
```

## Why

Nothing on the Herdr marketplace orchestrates *heterogeneous* agent teams. The
two existing orchestration plugins are Pi-only. This plugin ports the
`agent-team` concept (generated peer communication protocols) from the
[limux](https://github.com/caioniehues/limux) project onto Herdr's superior
control plane (agent status machine, blocking waits, native worktrees).

## Supported agents

| Agent | Status |
|---|---|
| Claude Code (`claude`) | first-class, live-tested (mid-turn message queueing verified) |
| Codex (`codex`) | first-class, live-tested (mid-turn queueing verified; note: codex's default sandbox blocks worker→god `msg` without approval — see `examples/agents.toml` for the permissive-entry trade-off) |
| others | add via the data-driven launcher table in plugin config — no code changes |

Every launch prompt is injected and submitted with one `herdr pane run` call.
For launchers with `submit_verify = true`, the plugin waits for status
`working`; if that times out, it performs one empty `pane run` to submit the
existing composer without duplicating the prompt, then verifies again.

## Install

```bash
herdr plugin install caioniehues/herdr-agent-team
```

Local development:

```bash
cargo build --release
herdr plugin link .
```

### Upgrade or relink after manifest changes

Herdr caches `herdr-plugin.toml` when a plugin is linked. After **any** manifest
change, `herdr plugin disable` followed by `herdr plugin enable` still serves
the cached manifest. The failure signature is a stale reported version and new
events or panes that are missing or never fire.

Force Herdr to read the manifest again:

```bash
herdr plugin unlink caioniehues.agent-team
herdr plugin link /absolute/path/to/herdr-agent-team
```

Use that relink as the mandatory post-release smoke test, then read the linked
plugin's reported version back and confirm it matches the release. This is a
documentation workaround for current Herdr cache invalidation; if upstream
later reloads changed manifests automatically, retain it as historical release
guidance.

For direct terminal use, put the linked or installed binary on `PATH` once:

```bash
mkdir -p ~/.local/bin
ln -sf /path/to/herdr-agent-team/target/release/herdr-agent-team ~/.local/bin/herdr-agent-team
```

Without Herdr-injected environment variables, the binary resolves the stable
plugin state/config directories from the standard XDG or home layout.

## Documentation map

- [docs/spec.md](docs/spec.md) — full v1 specification (behavior, spec-file
  format, event flow, state layout).
- [docs/adr/](docs/adr/) — architecture decision records; every locked design
  decision with its why.
- [CONTEXT.md](CONTEXT.md) — domain glossary (god, worker, star/mesh, inbox,
  run-board…).
- [herdr-plugin.toml](herdr-plugin.toml) — the Herdr plugin manifest.

## License

MIT
