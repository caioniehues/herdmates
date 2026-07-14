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
