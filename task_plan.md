# Task Plan: Waves 5+6 orchestration (god session)

## Goal
Close #22, #14, #17 (wave 5) then #23, #24, #25 (wave 6) via codex pane workers. Coordinator never writes code. One release per wave (0.7.0, 0.8.0), only on Caio's word.

## Current Phase
Phase 7

## Phases

### Phase 1: Wave 5a — worker G (#22 seam unification)
- [x] Write brief + team spec, spawn worker G
- [x] Monitor, review PR at diff, central gate, merge
- **Status:** complete

### Phase 2: Wave 5b — worker H (#14 + #17 spawn robustness)
- [x] Brief + spawn on top of merged G
- [x] Review, gate, merge
- **Status:** complete

### Phase 3: Wave 5 DoD + release 0.7.0
- [x] Live spawn test: 2-worker team, parallel launches, spawn --resume demo
- [x] Report to Caio; WAIT for release word; release + relink plugin
- **Status:** complete

### Phase 4: Wave 6 — workers I (#23+#24) and J (#25)
- [x] Spawn I; J after I's verb table exists
- [x] Review, gate, merge both
- **Status:** complete

### Phase 5: Wave 6 DoD + release 0.8.0
- [x] Live demos (team wait, inbox --unread, skill installs)
- [x] Report to Caio; WAIT for word; release + relink
- **Status:** complete

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| Sequential G then H | H reworks spawn.rs launch flow G touches |
| J briefed against I's PR verb table | #25 written against #23/#24 verbs |
| Workers never bump manifest version | Coordinator bumps at merge |

## Errors Encountered
| Error | Resolution |
|-------|------------|

### Phase 6: Wave 7 — workers K (#28+#29) and L (#16+#34)
- [x] Spawn K + L parallel
- [x] Review, gate (15x for K), merge; new wave6-style DoD probes on adopt recovery
- [x] Learnings doc, release 0.9.0 on Caio's word
- **Status:** complete

### Phase 7: Wave 8 — worker M (#8 socket backend)
- [x] Brief + spawn M (ADR-0011 scope, contract discipline, parity bar)
- [x] Review (adversarial pane review, 15x gate), merge
- [x] Live parity check per M's REPORT commands; write learnings
- [x] Teardown; release 1.0.0 on Caio's word
- **Status:** complete
