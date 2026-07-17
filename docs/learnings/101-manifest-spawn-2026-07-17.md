# Learnings — #101 manifest spawn fix + v2.1.0 release (2026-07-17)

- herdr resolves manifest command argv[0] via PATH ONLY — relative paths
  never spawn (proven #98, fixed here). Convention: bare `herdmates`,
  installed by `cargo install --path . --root ~/.local`.
- `~/.cargo/bin` is NOT on PATH on this machine and CARGO_HOME is unset —
  the plain-`cargo install` default target would have produced a
  non-resolvable binary. `~/.local/bin` is the machine's real user-scoped
  seat (herdr's own binary lives there). Builder checked BEFORE
  installing — hypothesis-labeling in the brief ("NOT yet proven — that's
  M1") is what made the check happen.
- herdr spawns argv directly, no shell: `~` in a manifest command array
  is a literal path. Anything needing expansion goes through the
  manifest's existing `sh -c` convention.
- Live-verify count: 2 (spike form + committed form), both
  `plugin_pane_opened`. The pane's subsequent resolve_team
  ambiguous-team exit (13 team dirs) is designed honesty, carried.
- Released v2.1.0: 11 commits 43aceec..80d7260 pushed, tag v2.1.0.
