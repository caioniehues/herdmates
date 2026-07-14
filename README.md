# herdr-agent-team

A [Herdr](https://herdr.dev) plugin that spawns and runs **heterogeneous coding-agent
teams** — Claude Code and Codex side by side as first-class teammates — under a
single coordinating "god" agent session, with push-based status reporting instead
of polling.

> **Status: pre-v1 scaffold.** Design is locked (see [docs/spec.md](docs/spec.md)
> and [docs/adr/](docs/adr/)); implementation has not started.

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
- **Star or mesh topology** — per-team flag. Star (default): workers report only
  to the god. Mesh: workers also get a peer table and can message each other via
  `herdr agent send`.

## Why

Nothing on the Herdr marketplace orchestrates *heterogeneous* agent teams. The
two existing orchestration plugins are Pi-only. This plugin ports the
`agent-team` concept (generated peer communication protocols) from the
[limux](https://github.com/caioniehues/limux) project onto Herdr's superior
control plane (agent status machine, blocking waits, native worktrees).

## Supported agents

| Agent | Status |
|---|---|
| Claude Code (`claude`) | first-class, live-tested |
| Codex (`codex`) | first-class, live-tested with pane-run-only submission |
| others | add via the data-driven launcher table in plugin config — no code changes |

Every launch prompt is injected and submitted with one `herdr pane run` call.
For launchers with `submit_verify = true`, the plugin waits for status
`working`; if that times out, it performs one empty `pane run` to submit the
existing composer without duplicating the prompt, then verifies again.

## Install (once released)

```bash
herdr plugin install caioniehues/herdr-agent-team
```

Local development:

```bash
cargo build --release
herdr plugin link .
```

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
