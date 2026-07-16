# Handoff — north-star documentation session (2026-07-16)

**For:** a fresh session in `~/Projects/herdr-agent-team` running `/grill-with-docs` → `/to-spec`
to produce the herdmates north-star document.
**From:** a research-heavy session in `~/Projects/cmux-kde` (context exhausted; all findings
verified and filed). Read this first, then the research files in §4.

---

## 1. What the product is (owner's own words, decided this session)

Caio's year-vision is **agent mission control** (Nathan Flurry's July-2026 concept: progress
log, human-friendly artifacts, roadmap, ETA) — NOT a terminal emulator. Decision archived
(`~/.config/makerskills/decide/archive/2026-07-16-herdmates-vs-limux-focus.md`, revisit
2026-08-15): **limux Qt port paused; herdmates is the single vehicle**, sequence:

1. D1 agent board (sidebar tokens over team files) — in progress
2. teammux shim (fake `tmux` on PATH → herdr pane calls) — de-risked, in active development
3. rich mission-control surface (roadmap/ETA/artifacts) — local web reader over team files, later

Locked context: **all agents are Claude Code now** (Codex workers frozen at v1.1.0);
`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` is enabled for Caio; herdr's bash/CLI-driven model
is a feature, not a gap. The product pillars are: **native Claude Code teams support in
herdr, the teammux shim, and the full mission-control potential.**

## 2. Ground truth established this session (all triple-verified)

**Claude Code agent teams = two independent layers.** This is the load-bearing architecture
fact; every product decision hangs off it.

- **Coordination (universal, file-based):** teammates are full independent Claude Code
  instances coordinating via mailboxes `~/.claude/teams/{team}/inboxes/{agent}.json`,
  locked task files `~/.claude/tasks/{team}/` (tasks carry `blockedBy` edges — a real DAG
  with auto-unblock), team config `config.json`, and the native SendMessage tool. Works in
  ANY terminal. A file-reader observes real team behavior with zero agent cooperation.
- **Display (`teammateMode`, settings-file key only):** `in-process` (default since
  v2.1.179) | `auto` | `tmux` | `iterm2`. Split panes hardcoded to tmux+iTerm2; explicitly
  unsupported in Ghostty/VS Code/Windows Terminal. A PATH shim is the only third-party
  route (iTerm2 backend added v2.1.186 = the list does extend upstream; possible future ask).

**Shim contract (fully de-risked — ADR-0012's control-mode caveat is STALE):** all tmux
calls are discrete CLI argv, NO `-C` control mode — confirmed by (a) this repo's
`spike-tmux-verbs-2026-07-16/REPORT.md` 36-call live capture, (b) string/argv probes of
claude binary v2.1.211, (c) official docs. Spawn call:
`tmux split-window -d -t <pane> -h -l 70% -P -F '#{pane_id}' -- <cmd>`.
Verbs: split-window, send-keys, capture-pane, select-pane, new-session, new-window,
kill-pane, list-panes. Hardest work: `%N`↔herdr-pane-id translation table (herdr
`pane split` returns the new id synchronously = the `-P` semantics needed). 7 cosmetic
styling verbs can no-op; startup probes can return static constants. `HERDR_PANE_ID`
substitutes for `TMUX_PANE`.

**Competitive/landscape facts:** cmux (macOS) ships the same shim trick genericized across
4 orchestrators (claude-teams/omc/omx/omo) — genericize ours from day one. limux is not a
factor for this horizon (its tmux-compat ported the wrong half; live bridge exposes 19/90
methods; plugin system post-v1; GTK host mid-replacement). herdr's plugin model: panes are
always TUI subprocesses (no non-terminal plugin UI — D2's known gap); decoration via
`pane report-metadata` tokens + user-config sidebar rows; rich event vocabulary incl.
`pane.agent_status_changed`.

## 3. Mission-control feature research (what to build — feed to the grilling)

Full ranked report: `docs/research/mission-control-feature-landscape-2026-07-16.md`
(43-agent verified sweep across 12 tools: cmux, Conductor, Crystal, Vibe Kanban [sunsetting],
claude-squad, Sculptor, container-use, Agent-Monitor, Stargx, claude-view, native teams,
Mission Control). Headlines:

- **Whitespace nobody ships, and it's literally renderings of files we already read:**
  mailbox message graph; task-DAG rendering (the native `blockedBy` graph — closest real
  thing to Flurry's "roadmap"); **task-status-lag deadlock detection** (documented native
  bug → headline feature); orphan/dead-teammate detection; same-file-collision warning
  (novel hazard: native teams share ONE worktree, unlike every isolating competitor);
  black-box recorder (native `/resume` doesn't restore in-process teammates — an external
  reader is the only durable record after a lead crash); multi-team overview; semantic
  (team-event) push notifications.
- **v1 TUI spine (all single-row, poll-driven, no hooks):** agent overview + status glyphs
  + waiting-REASON tags (permission-prompt vs done vs hung — flagship; competitors' pain
  point) + flat task list with ready/blocked flags + mailbox tail + deadlock badges +
  context-window utilization bar + metadata row.
- **Web-later:** DAG graph, comm graph, cost charts, activity cards, full-text search.
- **Out-of-lane for a reader:** diff review (borrow git; shared worktree makes bespoke diff
  UI a mismatch) and plan-approval gating (needs hooks; a reader can only *surface*
  pending approvals). Field splits into monitors vs gates — choose identity deliberately.
- **ETA: nobody ships it;** Flurry himself caveats accuracy. Honest proxy instead:
  tasks-done/total from the DAG + per-task elapsed. Tier-3/flagged at most.
- **Beads vs native tasks:** native task tools already ARE the DAG (blockedBy, auto-unblock,
  ready detection, file locking) — read them for live supervision. Beads = complementary
  durable cross-session planning layer; only worth its briefing tax when cross-session
  memory becomes the bottleneck. Ship the native reader first.

## 4. Files to read in this repo (in order)

1. This handoff.
2. `docs/research/cmux-limux-herdr-comparison-2026-07-16.md` — three-way comparison, teams
   mechanism verification, custom-pane extensibility, herdmates implications.
3. `docs/research/mission-control-feature-landscape-2026-07-16.md` — tiered feature
   landscape + criticisms + gaps + build-order recommendation.
4. Existing project docs it must reconcile with: `docs/adr/0012-pivot-to-herdmates.md`
   (NOTE: its teammux control-mode caveat is stale, see §2), `spec.md`, `CONTEXT.md`,
   `HANDOFF.md`, `docs/research/cmux-comparative-2026-07-16/{REPORT.md,REPORT-features.md}`,
   `docs/research/spike-tmux-verbs-2026-07-16/REPORT.md`.

Cross-session memory (auto-loaded in `~/Projects/cmux-kde` sessions, NOT here — copy what
matters into this repo's docs): `~/.claude/projects/-home-caio-Projects-cmux-kde/memory/`
(agent-teams architecture, limux gaps, comparison pointer, cmux feature catalog).

## 5. Open product questions the grilling must settle (north star is mush without these)

1. **Monitor or gate?** Pure read-only observer, or may it WRITE (inbox JSON messages =
   steering/reply-from-dashboard; hook companion = actual gating)? The research's
   cross-cutting anti-pattern warning: don't pretend a reader can enforce.
2. **Single-team v1 or multi-team board?** Native is single-team-per-session; multi-team
   aggregation is unshipped whitespace but scope creep.
3. **ETA stance:** honest-proxy only, flagged experiment, or banned?
4. **Shim status:** still "quarantined upside" (ADR-0012 framing) or promoted to
   first-class pillar now that it's de-risked? Genericized for non-Claude orchestrators
   from day one, or Claude-only?
5. **Black-box recorder:** in scope for v1 (persist observed team state to survive lead
   crashes) or later? It's cheap and nobody positions there.
6. **TUI↔web boundary:** which tier-2 features justify starting the web view, and when?
   (Flurry thread's live debate: TUIs "not expressive enough" vs same-UI-applies.)
7. **Naming/identity:** is the mission control a herdmates surface, or does it outgrow
   herdr (local web app readable from any terminal) with herdr as one display adapter?
   §2's two-layer split says the reader core should not depend on herdr.
8. **Deadlock/status-lag detection thresholds:** what signals count (task lock state +
   teammate idle + mtime age) and what's the false-positive tolerance?

## 6. Working agreements that apply here (from Caio, this session)

- Worker/model tiering: haiku = mechanical, sonnet = research/execution, opus =
  judgment/synthesis, fable = second-opinion only; ALWAYS set model explicitly on spawned
  agents/workflows.
- Verify worker claims at ground truth; probe artifacts, not summaries.
- Caio gates state-changing steps; no pushes/PRs/issues without asking.
