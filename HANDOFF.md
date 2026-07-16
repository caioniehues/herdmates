# Handoff — herdmates wave 1 executed; STOPPED before final E2E + release

Updated 2026-07-16 EOD, after Caio's stop-all-agents order. Previous
handoff (pivot/foundation) is fully superseded; foundation + wave 1 are
done. Read `docs/adr/0012-pivot-to-herdmates.md` first if you lack pivot
context, then this file.

## Where things stand (all local, NOTHING pushed since 9c3c781)

- **#84 D1 sidebar agent board: MERGED + closed** (in main since 3120c5d).
  Dogfooded live all wave as the fleet board.
- **#85 teammux shim: MERGED to main** (`ace14b4`), after adversarial
  review MERGE-WITH-NITS (`REVIEW-85` — see Review artifacts below) and
  the geometry-dispatch fix (`11fbe9a`). Coordinator commit 8 (launcher,
  `herdmates teammux-launch`) done on main: `c9bebea`, 286 tests.
  **Commit 9 (live E2E: real 2-teammate team through the shim, evidence
  under docs/research/, claude version pinned) NOT RUN — stop order.**
- **#86 focus pane: worker commits 1–8 ALL on `feat/86-focus-pane`**
  (worktree `~/Projects/herdmates-issue84`; e347ae5, 8e2426b, 95db411,
  44ecfdf, 35e9ca0, a89d6f5, 3dee064, c326efc; 261 tests). Adversarial
  review verdict MERGE-WITH-NITS (`REVIEW-86`) with fixes required
  before merge; fixes were dispatched to the worker, which was **HALTED
  mid-fix** by the stop order. **Its worktree may hold uncommitted
  partial fix state — inspect/salvage (`git status` + semantic diff)
  before re-briefing anything there.** NOT merged. Step 9 (live E2E)
  not run.
- Fleet: shim pane closed (scope done). build:86 pane and reviewer
  (spike) pane were told to stand by; they may or may not still exist.

## Exact next steps (in order)

1. Salvage check `~/Projects/herdmates-issue84` (uncommitted review-fix
   state), then finish the three REVIEW-86 findings:
   1. HIGH `src/attention.rs`/`src/audit.rs`: blocked/inbox attention ids
      are not occurrence-unique — a pane that blocks a second time is
      permanently swallowed by the append-only consumed set. Fix via
      occurrence nonce/timestamp in the id OR time-scoped consumed
      membership; add the missing cross-time collision test.
   2. MEDIUM `skills/atomizer/atomize.sh`: decisions regex
      `^-[[:space:]]*\[` accepts zero spaces; tighten to literal `^- \[`
      to match `focusfile.rs` and docs/focus-file.md.
   3. LOW `src/focus_pane.rs`: `QueueAction::Jump` discards jump errors
      (`let _ =`); surface a status line.
2. Gate + commit fixes; merge `feat/86-focus-pane` → main (expect a
   small conflict vs the #85 lib.rs restructure — main.rs is now
   `use herdmates::*` with modules in src/lib.rs; add #86's new modules
   there: focusfile, attention, audit, jump, focus_pane).
3. #85 commit 9: live E2E — `herdmates teammux-launch` a real
   2-teammate team (`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`) inside
   herdr; archive log + pane snapshots under `docs/research/`; pin the
   claude version tested.
4. #86 step 9: live E2E evidence for the focus pane (open via
   plugin.pane.open split, exercise jump/done against a real team).
5. Release call to Caio: version bump (manifest is 2.0.0 line;
   Cargo.toml still says 1.0.0 — reconcile), tag, push. Pushes to main
   are releases; Caio's word required.
6. File follow-up issues (triage candidates): REVIEW-85 finding 2
   (resize-pane -x fixed-right direction is geometrically unsound —
   needs ADR note), finding 3 (HerdrClient real CLI paths untested —
   thin integration test once herdr is scriptable in CI).

## Review artifacts (not in repo — coordinator workspace)

`/home/caio/Projects/herdmates-spike-recon/REVIEW-85.md` and
`REVIEW-86.md` (+ the BRIEF-review-*.md contracts). Consider archiving
under `docs/reviews/` with the wave records before the release.

## What shipped in wave 1 (for the release notes)

- D1 agent board: team-file → sidebar-token pump, board/pump-board
  commands, event wiring (#84).
- teammux shim: idmap (flock-transactional %N/@N), static probes,
  structural reads, split-window, lifecycle verbs, styling no-ops
  (logged under TEAMMUX_LOG), geometry format-string dispatch from live
  pane layout, `teammux-launch` lead launcher with fake TMUX env +
  scoped `{"teammateMode":"tmux"}` settings (#85).
- Focus pane (unmerged, pending fixes): focus-file contract + parser
  (FNV-1a stable decision ids), docs/focus-file.md, attention queue
  (blocked > decisions > lead inbox), JSONL audit log, `jump`
  subcommand, ratatui TUI with selectable queue (Enter=jump, d=done),
  atomizer skill (#86).

## Key wave learnings (full detail: docs/learnings/herdmates-wave1-2026-07-16.md)

- Worker-push protocol (workers `pane run` the coordinator at step
  boundaries) replaced polling; watches only for blocked/crash.
- Mid-wave `/compact` of worker panes at step boundaries works cleanly —
  send the literal text `/compact` via `pane run`, wait for "Compacted",
  then re-point at BRIEF + task_plan.
- Session usage limits hit all worker panes at once (same account) —
  wake after reset, re-brief with pointers, nothing lost if state was
  committed at boundaries.
- Merge order trap: branches created before the lib.rs split need
  their new modules grafted into src/lib.rs (see step 2 above).

## Standing rules unchanged

- Pushes to main are releases — gated, version-bumped, tagged, Caio's
  word required.
- Coordinator owns all git; workers never mutate git.
- Verify external claims via ctx7/upstream source (ADR-0010);
  `~/Projects/herdr-upstream` goes stale — pull before citing.
