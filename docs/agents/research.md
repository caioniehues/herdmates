# Research Rules

How the engineering skills (`/research`, `/grill-with-docs`, `/diagnosing-bugs`, `/implement`, and any worker briefed for reading) must gather external facts.

## Always use ctx7 (via the find-docs skill) first

For **any external code repository, library, framework, or its documentation**:

1. **ctx7 CLI first** (through the `find-docs` skill) — current API surface, signatures, options, examples. Never the Context7 MCP server; never training data.
2. **Upstream source second** — when ctx7 lacks the answer or the question is about behavior, clone/read the actual repo. Herdr itself is open source: https://github.com/ogulcancelik/herdr (local clone convention: `~/Projects/herdr-upstream`).
3. **Live behavior last and decisive** — a running test against the real binary beats both when they disagree; record the result in spec/ADRs.

## Never assume — verify

Inherited claims (handoffs, old docs, past-session notes) are **hypotheses, not facts**. Any load-bearing claim gets a cheap verification probe (one `gh repo view`, one `--help`, one ctx7 lookup) before work builds on it.

Origin: 2026-07-15 — "herdr is closed-source" lived unverified in HANDOFF.md across sessions and was false. Cost: the project ran without upstream source as ground truth.

## Where findings go

- API/behavior facts that lock decisions → spec or a new ADR.
- Research reports → `docs/research/<topic>-<date>.md`, cited.
- Never paste raw fetched content into planning files; distill.
