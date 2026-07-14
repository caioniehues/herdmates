# CONTEXT.md — domain glossary

Ubiquitous language for herdr-agent-team. One meaning per word; challenge
drift here first.

- **Team** — a named set of workers spawned together from one spec, plus their
  run state. One team ↔ one run dir.
- **Worker** — a single coding-agent CLI (claude, codex, …) running in its own
  Herdr workspace as part of a team. Identified by unique worker `name`.
- **God (agent)** — the user's main interactive agent session that coordinates
  the team: spawns, briefs, receives reports, decides. Exactly one per team;
  the plugin never spawns a god. (Term borrowed from herdr-orchestrate.)
- **Topology** — who may talk to whom. **Star**: workers ↔ god only. **Mesh**:
  workers also message each other peer-to-peer. Per-team flag; star is default.
- **Brief** — a per-worker instruction file the worker reads at launch.
  Delivered as a one-line pointer injection, never inline text.
- **Report** — a worker's durable output file at `<run>/inbox/<worker>.md`.
  Written by the worker before it goes idle/done.
- **Pointer injection** — the delivery mechanism: one line typed into a pane
  naming a file path. Payload stays on disk; context stays lean.
- **Inbox** — the run dir's `inbox/` directory: report files + `events.jsonl`.
- **Run-board** — the durable record of a team run (`run.toml` + inbox): who
  was spawned, where, current lifecycle state.
- **Launcher table** — data-driven config mapping agent kind → launch argv,
  submit keys, AGENTS.md capability. Adding an agent = adding a table entry.
- **Setup command** — team-spec command run inside each fresh worktree before
  the worker launches (project preflight: symlinks, deps, skip-worktree).
- **AGENTS.md (generated)** — the communication protocol file the plugin writes
  into the team cwd/worktrees: identity, report protocol, and (mesh only) the
  peer table + message envelope. Distinct from any repo's own authored
  AGENTS.md.
- **Status flip** — a Herdr agent-status transition (idle/working/blocked/
  done/unknown). Flips to `blocked`/`done` trigger the report flow.
