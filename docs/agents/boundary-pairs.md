# Boundary Pairs

Seams in this repo where two sides — a producer and a consumer — must agree
on a shape neither the compiler nor a single-file review checks. Issue #127:
the #121 stale-wiring bug (manifest advertised subcommands `main.rs` no
longer dispatched) is this class. A reviewer existence-checking one side
("does the field exist?") misses drift; only cross-comparing both sides
catches it. One section per pair below: producer, consumer, what drift looks
like, how to cross-compare, and which check (if any) is mechanical.

## herdr CLI verb <-> teammux translation table

- **Producer**: the real `claude` process's tmux invocations — ground truth
  recorded in `docs/research/spike-tmux-verbs-2026-07-16/REPORT.md` (18 verb
  shapes, verb -> herdr mapping table).
- **Consumer**: `src/tmuxargs.rs`'s `Verb` enum (what teammux parses) and
  `src/teammux.rs`'s `dispatch` match (what teammux does with each variant —
  a live herdr call, a static-fake, or a documented no-op).
- **Drift looks like**: a verb shape Claude Code emits that `tmuxargs::parse`
  doesn't recognize (silently falls to the generic "unrecognized verb" error
  at runtime, never caught by tests), or a `Verb` variant with no arm in
  `dispatch` (falls to the catch-all `other => Error` arm — currently a loud
  failure, not a silent one, but still a live gap nobody flagged).
- **Cross-compare**: walk REPORT.md's 18-row table against `Verb`'s variants
  AND `dispatch`'s match arms together — a variant that parses but isn't
  dispatched is just as broken as a verb that never parses. Don't stop at
  "does `Verb` have 18 variants"; confirm each REPORT.md verb shape maps to
  one specific variant + one specific dispatch arm, using `teammux.rs`'s own
  module doc comment (it narrates which commit added which verb) as the
  index.
- **Check**: manual — reviewer brief only. The correspondence is prose-table
  row <-> enum variant <-> match arm, three shapes with no shared
  vocabulary a script could line up without becoming a hand-maintained
  duplicate of the mapping itself (which is exactly the drift risk it would
  be checking). A new spike (re-running the recon capture) is the right
  mechanical companion if the verb set is ever suspected to have grown —
  not a static grep against a frozen report.

## teamfiles schema <-> consumers (gather/brain/enrich)

- **Producer**: `src/teamfiles.rs` — `TeamConfig`, `Member`, `InboxMessage`,
  `Teammate`, the on-disk shapes read from `~/.claude/teams/{team}/
  config.json` and `inboxes/*.json` (see `CONTEXT.md`'s Team files entry).
- **Consumer**: `src/gather.rs`, `src/brain.rs`, `src/enrich.rs` — each reads
  fields off those structs to build its own view (gather assembles raw
  state, brain derives higher-level signal, enrich annotates it further).
- **Drift looks like**: a field renamed or removed in `teamfiles.rs` that a
  consumer still references by its old name/shape (compiler catches a
  renamed struct field, but NOT a field that changed meaning while keeping
  its name/type — e.g. `agent_status` going from "raw string" to "enum-like
  string set" upstream in herdr without a consumer's parsing logic updating
  to match), or a new field a consumer should start reading but doesn't
  (silent staleness, not a build error).
- **Cross-compare**: for each `teamfiles.rs` struct, list every consumer
  file's read of that struct's fields (`rg 'member\.' src/gather.rs
  src/brain.rs src/enrich.rs` style) and confirm the *meaning* each consumer
  assumes still matches the producer's current doc comment — the compiler
  guarantees type agreement, never semantic agreement.
- **Check**: manual — reviewer brief only. Field existence is compiler-
  enforced already (Rust rejects a renamed/missing field at every call
  site); the only real drift risk here is semantic (meaning changed, name
  didn't), which needs a human reading both the producer's doc comment and
  the consumer's usage — not a grep target.

## herdr-plugin.toml commands <-> main.rs arms

- **Producer**: `src/main.rs`'s dispatch match arms (what the binary
  actually runs) and its usage string (what it tells a human).
- **Consumer**: `herdr-plugin.toml`'s `command = [...]` arrays (what herdr
  is told it may invoke).
- **Drift looks like**: the #121 bug — the manifest kept advertising
  `spawn`/`status`/`kill`/`board`/`open-report`/`on-agent-status` for a
  whole commit after the arms were deleted.
- **Cross-compare**: already covered — see `scripts/check-subcommand-seams.sh`.
  Don't duplicate its Side A/B/C comparison here; reference it.
- **Fourth side** (review finding, 2026-08-21): in-code self-invocations —
  `pump.rs::auto_pump` spawns `current_exe` + `"pump-board"`. A renamed arm
  silently disables that caller (stderr nulled, spawn Result discarded), so
  the script's Side D cross-checks every `.arg()` literal near a
  `current_exe` spawn against the live arms.
- **Check**: `scripts/check-subcommand-seams.sh` (wired into CI already).

## tokens.rs published names <-> sidebar-rows.toml `$name` rows

- **Producer**: `src/tokens.rs`'s `Token { name: … }` literals — the names
  the board pump publishes via `pane report-metadata --token`.
- **Consumer**: `docs/sidebar-rows.toml`'s `[ui.sidebar.agents] rows`, the
  table a user merges into `~/.config/herdr/config.toml`.
- **Drift looks like**: `$model` was published from the first board wave and
  had no row for months — the token reached herdr and rendered NOWHERE, with
  the pump, `pane list` and every test looking healthy. Found 2026-09-03 by
  the check below on its first run, not by review.
- **Both directions are silent**, which is what makes this a real seam rather
  than a naming convention: a published token with no row is invisible, and a
  row naming no producer makes `herdr server reload-config` return `"partial"`
  and keep the OLD layout — no crash, no error, edits just stop taking
  effect. The second failure is worse, because it disables rows that *were*
  working.
- **Cross-compare, never existence-check**: the presence of a `$reap` row
  proves nothing about whether anything publishes `reap`, and vice versa.
- **Check**: `scripts/check-token-seams.sh` (wired into CI). Mutation-proven
  in three directions — missing row, phantom row, and a producer shape change
  (which fails with a misleading tag, documented in the script's header).

## Doc staleness (commit-count heuristic)

- **Producer**: source files that describe behavior a doc depends on (e.g.
  `src/teammux.rs`'s verb dispatch, `src/teamfiles.rs`'s schema).
- **Consumer**: the docs that narrate that behavior for humans/agents —
  `CONTEXT.md`, `docs/adr/*.md`, `docs/agents/*.md`, module-level `//!` doc
  comments.
- **Drift looks like**: a source file collects many commits touching its
  behavior while the doc describing that behavior gets none — the doc is
  quietly falling behind, with no compiler or test to notice.
- **Cross-compare**: `git log --oneline -- <src file>` vs `git log --oneline
  -- <doc file>` over the same window; a large gap in commit *recency*
  (not just count) between a source file and the doc that depends on it is
  the signal worth a human look.
- **Check**: manual — reviewer brief only. A commit-count/recency gap is a
  prompt to go *read* the doc against the current code, never a pass/fail
  on its own — a doc can go 50 commits untouched because nothing it
  describes changed, and one commit can invalidate it entirely. Automating
  the threshold would just move the false-positive/false-negative judgment
  call into the script instead of removing it.

## ADR-0004: cd-in-prompt-text anti-pattern

- **Producer**: ADR-0004's decision — "Pane cwd is ALWAYS set at creation
  (herdr `--cwd`), never via prompt text" — because a prompt-level `cd`
  causes split-brain (relative writes leak into the launch dir, since the
  pane's real cwd never moved).
- **Consumer**: every place herdmates constructs a command string that gets
  submitted to a pane as its prompt/input (`pane_run`, `lead_command_line`,
  `respawn_pane`'s command argument) — these must never build `cd <path>
  && ...` themselves; cwd must already be set by `--cwd` at
  `pane_split`/`workspace_create` time.
- **Drift looks like**: new code composing a launch command as `cd X &&
  <rest>` instead of relying on `--cwd`, reintroducing the split-brain bug
  the ADR closed.
- **Cross-compare**: n/a — this seam has one side that can actually violate
  it (herdmates' own command-construction code); the other side (the ADR
  decision) doesn't drift. A one-sided grep is legitimate here specifically
  because the "other side" is a fixed decision record, not a second moving
  implementation.
- **Check**: `scripts/check-boundary-seams.sh` — greps `src/*.rs` production
  code (skipping `#[cfg(test)]` modules and comments, since test fixtures
  legitimately echo Claude Code's *own* tmux calls, e.g.
  `teammux.rs`'s `respawn_pane` tests, which record what the external
  process sends, not what herdmates constructs) for a string literal
  shaped like `cd <path> && ...`.
