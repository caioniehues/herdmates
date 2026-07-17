# Findings — hook-correctness wave (2026-07-15)

## 2026-07-15 coordinator takeover — Wave 8

- `.scratch/codex-coordinator-handoff.md` was read fully. Repo-root
  `task_plan.md` is the current Wave 8 plan (Phase 7 in progress); the
  `.planning/.active_plan` pointer still names a completed documentation plan.
- PR #38 is already centrally gated (fmt, clippy, 168 tests, 15/15 repeated
  full-suite runs) and live socket/CLI parity was verified. The remaining
  pre-merge requirement is a visible Codex adversarial review written to
  `.scratch/pr38-review.md`, followed by fix/re-gate/re-review if needed.
- Coordinator boundary: no code implementation; all review and implementation
  labor goes through visible Herdr panes. No merge or push to main until the
  prescribed review is clean; release/version/tag/push additionally requires
  Caio's explicit release word.
- Visible two-axis PR #38 review completed in panes `w1A:pA` (Standards) and
  `w1A:pB` (Spec). Authoritative reports:
  `.scratch/pr38-standards-review.md` and `.scratch/pr38-spec-review.md`.
- Spec verdict: 7 findings (HIGH: missing socket I/O deadlines; doubled wait
  timeout on fallback; board/wait discard socket snapshot/subscription state;
  trace may leak server-controlled message text. MEDIUM: unbounded/hot-spin
  reconnect policy; raw `serde_json::Value` bypasses typed validation; runtime
  schema is never checked against `herdr api schema --json`). Mutation routing
  and durable run.toml/inbox verdict truth passed.
- Standards independently confirmed runtime-schema validation as a hard
  contract breach and flagged raw JSON DTOs, duplicated subscribe framing, and
  the oversized multi-responsibility socket module as judgement-call smells.

## Spawn died midway (needs root-cause later)

Run `hook-wave-1784101002586`: spawn created run dir, protocols, both
worktrees, both workspaces/panes, launched codex in A's pane — then stopped.
B's codex never launched; both lifecycles stayed `pending`; no
herdr-agent-team process alive afterward. Cause unknown (user-ran command;
possibly interrupted, possibly a crash between A's `agent wait --status idle`
and brief submission). If it reproduces, file an issue: spawn should either
roll back or leave a resumable marker; `pending` + dead spawn = silent
half-team.

Salvage procedure that worked (mirror of spawn.rs launch_and_brief_worker):
1. `herdr pane run <pane> "Read your brief at <abs-brief> and execute it
   fully. The repository's authored AGENTS.md remains in effect. Read the
   generated team protocol at <run>/protocols/<worker>.md."`
2. `herdr wait agent-status <pane> --status working --timeout 30000`
3. Set worker `lifecycle = "running"` in run.toml.
(For B: first `pane run <pane> "codex --dangerously-bypass-approvals-and-sandbox"`,
wait idle, then steps 1–3.)

## Contract changes this wave (Caio)

- Worktree workers RUN GIT: commit own branch, push, open PR via gh.
  Coordinator reviews/gates/merges; merge to main = release, Caio's word only.
- Codex launch flag stays `--dangerously-bypass-approvals-and-sandbox`
  (no `--yolo` alias in codex 0.144.4 — checked --help live).
- agents.toml override already installed at
  ~/.config/herdr/plugins/config/caioniehues.agent-team/agents.toml.

## Key refs

- Run dir: ~/.local/state/herdr/plugins/caioniehues.agent-team/runs/hook-wave-1784101002586
- Panes: A=wW:p1 (fix/hook-transitions), B=wY:p1 (fix/kill-adopt-robustness)
- Monitors: background waits bcmqoija2 (A), b5ke0uosp (B), match
  "READY FOR REVIEW", 1h bound
- #3 decision: --team with active run = hard error; --team + --run = error too
- Sentinel: `READY FOR REVIEW: <pr-url>`; workers ping via msg every ~10 min

## Monitoring lessons (this wave)

- `herdr wait output --match` matches EXISTING scrollback too — a re-armed
  sentinel wait after a first completion fires instantly on the old line.
  For re-work rounds: rely on hook pointer injection (works for unseen/background
  completions) + fallback `herdr wait agent-status <pane> --status done`
  (B's pane is unseen, so completion = done; NEVER do this for watched panes).
- Hook pointer injection confirmed working for background workers pre-#10:
  "kill-b is done" pointer landed in god pane on B's first completion.

## Sentinel-string trap (twice)

Never put the literal sentinel string in msg text or anything echoed into the
worker pane — `wait output --match` scans scrollback, so the wait fires on the
instruction itself. Sentinel must only ever be printable by the worker at
completion. Brief-by-file-pointer is safe; inline msg is not. After first
completion, re-armed output waits also match the old sentinel — for re-work
rounds use unseen-completion status waits (`wait agent-status --status done`)
or a round-numbered sentinel (READY FOR REVIEW R2).

## Worker A protocol-vs-brief conflict

A finished green (107 tests) but skipped its brief's git contract because the
generated worker protocol still encodes "workers never run git" (old rule).
Protocol template needs updating if the new git contract stays — candidate
follow-up issue after this wave.

## Failure-idle pointer (wave 3, worker E)

"is done" pointer fired but report missing + no PR: codex died on "Selected
model is at capacity" mid-task and dropped to idle. Injection was honest
(status transition) but transition ≠ completion (#1217 discipline held —
report file is the truth; checked it BEFORE trusting the pointer). Salvage
check: worktree clean, no commits, early-stage death → simple resume msg
worked. Pattern: on every completion pointer, verify report exists before
review; missing report + idle pane = inspect pane scrollback for agent-side
errors (capacity, auth, crash), then msg-resume.

## PR #12 review round 1 (B)

- Finding: end_worker_lifecycles overwrote Failed -> Ended/Released, reversing
  deliberate 3b8d0c6 behavior (failure diagnostics lost). Sent fix request via
  msg: Failed = terminal, never overwrite; keep error-tolerance; restore test.
- Rest clean: kill tolerance per brief, adopt --team semantics + 3 tests exact,
  spec §12 amended. Gate: fmt/clippy/tests green, 102 tests (baseline 98).
