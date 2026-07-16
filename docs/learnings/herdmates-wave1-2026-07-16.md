# Learnings — herdmates wave 1 (pivot execution), 2026-07-16

Wave scope: ADR-0012 pivot decided + executed same day — v1.1.0 tombstone
released, repo renamed, #84 (D1 board) built and merged, #85 (shim) mid-
flight, #86 (D3 focus pane) filed + dispatched, two research reports
archived. Records: `.planning/2026-07-16-herdmates-wave-1/`,
`docs/research/spike-tmux-verbs-2026-07-16/`,
`docs/research/cmux-comparative-2026-07-16/`.

## Process learnings

- **Grill → ADR → tiny-commit issues → pane workers with per-step
  coordinator commits** ran a full pivot (decision to merged feature) in
  one day. The per-step commit gate caught nothing broken all wave —
  because workers ran the gates first; keep both layers anyway (trust,
  verify).
- **Prototype before building D1 paid off 4×:** the throwaway sidebar
  script surfaced agent-less-pane invisibility, the `state_text` token
  name, the partial-reload trap, and the ~20-char practical truncation —
  all folded into the shipped docs. One hour, four design corrections.
- **Recon-first spike beat build-first:** the logging-wrapper inventory
  (18 verbs, no control mode) made #85 a mapping exercise instead of an
  emulator. BUT the comparative research (cmux) showed the capture was
  environment-blind — geometry format-string queries never appeared
  because real tmux answered them invisibly. Lesson: **a recon run under
  a real implementation hides the verbs the real implementation absorbs**;
  always cross-check against prior art.
- **Comparative research mid-wave changed the design cheaply.** cmux
  (manaflow-ai/cmux) ships our exact shim in production: fake
  `TMUX`/`TMUX_PANE` env (no real tmux), single `__tmux-compat`
  dispatcher reused across 5 agent CLIs, geometry answered from host
  state. Three corrections adopted while the worker was between commits —
  cost: one message. Their claude-teams is still *nightly-only* with a
  known mutual-exec-loop bug: shim hardening is real scope, not polish.
- **Worker-push beats coordinator-polling.** Workers now run
  `herdr pane run <coordinator-pane> "<WORKER>: STEP n READY — read
  <path>"` at boundaries; signals arrive as queued user messages
  mid-turn. Watches remain ONLY for `blocked`/crash (a blocked worker
  can't push). Verified live same day.
- **Friction journal habit (Caio directive):** every worker question,
  block, or swallowed message = a coordination defect; fix AND record in
  the brief-template lessons memory immediately.

## Herdr facts learned this wave (beyond ADR-0012's list)

- `pane move` within the same tab is a **no-op** (`changed:false`,
  reason `same_tab`); tree restructuring needs `layout.apply` /
  `layout.set_split_ratio` — **socket-only, no CLI wrapper** (schema
  consts confirm). Cross-tab `pane move --new-tab` + `--split
  --target-pane --ratio` works and **pane IDs survive** same-workspace
  tab moves.
- Background-tab completions report `done` (unseen), not `idle` — never
  key a fallback watch on `idle` for panes parked in a background tab.
- Sidebar: `ui.sidebar_width` (default 26) / `ui.sidebar_min_width` /
  `ui.sidebar_max_width` (default 36). Builtin row token is
  `state_text`; an unknown token makes `reload-config` return status
  `partial` and silently keep the old UI.
- `herdr wait agent-status --status idle` watches die silently when the
  pane flips `blocked` — arm a parallel blocked-wait for workers.

## Worker traps (new this wave; full list in memory worker-brief-template-lessons)

- Fresh claude pane **swallows the first `pane run` brief** reliably
  enough to always verify (token count >0 / non-empty prompt) before
  trusting a working-wait.
- Worker worktrees branched mid-wave lag main docs: brief must include
  "read newer docs via `git show <commit>:<path>`, never rebase" — a
  worker burned its block on exactly this question.
- `claude --teammate-mode` flag does NOT exist (2.1.211): settings key
  only (`--settings '{"teammateMode":"tmux"}'`).

## Coordination mechanics that worked

- Warm-pane reuse across missions (spike pane ran 3 briefs: tmux recon →
  cmux architecture → cmux product survey) — context compounds, spawn
  cost paid once. Worker cwd is fixed at pane creation, so REUSE only
  works when the new task lives in the same directory; otherwise fresh
  pane (ADR-0004).
- Rebasing a warm worker onto a new branch: coordinator does the
  checkout in the worker's worktree (worker never touches git), archive
  old PROGRESS, rewrite BRIEF, message. Worker keeps all its codebase
  context.
- Sidebar tokens as fleet board (`task=`/`step=` per worker pane,
  updated at each boundary) — D1's design, dogfooded all day.

## Pitfalls hit (all in plan error table)

- `git tag` chained after a piped `git merge | tail` tagged a pre-merge
  commit (pipe swallowed the failure). Never chain `&&` off a pipeline
  whose left side must succeed.
- CONTEXT.md pivot rewrite conflicted with Stage 3 vocabulary from the
  integrate branch — resolved semantically (grafted 2 terms), not
  take-ours.
