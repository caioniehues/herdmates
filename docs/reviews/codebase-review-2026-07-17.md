# Codebase review — 2026-07-17 (post-v2.1.0, whole repo)

Baseline: local main 23e4f21 (v2.1.0 released + #102 unpushed), 450 tests
green.

Method: 3 Fable finder agents, one per axis — (A) correctness +
concurrency + filesystem robustness on the active v2 surface, (B)
complexity + architecture over all of src/ including a light frozen-legacy
scan, (C) test quality + docs/manifest consistency — followed by 2 sonnet
adversarial verifiers that independently re-derived every claim from
source (instructed to REFUTE, not confirm). 12 raw findings, 11 after
dedup (finders A and B independently hit the inbox `.lock`), verdicts:
**10 CONFIRMED, 0 REFUTED, 1 reframed by verification into a live bug**
(worse than originally claimed). Severity ranking below is
post-verification; two severities were corrected upward by verifiers.

---

## 1. HIGH — `src/teamfiles.rs:88` — stale inbox schema is a live display bug in `jump`

**Category:** correctness / dead-schema (finder B; reframed by verifier)

**Summary:** The same on-disk inbox-entry JSON file has three independent
Rust struct definitions across three modules, and the one still wired into
the `jump` attention path uses a schema that predates the live capture —
so real messages render as empty text from `"unknown"`.

**Full description:** Three structs model the identical file:
- `teamfiles::InboxMessage` (teamfiles.rs:88-95) — `fromAgentId` /
  `toAgentId` / `content`, all `Option`;
- `gather::InboxEntryWire` (gather.rs:466-478) — `from` / `text` /
  `timestamp` / `read`, matching the live-verified schema from
  docs/research/teammux-e2e-2026-07-16;
- `inbox_write::InboxEntry` (inbox_write.rs:50-61) — the write side,
  correct (`from`/`text`/`timestamp`/`msgV`/`msg_id`/`type`/`read`).

`gather.rs:460-464`'s own comment admits the problem: "This is a distinct,
more complete shape than `teamfiles::InboxMessage` ... that struct
predates the live capture ... flagged as a future cleanup, not fixed in
this pass." Because `InboxMessage`'s fields are all-optional and serde
silently ignores unknown JSON fields, a real entry `{from, text,
timestamp, msgV, msg_id, type, read}` deserializes SUCCESSFULLY with every
field `None` — no error, silent garbage.

**Verifier correction (this is why it ranks #1):** the finder called it
"inert until first future use" — wrong. `.inbox` IS read today:
`attention.rs:96-104` reads `lead.inbox` through the stale struct, pulling
`message.content` (→ `None` → default `""`) and `message.from_agent_id`
(→ `None` → default `"unknown"`), and `build_attention_queue` is wired
live via `jump.rs:155-167` (`merge_team_queues`) → `main.rs:38` `jump`
subcommand. Data flow: `pump.rs:159 read_inboxes` →
`teamfiles::build_teammates` → `Teammate.inbox` → attention queue. So
every real inbox message that reaches the jump attention view renders as
`"unknown: "` (empty text) — a live #92 honesty-doctrine violation
(silent-wrong output, not honest degradation).

**Verifier verdict:** PLAUSIBLE for the claim-as-written ("dormant"), with
the corrected claim CONFIRMED against source (attention.rs:96-104,
jump.rs:155-167, main.rs:38 all independently read). Severity raised
medium → high: "it's not dormant."

**Fix:** delete `teamfiles::InboxMessage`, point `pump.rs:159` at
`gather::InboxEntryWire` (single canonical read-side struct). ~15 lines
removed net.

---

## 2. HIGH — `src/inbox_write.rs:165` — orphanable sidecar `.lock`, no staleness recovery

**Category:** crash-safety (found independently by finders A and B —
deduped)

**Summary:** `acquire_lock`'s sidecar `.lock` file has no staleness/TTL
detection, and cleanup only happens in `LockGuard::drop` — a killed
process orphans the lock and permanently breaks the nudge affordance for
that inbox.

**Full description:** `acquire_lock` (inbox_write.rs:165-188) is pure
`OpenOptions::create_new` + 20×50ms retry, with zero mtime/age check on an
existing lock file. The only cleanup path is
`impl Drop for LockGuard { fn drop(&mut self) { let _ =
std::fs::remove_file(&self.0); } }` (inbox_write.rs:159-163). A SIGKILL
skips `Drop` entirely. Realistic kill vectors in this system: `herdr pane
close` on a teammate mid-write, OOM kill, panic-abort. After the orphan,
every subsequent `append_entry` to that inbox retries the full 1-second
window then returns `InboxWriteError::LockContention` — forever, silently,
until a human manually deletes the stray file. Not a self-healing honest
error; a silent-until-discovered outage of the nudge feature. The only
lock test (`acquire_lock_times_out_when_the_lock_file_is_already_held`)
covers a live holder, not an orphaned lock — the crash path has zero
coverage.

**The kicker (finder B):** the crate already ships the correct primitive.
`fs4 = { version = "1.1", features = ["sync"] }` is a live dependency
(Cargo.toml:11) providing OS advisory file locks that the kernel releases
automatically on process death, and it is already used for exactly this
purpose in `idmap.rs:22,258,270` and `run.rs:5,179,188`. `inbox_write.rs`
never imports it.

**Verifier verdict:** CONFIRMED — both halves independently re-derived
(no staleness check in the retry path; fs4 present and used at the cited
lines).

**Fix:** switch the inbox lock to `fs4` advisory locking (roughly
line-neutral), or add an mtime-based staleness override to the sidecar
protocol. Add a crash-path test either way.

---

## 3. HIGH — `src/recorder.rs:281` — spool grows forever, fully re-read every tick

**Category:** unbounded-resource-growth (finder A)

**Summary:** `consume_spool` reads the ENTIRE hook-spool file into memory
on every tick regardless of the tracked offset, and nothing in the crate
ever rotates or truncates that file.

**Full description:** `let Ok(contents) = std::fs::read(spool_path) else
{ ... }` (recorder.rs:281) performs a whole-file read, then slices
`contents[offset..]` (recorder.rs:302) — the offset optimizes parsing, not
I/O; there is no seek. The spool is append-only forever:
`team_hook::append_line` (team_hook.rs:177-186) opens with
`.create(true).append(true)` and never truncates, and a grep across
`src/*.rs` for rotation/truncation logic finds none anywhere in the crate.
Hooks fire for every registered team event machine-wide (user-scope
registration), and consumers tick continuously: `record_command`'s loop on
a default 2s interval for the life of a "runs until killed" daemon
(recorder.rs:451-456), plus the pane-board spool-growth wake path. During
the project's own stated multi-week dogfood period the spool grows without
bound, and each tick re-reads the full file into memory just to use its
last few bytes.

**Verifier verdict:** CONFIRMED — whole-file read, append-only writer,
zero rotation, 2s tick cadence all independently re-derived.

**Fix:** `File::open` + `seek(SeekFrom::Start(offset))` + read tail
(small diff, kills the per-tick cost), plus an eventual rotation/size-cap
policy for the file itself.

---

## 4. HIGH — `src/pump.rs:146` vs `src/gather.rs:269` — team-dir enumeration duplicated, already diverged

**Category:** duplicated-logic (finder B)

**Summary:** teams_root resolution and team-dir enumeration ("dirs under
teams_root containing config.json") are each implemented twice —
`pump::default_teams_root`/`pump::discover_team_dirs` and
`gather::GatherPaths::from_env`/`gather::list_team_dirs` — and the copies
have already diverged.

**Full description:** Divergence today: `pump::discover_team_dirs`
(pump.rs:146) checks `path.is_dir() && join("config.json").is_file()`,
sorts, returns `Vec<PathBuf>`; `gather::list_team_dirs` (gather.rs:269)
checks only `join("config.json").is_file()`, does not sort, returns
`Vec<String>` of names. Both copies sit on live, independent hot paths for
team resolution: pump's via `pump_once` (pump.rs:111) and
`jump.rs:129` (`discover_team_leads`); gather's via `resolve_team`
(gather.rs:218) and `team_hook.rs:131` (M5 bucket resolution). The
concrete consequence already happened once: #102's liveness filter landed
only on the gather copy — `jump` still enumerates through the unfiltered
pump copy. The next behavioral fix (stale-dir filter, new layout) lands on
one copy and silently misses the other's whole call graph.

**Verifier verdict:** CONFIRMED — both functions, the divergence, and all
four caller sites independently traced.

**Fix:** collapse to one canonical function (gather's, since it now
carries the liveness filter) called by both modules. ~15-20 lines
removable.

---

## 5. HIGH — `HANDOFF.md:1` — describes dead project state as current

**Category:** doc-drift (finder C)

**Summary:** HANDOFF.md presents a completely superseded project state as
"current" while CLAUDE.md's mandatory read order sends every fresh agent
there FIRST.

**Full description:** The file says "Updated 2026-07-16 EOD" (line 3),
"all local, NOTHING pushed since 9c3c781" (line 8), describes a halted
worker mid-fix whose worktree needs salvage (lines 22-26), and lists the
version-reconcile + release call as pending next steps. Reality, verified
against git: #95-#102 all landed (43aceec...23e4f21), v2.1.0 is pushed and
tagged (`origin/main` at 3cc4bc9, far past 9c3c781; `git tag -l` shows
v2.1.0), `Cargo.toml:3` and `herdr-plugin.toml:3` both read
`version = "2.1.0"`, and the version reconcile happened in #95 on
2026-07-16. Meanwhile `CLAUDE.md:13` instructs: "Read in this order: 1.
`HANDOFF.md` — current state + exact NEXT steps." A fresh agent trusting
the mandated first read would try to salvage a worktree that no longer
exists and treat a shipped release as unreleased — the maximally
misleading position for stale state.

**Verifier verdict:** CONFIRMED — all quotes verbatim, git/tag/manifest
contradictions independently re-derived.

**Fix:** rewrite HANDOFF.md to the current state, or archive it to
docs/handoffs/ and point CLAUDE.md's read order at the live state source.

---

## 6. MEDIUM — `src/recorder.rs:480` — one transient write error kills the recorder daemon

**Category:** availability (finder A)

**Summary:** `record_command`'s "runs until killed" loop terminates the
whole process on any single transient `append_records` I/O failure.

**Full description:** Inside the `loop {}` (recorder.rs:470-482),
`append_records(&log_path, &records)?;` (line 480) propagates any
`RecorderError` out via `?` — `RecordCommandError` (recorder.rs:443-449)
has `#[from] RecorderError`, so the conversion is automatic. `main.rs:43`
wraps the call as `exit(recorder::record_command(&args))` and `exit()`
(main.rs:69-77) maps `Err` to `ExitCode::FAILURE`. A single momentary
write failure — disk briefly full, log directory swept by an external
cleanup, permission hiccup — permanently ends the dogfood tap, directly
contradicting the function's own doc: "Runs until killed — the recorder is
a live dogfood tap, not a one-shot report" (recorder.rs:451-456).

**Verifier verdict:** CONFIRMED — error type conversion, loop exit, and
main.rs mapping independently traced.

**Fix:** log the failure to stderr and continue next tick (honest
degradation, consistent with #92). Persistent failure can still terminate
after N consecutive errors if wanted.

---

## 7. MEDIUM — `src/team_hook.rs:268` — hook stdin read can block forever

**Category:** blocking-io (finder A)

**Summary:** `hook_command` reads all of stdin with no timeout,
contradicting the module's own documented invariant that the hook process
"must never block."

**Full description:** `std::io::stdin().read_to_string(&mut input)`
(team_hook.rs:268) blocks until EOF with no deadline anywhere in the
function. The module doc states the invariant verbatim: "it must never
block" (team_hook.rs:8-9) — the hook runs inside Claude Code's own event
path for every team event on the machine. If the invoking parent ever
holds the stdin pipe open longer than expected (slow parent, a future
Claude Code version buffering differently), the hook hangs indefinitely,
stalling the event path it was designed to stay out of.

**Verifier verdict:** CONFIRMED — doc invariant and unguarded read both
verified at the cited lines.

**Fix:** bounded read (size cap) with a read deadline; on timeout, exit 0
per the module's existing all-failures-are-silent contract.

---

## 8. MEDIUM — `src/gather.rs:240` — future transcript mtime declares a live team dead

**Category:** clock-skew / correctness (finder A; #102's own new logic)

**Summary:** `is_transcript_live` silently treats a transcript mtime in
the future relative to `now` as not-live, so clock desync knocks a
genuinely live team out of `resolve_team` candidates.

**Full description:** The predicate (gather.rs:240-244) is
`transcript_mtime.and_then(|mtime| now.duration_since(mtime).ok())
.is_some_and(|elapsed| elapsed <= LIVE_TRANSCRIPT_WINDOW)`.
`SystemTime::duration_since` returns `Err` whenever `mtime > now` — e.g.
an NTP step-backward between the transcript write and the `resolve_team`
call, or desynced clocks in a container. `.ok()` swallows that to `None`,
`is_some_and` on `None` is `false`, `team_is_live` (gather.rs:250-260)
reports not-live, and `resolve_team` (gather.rs:210-228) drops the team
from `candidates` — producing `NoTeams`/`Ambiguous` on exactly the
acceptance-critical path #102 just shipped (`plugin.pane.open
--entrypoint pane-board` without `--team`).

**Verifier verdict:** CONFIRMED — err-to-false path independently traced
through `team_is_live` into `resolve_team`'s candidate filter.

**Fix:** treat a future mtime as elapsed-zero (i.e. live):
`now.duration_since(mtime).unwrap_or(Duration::ZERO)`. One line, one test.

---

## 9. MEDIUM — `src/pane_board.rs:436` — nudge write freezes the TUI up to 1s, including Quit

**Category:** ui-blocking (finder A; severity raised by verifier)

**Summary:** `inbox_write::append_entry` runs synchronously inside the
TUI's key-handling branch, so inbox lock contention freezes the entire
single-threaded event loop.

**Full description:** `run_until_quit` (pane_board.rs:395-453) is a single
ordinary function — no `thread::spawn` anywhere in pane_board.rs or
main.rs — so `terminal.draw`, `event::poll`/`event::read`, and the
`Action::SendNudge` arm all share one thread. That arm
(pane_board.rs:428-439) calls `inbox_write::append_entry` inline, which
runs `acquire_lock`'s full retry budget on contention:
`LOCK_RETRY_ATTEMPTS: u32 = 20` × `LOCK_RETRY_DELAY: 50ms`
(inbox_write.rs:154-155) = 1000ms worst case. During that window the board
cannot redraw, process keys, or quit.

**Verifier verdict:** CONFIRMED, severity raised low → medium: "the freeze
also blocks the Quit key on the same call stack, not just redraw — a stuck
lock briefly makes the TUI unkillable via its own keybinding."

**Fix:** shorter retry budget for the interactive path (e.g. 4×50ms with
honest WriteError overlay on failure), or accept and document the ceiling
with a `ponytail:` comment. Combines with finding 2: fs4 locking removes
most contention scenarios.

---

## 10. MEDIUM — `src/recorder.rs:291` — truncation-resync branch has zero test coverage

**Category:** test-gap (finder C)

**Summary:** The external-truncation/rotation resync branch of
`consume_spool` is the only branch of that function without a dedicated
test.

**Full description:** When `state.spool_offset` exceeds the current file
length (spool rotated/truncated/replaced externally between ticks), the
code resyncs to EOF and returns no records instead of panicking on an
out-of-bounds `contents[offset..]` slice (recorder.rs:291-297; the comment
says "rotated/truncated externally ... rather than panicking"). Every
sibling branch has a named test — missing file (recorder.rs:872),
first-call EOF baseline (880), appended line (900), partial trailing line
(948), malformed line (974) — but nothing ever shrinks the spool file or
forces `spool_offset` past file length. The only truncation-adjacent test
(`append_records_never_truncates_prior_lines`, recorder.rs:828) exercises
a different function. This is precisely the defensive branch a future
refactor silently regresses to a panicking slice, with the whole suite
staying green.

**Verifier verdict:** CONFIRMED — grepped `truncat|rotat|shrink|
spool_offset =` across the test module; no test exercises the branch.

**Fix:** one test — write spool, consume, truncate the file shorter than
the stored offset, tick again, assert empty records + resynced offset.

---

## 11. LOW — `docs/agents/issue-tracker.md:3` — stale repo slug

**Category:** doc-drift (finder C; hedge removed by verifier)

**Summary:** The issue-tracker doc names the pre-rename slug even though
the GitHub rename has already happened.

**Full description:** `docs/agents/issue-tracker.md:3` reads "Published
2026-07-15: `caioniehues/herdr-agent-team`". The finder hedged ("low risk,
gh follows redirects — but the caveat lives only in CLAUDE.md:108, not in
the doc agents actually follow"). The verifier removed the hedge: the
rename HAS happened — `gh repo view --json name,nameWithOwner` returns
`caioniehues/herdmates` today and `git remote -v` shows origin at
`github.com/caioniehues/herdmates.git`. The doc is stale now, not
hypothetically, and works only while GitHub's redirect survives (breaks if
the old name is ever reused).

**Verifier verdict:** CONFIRMED.

**Fix:** one-line slug update in the doc.

---

## Explicitly checked, NOT flagged

- `team_hook.rs` `GateDecision::Block` + `#[allow(dead_code)]`: inspected
  by finder B — explicit, documented plumbing for the stated post-v1
  gating predicate, with a test pinning the current no-op contract.
  Deliberate, not speculative generality.
- `herdr.rs` / `audit.rs`: finder B corrected the coordinator's brief —
  these are NOT frozen legacy; both are active shared infra (herdr.rs
  backs docs/spec.md §4/6/9; audit.rs is #86/D3). No
  active-code-tangled-into-frozen-files freeze-violation risk found.
- Frozen legacy surface (ADR-0012): light scan found no glaring rot worth
  a finding.

## Aggregate

- ~30 directly deletable lines (findings 1 + 4), one roughly line-neutral
  crash-safety fix (finding 2), the rest small scoped diffs.
- Verification stats: 10/11 confirmed on independent re-derivation, 0
  refuted; the single non-confirm got WORSE on inspection, not better.
  Finder precision was high.
- Clusters worth noting: findings 2+9 share a root (sidecar lock design —
  fs4 fixes both), findings 3+10 share a file and a theme (spool lifecycle
  is the least-hardened part of the #100 surface), findings 5+11 are both
  "docs a fresh agent reads first are the stalest."
