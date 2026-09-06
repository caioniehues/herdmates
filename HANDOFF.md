# Handoff — current state

Updated 2026-09-03. Sections below dated 2026-08-21 and earlier are still
accurate for what they cover; the teammate-lifecycle wave on top of them is
summarised first.

## Teammate lifecycle wave (2026-09-02 / 09-03) — branch `fix/teammate-idle-self-wake`, UNPUSHED

Five commits on that branch, gate clean at 365 tests. **The full record lives
outside the repo** — `~/.claude/plans/herdmates-panes-and-stale-agents-findings.md`
(read §10 first: what is actually true of the primitives; then the retracted
Fix 3 and the shipped Fix 3′/Fix 4 in §6; then §8's thirteen reversals).

- **Self-wake fixed** (`88df226`). A split-pane teammate's `TeammateIdle` hook
  inherits its OWN inbox socket, so the enrichment post woke the teammate that
  generated it — 208 injections into teammates, none into any lead. In-process
  teammates were never affected and still deliver. Filed upstream as #133,
  which offers a PR from a fork; **not opened.**
- **Lead-tab arrangement fixed** (`4d87f31`, `8188a33`). Claude Code
  binary-subdivides the lead's window then repairs it with `select-layout
  main-vertical`, which the shim was dropping as a styling no-op while
  honouring the `resize-pane` that accompanies it. Now arranged at split time;
  lead at 30%, teammates in one column.
- **Finished teammates are now visible** (`9251b49`, `f0fdcc7`, `9e63c3d`).
  A teammate is a persistent session — one measured alive 14 h 09 m after
  finishing — and only `TaskStop` reaps one. `roster` and `pump-board` now name
  the teammates waiting for it. Registry rows are deliberately untouched.

**Fixed 2026-09-06 (`387145d`): the reap badge never cleared.** `pane report-metadata --token`
only sets, so a token that stops being published keeps its last value. `reap` skipped publishing
at count zero, which meant the badge asking you to reap survived the reaping — found live on a
lead frozen at `reap: 10 to stop` with no teammates left. The pump now clears the names it does
not publish, in the same call. Two things worth carrying forward: **clearing is not scoped to
`--source`** (it will remove another producer's token of the same name — measured), and the
deleted-`--clear-token` mutant initially survived the whole suite because the only coverage was
`FakeHerdr`, which reimplements the call instead of running it; `FakeBinary::recording_argv`
now asserts on the real argv.

**Next session has a brief:** `~/.claude/plans/herdmates-agent-detection-brief.md` — the
`HERDR_AGENT` lead (item 3 below), scoped so the first move is a gate question, not a design.

**Next steps, in the order they are worth doing:**

1. **Installed 2026-09-03 17:04** from this worktree via the plugin's own build hook
   (`cargo install --path . --root "$HOME/.local"`), which also re-registered the install source
   from the predecessor's worktree to this one. Backups: `~/.local/bin/*.pre-reaping-fix.bak`
   (this install) and `*.pre-selfwake-fix.bak` (pre-2026-09-02). Verified on real traffic — a
   genuine `TeammateIdle` hook 105 s after install spooled correctly. A `herdr plugin update`
   still rebuilds from upstream and silently reverts everything on this branch.
   `docs/sidebar-rows.toml`'s `$reap` row was also added to the live herdr config
   (`~/.dotfiles/config/herdr/config.toml`, `rows_by_agent.claude`, uncommitted there);
   `reload-config` returned `applied`, not `partial`.
2. **Correction to `f0fdcc7`'s commit message, worth knowing before you read it:** it says
   "nothing told the lead a teammate was done". Claude Code *does* — a native
   `idle_notification` carrying the teammate's summary and result reaches the lead's session
   (measured 2026-09-03, 7 minutes after a teammate finished). The lead was told and still had
   not reaped 5 h 36 m later, which is the real justification for the reap surface: a standing
   count, not another notification. Consequence: **Fix 5 in the findings doc is downgraded** from
   "restore missing delivery" to "add engine reasons to a notification that already arrives", and
   Probe 1 no longer blocks anything. Full record in that doc's §11.
3. **Defect D may be fixable after all — highest-value open lead.** The recorded conclusion
   ("herdr cannot hold a teammate's agent state") was too broad: it generalised from
   `pane.report_agent` (genuinely blocked) to all routes. `HERDR_AGENT=<agent>` is a documented
   env hint read from the process environ (`herdr/src/platform/mod.rs:322`) that drives herdr's
   OWN detection, and `teammux::respawn_pane` already owns the teammate's launch string, so
   prefixing it is reachable. **PROBED 2026-09-06 and the detection half WORKS** — findings doc
   §12 has the table. `HERDR_AGENT=claude` on the final binary of a `pane run` command turns
   `agent: null / unknown` into `agent: "claude" / idle`. **The naive implementation fails
   silently:** Claude Code's teammate command is compound (`cd … && env … <binary>`), so
   prefixing the whole string applies the variable to `cd` and changes nothing. Use
   `export HERDR_AGENT=claude; ` prepended, or inject into the existing `env`.
   **GATE ANSWERED 2026-09-06 — it passes** (findings doc §12.1). A real `claude` launched from
   the versioned path with the hint reports `agent: "claude"` and cycles `blocked` (verified
   against a real on-screen prompt) → `working` → `idle`; without the hint the same process is
   `agent: null`. So the status is a real read and `needs-you` becomes reachable for a teammate,
   which `roster` currently says is impossible. **Implement.** Two carry-forwards: poll faster
   than 1 s or be event-driven (a 4 s poll produced a false negative — the `working` window on a
   short turn is ~1–2 s), and confirm the same holds for an actual teammate before building on
   it, since the probe used a normal session.
4. **Do NOT file a new herdr issue** for Defect D: **herdr#2779 already reports it** (open since
   2026-08-13, `bug` + `maintainer-needed`), and herdr's `bug.yml` template explicitly forbids
   the shape the draft used (no root cause, no proposed fix; bots close such issues same-day).
   A short comment on #2779 adding the Linux + teammate-spawn data point is drafted at
   `~/.claude/plans/herdr-issue-REVISED.md` §7, unposted. The original draft for
   `lookup_agent` not recognising Claude Code's
   version-path argv0 is superseded; note it is NOT teammate-specific — ordinary `claude` panes
   fail too on installs whose launcher is a script rather than a symlink.
4. **Defect E — FIXED 2026-09-06, both halves** (`8c75713`, `8fc8e6a`).
   `gather::recover_lead_pane_via_teammates` recovers a rotated lead's PANE for
   `pump_once`/`gather_team`; `brain::resolve_own_team` recovers the lead's own
   TEAM so a resumed lead keeps `why`/`deadlocks`/`roster`. Both read a live
   teammate's `--parent-session-id` and share `gather::teammate_launches`. The
   recorded `leadSessionId` is still tried first, and a session that genuinely
   leads nothing still gets the honest error.
5. Release when Caio says: bump the manifest + Cargo version (**owed** — these
   commits change behaviour) and tag.
6. **Verification note:** do NOT use `grep <string> ~/.local/bin/herdmates` to
   confirm an install. Release builds inline short literal comparisons, so
   `--parent-session-id` is absent from the binary that demonstrably uses it.
   Run the binary against a fixture instead.

## Where things stand (2026-08-21)

- **v3.0.0 released** 2026-08-20 (pushed + tagged): #117 sh-c fix, #119
  legacy-v1 deleted (−13.9k loc), #120 board/focus cluster deleted (−2.8k),
  #121 version bump + manifest cleanup (only `[[build]]` remains) + ADR-0015
  doc alignment. 259 tests at release.
- **v3.1 complete but UNPUSHED on local main (4 commits, 321 tests, gate
  clean)**: #122 brain verbs `why`/`deadlocks`/`roster` (src/brain.rs;
  team resolved from CLAUDE_CODE_SESSION_ID vs leadSessionId), #123 hook
  enrichment via own-child socket post (src/lead_post.rs + src/enrich.rs;
  LIVE-verified delivery incl. verifiedPeerPid; pending-live items + runbook
  in docs/research/hook-socket-enrichment-2026-08-20.md §4–5), #124 sidebar
  5-state priority tokens ("!! needs-you" marked; reuses brain::five_state,
  single-source rule #90). Release = Caio's word + bump to 3.1.0 + tag.
- **2026-08-21 wave (local main, still unpushed)**: #125 CLOSED — auto-pump
  rewired into the hook path as `pump::auto_pump` (in-process debounce +
  detached `pump-board` child; hook never blocks). #126 CLOSED — asyncRewake
  spike CONFIRMED live (0.49s wake; delivery is an isMeta user turn, NOT a
  system reminder → injection surface; full findings on the issue). #127
  CLOSED — `docs/agents/boundary-pairs.md` + `check-boundary-seams.sh`
  (ADR-0004, brace-depth cfg(test) skip, mutation-proven) + Side D
  (self-spawn literals) in `check-subcommand-seams.sh`; both in CI. Two opus
  review rounds applied (commits 9b582a0, 4792349).
- **Open tickets**: #128/#129/#130 (needs-triage), #131 (TeammateIdle
  enrichment debounce — teardown idle-storms flood the lead, observed live).
- **Upstream**: posted the socket-seam findings on claude-code#88332
  (issuecomment-5362847742) — fact-checked against the evidence doc.
- **Team/process** (user-level, affects work here): agent roster in
  ~/.claude/agents/ (implementer/reviewer/researcher/scribe/miner) — full
  toolsets, SendMessage reporting, opus reviewer gate on every non-trivial
  diff (measured: it caught real bugs both waves). Playbook-skills design
  done, build pending — see ~/.claude/playbook-handoff-2026-08-21.md.

## Previous state (2026-08-20, superseded above)

- **ADR-0015 accepted (2026-08-20): feature re-baseline, "the lead is
  mission control."** Full evidence trail in
  `docs/research/feature-rebaseline-brainstorm-2026-08-20.md`. Summary:
  herdmates feeds the native lead instead of building parallel team UIs.
  Keep: shim + doctor, signal engine + hooks/spool, recorder (black-box
  framing), sidebar tokens (+ priority states later). New (v3.1+): hook
  enrichment via the documented own-child socket post into the lead,
  lead-facing skill verbs (`why`/`deadlocks`/`roster`). Demoted:
  inbox-write → break-glass only (lead-down fallback). Delete outright:
  pane-board TUI, focus pane cluster (focus_pane/focusfile/attention/
  audit/jump), legacy v1 (15 fenced modules + god/codex-prompting
  skills), skills/atomizer.
- **v2.3.1 plan is dead**; next release is the **v3.0.0 re-baseline**
  (deletions + #117 fix + docs), then v3.1+ brain layer. Work is
  ticketed on GitHub — `gh issue list`, the re-baseline tickets name
  their blockers.
- #118 closed wontfix per ADR-0015 (team-ambiguity dissolves
  structurally: no surface guesses "which team" anymore). #117 stays
  open, folded into the v3.0.0 wave (fix: wrap the `pane run` line in
  `sh -c '…'` — quoted words are never spell-corrected).
- Still unpushed on main: the pre-review commits (E2E evidence,
  ADR-0014 gate, cross-session research) + this session's docs. All
  ship together with v3.0.0 on Caio's word.
- Key research facts (verified 2026-08-20, citations in the brainstorm
  doc): own-child socket injection into a session is DOCUMENTED
  (`CLAUDE_CODE_MESSAGING_SOCKET`/`_TOKEN`); team config dirs are
  removed on clean session end, task dirs persist; teammates load
  project skills; in-process is the default teammate mode; no external
  injection API is coming (#88331/#88332 no traction). AMQ stack
  (amq/amq-squad/amq-noc) = pattern donor, not scope threat.

## How to resume

1. `gh issue list` — the re-baseline tickets; work blockers-first.
2. `docs/adr/0015-feature-rebaseline-lead-centric.md` — the new base.
3. `docs/research/feature-rebaseline-brainstorm-2026-08-20.md` — why.
4. ADR-0013 doctrine still applies (honesty, risk layering); its
   surface list is superseded by ADR-0015.
5. Implement each ticket in a fresh session/context (they are
   self-contained by design).

## Standing rules (unchanged)

- Pushes to `main` are releases: gate (fmt/clippy/tests + CI), bump
  manifest version on behavior change, tag. **No push without Caio's
  word.**
- Hooks resolve the bare `herdmates` binary via PATH — reinstall
  (`cargo install --path . --root ~/.local`) after src changes or live
  hooks run stale.
- New evidence → new ADR, ask Caio.
- Re-verify after upstream updates: re-snapshot the herdr API schema
  after any herdr update; re-run the shim E2E after any Claude Code
  update (see `skills/herdmates/SKILL.md` "Version discipline").
