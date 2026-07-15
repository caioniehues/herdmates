# Issue tracker: GitHub Issues

Published 2026-07-15: `caioniehues/herdr-agent-team` (public, marketplace
topic `herdr-plugin`). Issues and specs now live on GitHub Issues via the
`gh` CLI.

## Conventions

- One issue per ticket; `gh issue create --title "NN — <slug>" --body-file <md>`.
- Blocking edges as native GitHub blocking links (or a `Blocked by: #N`
  line in the body where links aren't available).
- Triage state via the label vocabulary in `triage-labels.md`
  (`needs-triage` / `needs-info` / `ready-for-agent` / `ready-for-human` /
  `wontfix`).
- Specs (PRDs) live in the repo under `docs/`; an issue links to its spec
  section rather than duplicating it.
- External PRs ARE a triage surface now — run `/triage` over incoming PRs
  and issues.

## History

The v1 build (tickets 01–18) ran on the pre-publish local tracker under
`.scratch/team-v1/` (gitignored, retained locally as history). All 18 were
closed before publish; nothing needed migration.

## When a skill says "publish to the issue tracker"

`gh issue create` in this repo.

## When a skill says "fetch the relevant ticket"

`gh issue view <number>` (the user will normally pass the number).
