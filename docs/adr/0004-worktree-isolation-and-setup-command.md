# ADR-0004: Per-worker worktree flag + team-level setup command

Status: accepted (2026-07-14)

## Context

Parallel writers collide in one tree; readers don't need isolation. Herdr has
native `worktree create`. Hard-won project traps must be encoded, not
remembered: fresh worktrees may need symlinks/deps preflight (e.g. limux's
ghostty symlink + `git update-index --skip-worktree`), and pane cwd set via a
prompt-level `cd` causes split-brain (relative writes leak into the launch
dir).

## Decision

- Per-worker `worktree = true|false` in the spec; default true for
  `role = "builder"`, false otherwise. `branch` required when true.
- Team-level `setup` command array runs inside each fresh worktree before the
  agent launches.
- Pane cwd is ALWAYS set at creation (herdr `--cwd`), never via prompt text.
- `team kill --remove-worktrees` refuses on a dirty worktree (salvage rule:
  killed workers' uncommitted state is inspectable evidence).

## Consequences

- Project preflight becomes config (`setup`), portable across teams.
- Mixed teams (isolated builders + shared-tree reviewers) are one spec file.
