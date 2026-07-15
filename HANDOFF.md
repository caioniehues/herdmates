# HANDOFF — next session orientation

Last updated 2026-07-15 (post research wave + docs overhaul).

## Read first

1. `docs/spec.md` — v1 spec; §8 = post-v1 roadmap (research-backed wave),
   §9 = authority-tagged verified facts.
2. `docs/adr/0001–0011` — locked decisions + why. New evidence → new ADR,
   ask Caio first. ADR-0010 (evidence hierarchy) and ADR-0011 (socket
   backend) are the newest.
3. `CONTEXT.md` — vocabulary. `docs/agents/research.md` — research rules
   (ctx7 first; never assume; verify inherited claims).

## State

- **v1 SHIPPED + PUBLISHED** (2026-07-15, Caio's go-ahead):
  https://github.com/caioniehues/herdr-agent-team — public, topic
  `herdr-plugin`, tag `v0.1.0`, marketplace-listed. Pushes to `main` are
  releases: gate (fmt/clippy/tests), bump manifest version for behavior
  changes, tag, never push without Caio's ask.
- DoD passed 2026-07-15 (run 2, limux repo, live): spawn, worktrees,
  pointer injection, msg round-trip, kill preserving dirty worktree.
- `team adopt` shipped (`10a855a`, closes #1): existing panes become full
  workers; ADR-0009, spec §12.
- **Herdr is OPEN SOURCE** — github.com/ogulcancelik/herdr (Rust core,
  vendored Zig libghostty-vt). The old "closed-source" note was an
  unverified assumption, corrected 2026-07-15 (ADR-0010). Local clone:
  `~/Projects/herdr-upstream`. Schema-snapshot discipline stays as drift
  detection (`docs/herdr-api-schema.snapshot.json`, protocol 16).
- **Research wave 2026-07-15** (4 reports in `docs/research/`): upstream
  architecture + claims audit, integration opportunities, herdr-claude-teams
  competitor analysis (verdict: pattern-source, not threat), awesome-herdr
  ecosystem survey (133 entries). Key corrections live in spec §9; roadmap
  rewritten in spec §8 from this evidence (grilled decisions Q1–Q6 with
  Caio, 2026-07-15).
- Central gate green at last commit: build, fmt, clippy `-D warnings`,
  98 tests.

## NEXT steps (in order)

0. **Hook-correctness wave DONE on `integration/hook-wave`** (2026-07-15,
   PRs #12 + #13, gate green 114 tests, v0.3.0, all four issues live-verified
   incl. the exact #10 watched-worker injection scenario) — **awaiting Caio's
   word to merge to main/push/tag/close #4 #10 #11 #3**. Follow-up issue
   candidates in task_plan.md observations: spawn dies midway leaving
   `pending` lifecycles; generated protocol still says workers-never-git
   (conflicts with 2026-07-15 git contract: workers commit/push/PR, god
   merges); manifest changes need plugin unlink+link (disable/enable caches).
1. **Roadmap step 2 / Issue #5:** persist full `agent_session
   {source,agent,kind,value}` + herdr session identity.
3. **Roadmap step 3 / Issue #6:** schema-gated metadata tokens (spec §8 step
   3) + aggregate notifications.
4. **Roadmap step 4 / Issue #7:** native board pane (`[[panes]]` + action +
   keybinds + link handler).
5. **Roadmap step 5 / Issue #8:** direct socket backend behind `HerdrApi`
   (ADR-0011); #2 team wait rides it.
6. **Roadmap step 6 / Issue #9:** run-scoped broadcast, bounded previews, and
   conservative restart (blocked by #5).
7. **Roadmap step 7:** later/optional declarative layouts, Kitty-graphics
   board enrichment, run-history browsing, tested opencode/gemini launchers,
   and limux backend extraction.

Work them via codex pane workers (never implement in this repo from the
coordinator — memory rule), one ticket per worker worktree, coordinator
commits and gates centrally.

## Context that doesn't fit the docs

- Marketplace survey (175 plugins) + awesome-herdr survey conclusions:
  `docs/marketplace-notes.md`; raw verdicts in the two survey JSON/report
  files. Competitive watch: herdr-factory, dual-author, herdr-orchestrator,
  Shepherd, herdr-symphony, herdr-factory-loop-skill, herdr-claude-teams.
- Caio runs god sessions inside herdr; research/analysis fan-outs run as
  **visible herdr pane teammates** (codex yolo), never invisible Agent-tool
  subagents (2026-07-15 incident: mailbox-spawned agents never started).
- Watch item: optional Claude-native visible-team compatibility mode
  (herdr-claude-teams proved feasibility) — separate experiment, never core.
