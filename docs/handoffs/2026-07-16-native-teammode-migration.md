# Handoff — session 2026-07-16 (wave 1 close + native-teammode doctrine)

Session-scope handoff: everything decided, learned, and left pending in
the 2026-07-16 coordinator session. Complements (does not replace)
`HANDOFF.md` at the repo root, which holds the exact resumption
checklist for the code. Read `HANDOFF.md` for WHAT to do next;
read this for WHY and for the doctrine that now governs HOW.

## 1. Where the code stands (summary; authority = HANDOFF.md @ 1d2b9e0)

- **#84 D1 sidebar agent board: merged, closed, dogfooded** all wave.
- **#85 teammux shim: merged to main** (`ace14b4`) after adversarial
  review (MERGE-WITH-NITS) + geometry fix (`11fbe9a`); coordinator
  launcher commit done (`c9bebea`, `herdmates teammux-launch`,
  286 tests). **Commit 9 — live E2E of a real 2-teammate native team
  through the shim — NOT run.** This E2E is now the single most
  important pending step (see §3).
- **#86 focus pane: commits 1–8 on `feat/86-focus-pane`** (261 tests),
  review found 1 High (occurrence-unique attention ids), 1 Medium
  (atomizer regex), 1 Low (silent jump failure). Worker was HALTED
  mid-fix on Caio's stop order — **worktree
  `~/Projects/herdmates-issue84` may hold uncommitted salvage; inspect
  before re-briefing.** Not merged.
- Review artifacts live outside the repo:
  `~/Projects/herdmates-spike-recon/REVIEW-85.md` / `REVIEW-86.md`
  (archive under `docs/reviews/` before release).
- NOTHING pushed since `9c3c781`. Pushes = releases, Caio's word only.
  Version reconcile pending (Cargo.toml 1.0.0 vs manifest 2.0.0).

## 2. Doctrine change (Caio, this session): native teams are the substrate

`~/.claude/rules/agent-coordination.md` §Substrate was REPLACED:

1. Coordination (always on, terminal-independent): teammates are full
   independent Claude Code instances coordinating via file mailboxes
   (`~/.claude/teams/{team}/inboxes/{agent}.json`), locked task files
   (`~/.claude/tasks/{team}/`), team config
   (`~/.claude/teams/{team}/config.json`), and the native SendMessage
   tool. Works in ANY terminal — herdr, limux, plain shell.
2. Native teams own spawn/messaging/lifecycle — never re-implement.
3. Panes/sidebar/boards = display layer only, never the mechanism.

Also updated: global `CLAUDE.md` delegation section (stale
"not SendMessage" NOTE removed); memory
`research-workers-in-herdr-panes` marked SUPERSEDED (the surviving
residue is briefing discipline — narration + exact report format — not
the pane-only mandate).

**What this retires:** the wave-1 fleet protocol (hand-spawned panes,
BRIEF.md files, `pane run` push signals, coordinator-managed sidebar
tokens). That was scaffolding used to build the plugin before the shim
existed. **What it does NOT retire:** the plugin itself — see §3.

## 3. Why the plugin still matters (Caio asked; answer verified in code)

Native teams coordinate invisibly (in-process). The whole point of
herdmates is to give that substrate a visible home:

- **Shim (#85)** = the bridge: `herdmates teammux-launch` makes the
  lead think it's in tmux, so native split-pane teammate mode fires and
  every teammate lands as a REAL herdr pane — watchable, steerable.
  The pending commit-9 E2E is the proof of exactly this promise.
- **Board (#84)** reads `~/.claude/teams/` (config + inboxes) via
  teamfiles.rs/pump.rs → sidebar tokens. **Gap found this session: it
  does NOT read `~/.claude/tasks/` — zero references in the codebase.**
  Follow-up: a task-file reader in the pump would replace the manual
  `task=`/`step=` tokens with ground truth. Verify the task-file schema
  live (ADR-0010) before parsing — we have never read one.
- **Focus pane (#86)** = what needs the human now (Feed pattern).

## 4. Session analysis: greatest pains (from 30 sessions of records)

1. **Silent failures** — costliest class: `gh` compound-flag no-ops
   (issues silently not created), `sd` overreach twice (rewrote
   min_herdr_version → shipped tag briefly demanded wrong herdr), pipe
   swallowed a merge failure → v1.1.0 tag on the wrong commit, herdr
   `reload-config` `partial` silently keeping old UI. Rule that
   survives: verify every write by reading back; never chain off a
   pipeline.
2. **Fragile pane message delivery** — Enter-swallow universal, paste
   trap, queued-message merge. Dissolved by native SendMessage (no TUI
   composer in the path).
3. **Monitoring semantics that lie** — done/idle is attention state;
   idle-waits die on `blocked`; three monitoring redesigns before
   worker-push inverted the problem.
4. **Environment mortality** — codex monthly limit mid-wave (codex now
   banned), session usage limits hitting ALL panes at once (one
   account = correlated failure), unsolved config-borne claude startup
   crash. Mitigation that works: commit at every step boundary.
5. **Context economics** — mitigated well by planning-with-files +
   step-boundary compaction; in-session loss was NOT a major cost.

Meta-pattern: the deep pain was never the work — it was that nothing
(tool exit codes, worker reports, status enums, own chained commands)
could be trusted without independent verification.

## 5. Session analysis: bottlenecks

1. **Coordinator serialization** — every step funneled through gate →
   commit → resume; workers idled at boundaries waiting for my turn.
2. **Verification tax** — everything checked twice; ~doubles
   coordinator work per unit of worker output. KEEP (it's epistemics,
   not overhead), but it bounds throughput.
3. **Worker cold-start** — hand-authored BRIEF.md per worker + spawn
   lottery + Enter-swallow ritual. Largest memory-shaped cost:
   **workers are amnesiac; only the coordinator has memory.**
4. **Correlated rate limits** — nothing task-shaped fixes one shared
   account.
5. **Cross-session memory** — best-mitigated but the mitigation IS the
   cost (HANDOFF/learnings/memory write tax each session end); failure
   mode is recall-not-storage (documented traps re-hit: Enter-swallow,
   "herdr closed-source" stale assumption).

## 6. Migration assessment (Caio's conclusion: native teammode solves this)

- **Custom worker subagents with Claude's memory feature** → kills
  worker amnesia (bottleneck 3) and the workers' recall problem: a
  persistent builder teammate accumulates repo craft (gate commands,
  trait+fake pattern, no-rebase rule); briefs shrink to pointers.
- **Native teams** → kills pane message fragility (pain 2) and
  monitoring ambiguity (pain 3): SendMessage + mailboxes replace TUI
  composers and status-enum watching.
- **Beads instead of `~/.claude/tasks/`** (assessed, not decided):
  solves worker cold-start briefing (ticket carries context) and most
  of the handoff write tax (work-state lives structured in-repo, not
  re-serialized as prose); partly helps serialization via
  dependency-aware ready-queues. Does NOT touch: verification tax,
  rate limits. CAVEAT: beads knowledge is training-data-level — verify
  current `bd` CLI + multi-agent story via ctx7/upstream before
  designing around it. Board design question either way: whichever
  task store wins becomes the board's task-data source.
- **Nothing solves:** verification tax (keep), shared account limits.

## 7. Standing directives absorbed this session

- Native-team substrate doctrine (§2) — canonical in
  rules/agent-coordination.md.
- Stop order: all agent work halted; shim pane closed (scope done),
  build:86 + reviewer panes told to stand by.
- Effort default now medium (was low briefly); /caveman + Explanatory
  styles active earlier in session lineage.
- Mid-wave worker compaction works: send literal `/compact` via
  `pane run` at a step boundary, wait for "Compacted", re-point at
  BRIEF + task_plan. (Legacy-relevant only while panes are still the
  worker surface.)

## 8. Next session, in order

1. Salvage-check `~/Projects/herdmates-issue84`, finish the 3 REVIEW-86
   fixes, merge `feat/86-focus-pane` (graft its new modules — focusfile,
   attention, audit, jump, focus_pane — into src/lib.rs; main.rs is now
   `use herdmates::*`).
2. #85 commit 9 live E2E (the shim's proof) + #86 step 9 E2E evidence.
3. Archive REVIEW-85/86 under docs/reviews/; version reconcile; release
   on Caio's word.
4. File follow-ups: REVIEW-85 findings 2–3; board reader for the task
   store (task files or beads — decide after verifying both live).
5. Consider the migration experiment: define a memory-enabled worker
   subagent + run a real native team through the shim — the two
   validate each other.
