# Handoff — current state

Updated 2026-07-17 (post-v2.1.0). Previous handoff (wave 1 EOD, stop
order, "nothing pushed") is SUPERSEDED and archived at
`docs/handoffs/2026-07-16-wave1-eod-superseded.md` — do not act on it.

## Where things stand

- **v1 mission-control build COMPLETE and RELEASED as v2.1.0** (pushed +
  tagged). Stages 0–5 all landed: #95 version reconcile, #96 signal
  engine, #97 recorder, #98 TUI pane-board, #99 jump + confirmed nudge
  (first inbox write), #100 hook companion (3 team hooks registered
  user-scope in `~/.claude/settings.json`, spool → recorder + board
  wake), #101 manifest spawn fix (bare argv0 + `cargo install --path .
  --root ~/.local`).
- **#102 resolve_team liveness filter: DONE, committed local, NOT
  pushed** — shipping it means a v2.1.1 release (bump + tag), which
  needs Caio's explicit word.
- **Codebase review executed 2026-07-17**:
  `docs/reviews/codebase-review-2026-07-17.md` — 11 confirmed findings,
  all fixed in the same-day fix commit (see git log).
- Tracker: zero open issues. Repo slug is `caioniehues/herdmates`
  (renamed from herdr-agent-team; redirect works but don't rely on it).
- Current phase: **dogfood-in-anger** — use the board + hooks over real
  work; new tickets only on usage evidence.

## How to resume

1. `git log --oneline -15` — the commit subjects narrate the build.
2. Project state canon: auto-memory
   (`~/.claude/projects/-home-caio-Projects-herdr-agent-team/memory/`),
   loaded automatically; richest single source.
3. `docs/adr/0013-north-star-mission-control.md` + `docs/spec.md` — the
   north star; ADR-0012 for the pivot context.
4. `docs/learnings/` — per-issue wave learnings, newest first.

## Standing rules (unchanged)

- Pushes to `main` are releases: gate (fmt/clippy/tests), bump manifest
  version on behavior change, tag. **No push without Caio's word.**
- Hooks call the ABSOLUTE release binary path — rebuild
  (`cargo build --release`) after src changes or live hooks run stale.
- Frozen legacy surface per ADR-0012; new evidence → new ADR, ask Caio.
