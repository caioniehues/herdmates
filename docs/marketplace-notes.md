# Marketplace survey — curated conclusions (2026-07-14)

Survey of all 175 `herdr-plugin` topic repos (76 parallel assessors/divers;
raw verdicts in [marketplace-survey-2026-07-14.json](marketplace-survey-2026-07-14.json)).
This file is the re-ranked, load-bearing distillation.

## Patterns to steal for this plugin (verified in source)

1. **Event payload reference** — `horn553/herdr-ntfy` parses
   `HERDR_PLUGIN_EVENT_JSON` for `pane.agent_status_changed` with plain jq;
   `cobanov/herdr-ntfysh` (Go) hooks the same event. Use both as reference for
   `on-agent-status`.
2. **Pane-targeting discipline** — `persiyanov/herdr-reviewr` `src/herdr.rs`:
   resolve a target agent by sole-agent-in-tab, else sole-in-workspace, else
   **refuse with an error** ("several agents here") — never guess. Adopt for
   god-pane resolution when `[god] target = "self"` is ambiguous.
3. **`worktree.created` race** — reviewr and herdr-plus both react to it and
   clobber each other's pane placement. Community convention: `auto_open =
   false` default + orchestrator invokes actions explicitly. We drive our own
   layout — ship that convention from day one and document it.
4. **Packaging** — `smarzban/herdr-file-viewer` `[[build]]` fetches a
   SHA256-verified prebuilt musl binary, cargo build only as fallback. Softens
   ADR-0005's "install needs cargo" consequence once we cut releases.
5. **Output hardening** — file-viewer strips all escape sequences except SGR
   before rendering agent/worktree content. Pointer-injection text we write
   into the god pane must be built from our own strings only; any
   worker-derived fragment (names, paths) gets the same stripping.

### Ecosystem conventions

Titles only; see [awesome-herdr-2026-07-15.md](research/awesome-herdr-2026-07-15.md#ecosystem-conventions-to-adopt) for the twelve source-backed conventions:

1. Require `HERDR_ENV=1` for in-pane control.
2. Use `pane run` for text plus submit.
3. Store large payloads in files and inject pointers.
4. Refuse ambiguous routing.
5. Separate mechanism from policy.
6. Close only resources the plugin created.
7. Plugin processes start in the plugin root.
8. Cover both `worktree.created` and `worktree.opened`.
9. Use `HERDR_PLUGIN_STATE_DIR` for runtime state.
10. Dashboards should consume snapshots/events.
11. Lifecycle integrations aggregate and debounce child events.
12. Expose least authority by role.

## Competitive watch

- `ryonakae/shepherd` — structured `agent list/get/read --json` for
  coordinators. Adjacent, not competing; candidate dependency instead of
  reimplementing status reads. (Distinct from `jwarykowski/shepherd`, same
  name.)
- `aashishd/herdr-agent-messenger` — inter-agent messaging; overlaps mesh
  topology. Young. We differentiate on protocol *generation* (per-worker protocol files) +
  heterogeneous spawn + run-board. **Deep-dived 2026-07-15** (full report:
  [research/herdr-agent-messenger-2026-07-15.md](research/herdr-agent-messenger-2026-07-15.md)):
  no daemon/MCP — TSV call-sign registry + `pane run` as sole delivery
  primitive + sender-side readiness poll (3 s interval, 300 s cap, blocks the
  sender). Stole: readiness-gating idea (ours moves to the hook-drained
  outbox, ADR-0008), ambiguity-refusal addressing, SKILL-taught single `msg`
  verb instead of raw primitives, `pane split --env` for state passing.
  Their gaps our design already covers: single-line messages only, no
  delivery ACK, no broadcast, call-signs die with panes. Independent
  confirmation of two of our verified facts: `pane run` is the only submit,
  and only Claude Code is known to queue mid-turn input.
- `carsonjones/herdr-agent-dashboard` — precedent to study before building our
  v1.1 dashboard pane.
- Niche check (2026-07-14): nobody does spawn-team + generated worker protocols +
  heterogeneous roster.

Closest strategic competitors; retain the team-domain boundary in [upstream-integration-opportunities-2026-07-15.md §9](research/upstream-integration-opportunities-2026-07-15.md#9-native-overlap-and-obsolescence-watch):

- `razajamil/herdr-factory` — heterogeneous ticket-to-PR belts with queue, attention, resume, and dashboard overlap broadly; we retain explicit star/mesh topology, spawned/adopted workers, durable run state, pointer reports, and the verified `msg`/outbox path. ([awesome-herdr §Executive findings and Roadmap overlaps](research/awesome-herdr-2026-07-15.md#executive-findings))
- `Tudor0404/dual-author` — fixed issue→implementation→dual-review pipeline with a dependency dashboard overlaps roles, DAG, and dashboard; we remain a general heterogeneous team protocol rather than one delivery pipeline. ([awesome-herdr §Executive findings and Roadmap overlaps](research/awesome-herdr-2026-07-15.md#roadmap-overlaps-and-recommended-response))
- `sean1588/herdr-orchestrator` — deterministic YAML state, SQLite audit, GitHub gates, and recovery overlap task state/restart; we retain cross-launcher god/worker teams, adopted workers, worktree isolation, and pointer reports without a fixed YAML/GitHub model. ([awesome-herdr §Executive findings and Roadmap overlaps](research/awesome-herdr-2026-07-15.md#roadmap-overlaps-and-recommended-response))
- `erwins-enkel/shepherd` — browser mission control with plan/review/merge gates and resume overlaps supervised heterogeneous fleets; we remain Herdr-native and in-terminal, with explicit topology and durable run/report semantics. ([awesome-herdr §Executive findings and Roadmap overlaps](research/awesome-herdr-2026-07-15.md#roadmap-overlaps-and-recommended-response))
- `tomoasleep/herdr-symphony` — tracker-driven headless workers with report-file completion overlap task boards and report protocol; we retain an interactive god pane, heterogeneous/adopted workers, explicit topology, verified messaging, and richer run lifecycle. ([awesome-herdr §Executive findings and Roadmap overlaps](research/awesome-herdr-2026-07-15.md#roadmap-overlaps-and-recommended-response))
- `machine-machine/herdr-factory-loop-skill` — spec-driven mixed-agent fleet with disk context, dispatch/collect, hooks, and TUI overlaps fleet/run-board control; we retain Herdr-specific durable membership, worktree isolation, pointer reports, and explicit star/mesh coordination. ([awesome-herdr §Executive findings](research/awesome-herdr-2026-07-15.md#executive-findings))
- `david-lutz/herdr-claude-teams` — Claude experimental-team calls translated into native Herdr panes overlap team launch, metadata, notifications, and resume; we retain first-class Claude/Codex heterogeneity, adopted workers, and plugin-owned topology/task/report state. ([awesome-herdr §Executive findings](research/awesome-herdr-2026-07-15.md#executive-findings))

## Curated install list for Caio's machine (context: CachyOS/KDE, god-agent workflow)

Core: `smarzban/herdr-file-viewer`, `persiyanov/herdr-reviewr`,
`milkyskies/herdr-attention`, `tdi/herdr-worktree-setup` (add limux ghostty
symlink + skip-worktree to its config), `ntindle/herdr-resurrect`.
Notifications (pick one): `horn553/herdr-ntfy` (ntfy, zero-dep) or
`amurru/herdr-whistle` (Telegram bot, notification + remote control).
Situational: `haphamdev/herdr-simple-switcher`,
`edouard-andrei/herdr-layout-tools`, `furuhashin/herdr-synchronize-panes`,
`blurname/herdr-git-tab-name`, `kkckkc/herdr-plugin-gh-workflow`.

Note: seven worktree-preflight plugins, three tab-namers, and four
notification bridges exist — the market independently converged on the same
fleet-coordination pains this plugin's run-board addresses.
