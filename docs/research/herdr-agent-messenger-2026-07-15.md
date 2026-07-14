# Research: aashishd/herdr-agent-messenger (2026-07-15)

Deep dive by a research subagent (fetched via GitHub API; distilled, verified
against repo source). Context: this plugin's mesh topology overlaps
messenger's territory; studied before locking ADR-0008.

Repo: Python + Bash + TypeScript, v0.2.1, created 2026-07-10 (5 days old at
study time), 4 commits, 1 star, tagged `herdr-plugin`.

## Architecture

Agent-to-agent messaging between live herdr panes on one machine, across
heterogeneous harnesses (Claude Code, Codex, pi, OpenCode). No daemon, no
socket server, no MCP:

- TSV registry `~/.local/state/herdr-messenger/names.tsv` keyed by
  `pane_id × terminal_id`, assigning each pane a deterministic two-word
  call-sign (`sha1(pane_id|terminal_id) mod namespace`, probe past
  collisions — concurrent assigners agree without locks; dead panes pruned
  lazily).
- Routing = shell out to `herdr pane list` + `herdr workspace list` at send
  time.
- Delivery = single `herdr pane run <pane_id> '<envelope>'`. Nothing else —
  no send-keys, no file+pointer.
- Compose UI = fzf board in a split, state passed via
  `herdr pane split --env "K=V"` + shared `mktemp -d`.

## Herdr primitives used (their working, tested set)

```bash
herdr pane list                                  # enumerate panes + agent_status
herdr workspace list                             # labels for display
herdr pane split <id> --direction down --ratio 0.35 --env "K=V" --focus
herdr pane run <board_pane> 'bash ...; exit'     # start board script
herdr pane run <target_pane> '<envelope>'        # DELIVER
herdr pane close <board_pane>
```

No herdr manifest events hooked — their hooks are harness hooks
(`UserPromptExpansion` / `UserPromptSubmit`), not `[[events]]`.

## Delivery semantics

- **Readiness gate, sender-side**: `send.sh` polls `herdr pane list` every
  3 s, up to 300 s, for `agent_status ∈ {idle, done, unknown}` before
  delivering. `--now` bypasses; their skill forbids `--now` on non-Claude
  harnesses (PROTOCOL.md: only Claude Code is known to queue mid-turn
  input). Sender's shell blocks for the whole wait.
- **No delivery ACK**: "delivered" = `pane run` exited 0. README: "no
  delivery acknowledgement or threading."
- **Addressing**: resolution ladder — exact pane id → exact call-sign →
  session-id prefix → call-sign substring → workspace label → label
  substring. Ambiguity exits 2 and lists candidates.
- **Single-line only**: `tr '\n\t' '  '` flattens payloads. No broadcast.

## Worth stealing (and what we did with each)

| Their idea | Our take (ADR-0008) |
|---|---|
| Readiness-gated delivery | Kept the gate, moved the wait off the sender: outbox files + status-hook drain — sender returns instantly, no polling |
| One `msg` verb taught via SKILL.md, `allowed-tools: Bash(msg:*)` | `herdr-agent-team msg` briefed in generated protocols; workers never see raw herdr primitives |
| Ambiguity-refusal addressing | Same (also marketplace pattern #2); we resolve from `run.toml`, fresher than their reverse-engineered TSV registry |
| `pane split --env` state passing | Noted for the v1.1 dashboard pane |
| DRAFT-REQUEST: Tab returns `additionalContext` telling the model to compose the body itself | Noted — elegant human/AI compose split for a future interactive surface |
| Single `harness_command.py` dispatcher returning typed outcome kinds | Pattern reference for multi-harness adapters |

## Their gaps our design already covers

Single-line messages (our payloads are files + pointers), no ACK (we verify
submission via `agent wait --status working`), no broadcast, call-signs die
with panes (our names live in the run-board).

## Maturity

Early but careful: 9 test files, README + PROTOCOL.md + SECURITY.md,
back-compat comment on registry path migration. Watch, don't depend. If it
matures, mesh could optionally interop (our `<agent-msg>` envelope travels
fine over their transport).
