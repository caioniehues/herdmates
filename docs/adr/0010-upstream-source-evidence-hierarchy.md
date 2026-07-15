# ADR-0010: Upstream herdr source is available evidence; documented evidence hierarchy

Status: accepted (2026-07-15, grilling interview with Caio)

## Context

Since inception this project carried the claim "herdr is closed-source" in
HANDOFF.md. It was an assumption, never verified. On 2026-07-15 one
`gh repo view` disproved it: herdr is open source at
https://github.com/ogulcancelik/herdr (Rust core with a vendored Zig
`libghostty-vt` terminal engine; ~16.5k stars; active). The project had been
running blind — guessing at event payloads and protocol behavior it could
have read in source — for no reason.

Four research reports (2026-07-15, `docs/research/`) were produced from the
upstream checkout (`~/Projects/herdr-upstream`) and the wider ecosystem:

- `upstream-architecture-claims-2026-07-15.md` — architecture map + audit of
  our "verified facts" against source.
- `upstream-integration-opportunities-2026-07-15.md` — full surface sweep.
- `herdr-claude-teams-analysis-2026-07-15.md` — competitor tmux-shim review.
- `awesome-herdr-2026-07-15.md` — 133-entry ecosystem survey.

The audit surfaced attribution errors: behaviors we live-verified were
correct, but *where they are implemented* was sometimes wrong (Enter
"paste-debounce swallowing" and mid-turn queueing live in the agent TUIs,
not herdr; herdr's `pane run` is one request carrying text + Enter). It also
surfaced that the upstream checkout contains **preview** surface
(`docs/next`, metadata `tokens`) that our installed herdr 0.7.3 / protocol-16
runtime may not have (our snapshot has `custom_status`; current source does
not).

## Decision

1. **Evidence hierarchy** for every factual claim in this repo's docs:
   - **Live behavior** on the installed herdr is decisive for *what
     happens* (behavior claims). A live test outranks source reading when
     they disagree about observable behavior on our runtime.
   - **Upstream source** (pinned checkout) is decisive for *why and where*
     (attribution claims: which component implements a behavior, what the
     full surface is).
   - **Preview surface** (anything in upstream source/`docs/next` not
     confirmed in the installed runtime) may only enter docs as
     schema-gated / feature-detected — never as an assumed fact.
2. **Facts are tagged** with their authority — `live`, `source`, or
   `preview` — plus date, in spec §9 and CLAUDE.md.
3. **Spec stays lean** (contract only); the research reports under
   `docs/research/` are the permanent evidence/reference layer, cited by
   file and section. Dense upstream detail (full event list, env-var
   matrix, CLI tree, IPC framing) is NOT duplicated into the spec.
4. **ADR-0001 and ADR-0005 decisions stand** under corrected reasoning:
   - ADR-0001's schema-snapshot discipline survives as *drift detection*
     against upstream releases (diff on `herdr update`), no longer as the
     only window into a closed binary.
   - ADR-0005's copy-don't-depend survives because herdr is a **binary
     crate with private modules** — there is nothing to link against
     (independently confirmed by the competitor analysis). Porting logic by
     copying remains correct.
5. **Local clone convention:** `~/Projects/herdr-upstream` (shallow, re-pull
   before source citations in new work). Research rules in
   `docs/agents/research.md` govern how new external facts get verified
   (ctx7 first, source second, live decisive).

## Consequences

- Inherited doc claims are hypotheses until probed (the never-assume rule,
  `docs/agents/research.md`).
- Spec §9 entries carry authority tags; conflicting evidence resolves by
  lane (live wins behavior, source wins attribution).
- Preview-derived features (metadata tokens, sidebar token rows) require a
  runtime schema probe before use; roadmap items must not assume them.
- Future herdr updates: re-snapshot `herdr api schema --json`, diff, and
  re-pull the upstream clone; discrepancies get triaged by the hierarchy.
