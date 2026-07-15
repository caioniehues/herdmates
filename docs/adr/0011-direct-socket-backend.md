# ADR-0011: Experimental direct NDJSON socket backend behind `HerdrApi`; CLI stays default

Status: accepted (2026-07-15, grilling interview with Caio; implementation
scheduled in the post-v1 wave, spec §8 step 5)

## Context

Every plugin operation today shells out to the `herdr` CLI (one subprocess
per call, `src/herdr.rs`). That is fine for low-rate mutations but the wrong
shape for two upcoming long-lived workloads:

- **Dashboard/board pane**: needs a consistent snapshot plus a live event
  stream; per-refresh CLI fan-out would block rendering (ecosystem
  convention #10, `docs/research/awesome-herdr-2026-07-15.md`).
- **`team wait`**: aggregate wait over all run members; the CLI offers only
  single-target waits, so CLI-based team wait means one subprocess per
  worker (`docs/research/upstream-integration-opportunities-2026-07-15.md`
  §5).

Three independent 2026-07-15 reports converged on the same recommendation:
integration-opportunities P0 #5, herdr-claude-teams "what to steal" #1–2
(whose Python shim demonstrates a working raw-socket client and a hermetic
fake-socket test harness), and the ecosystem survey conventions.

Herdr exposes two IPC surfaces (source-verified,
`docs/research/upstream-architecture-claims-2026-07-15.md` Part A §4):

1. Public automation socket (`HERDR_SOCKET_PATH`): newline-delimited JSON,
   `ping` reports protocol version (16 on our baseline).
2. Private client/render socket (`herdr-client.sock`): length-prefixed
   bincode for the TUI; version-locked with exact equality.

## Decision

1. **Add an experimental direct socket backend behind the existing
   `HerdrApi` seam.** The CLI backend remains the **default and the
   fallback**; the socket backend is opt-in until proven in dogfooding.
2. **Public NDJSON socket only.** The private bincode client socket is
   off-limits permanently (upstream treats it as internal; exact-version
   lock makes it a compatibility trap).
3. **Scope of the socket backend** (the only workloads allowed to require
   it):
   - board/dashboard bootstrap via `session.snapshot` + `events.subscribe`
     (re-snapshot on reconnect);
   - aggregate `team wait` via one multiplexed `events.subscribe` carrying
     all run members;
   - optionally high-frequency metadata publishing.
   Low-rate mutations (workspace/worktree/pane create, rename, close, run,
   kill) stay on the CLI: upstream owns validation, transport quirks, and
   diagnostics there.
4. **Contract discipline:**
   - validated against the checked-in protocol-16 schema snapshot and
     `herdr api schema --json` at runtime;
   - `ping` capability handshake at connect: record server version +
     protocol, refuse unsupported schemas with a clear error (the
     competitor's design called for this and never implemented it — we do
     the missing part);
   - response-ID checking, typed `result.type` validation, bounded frame
     size, structured errors;
   - hermetic **fake socket** test server (record method/params, inject
     typed errors, replay protocol-16 fixtures) so transport is testable
     without a live herdr — pattern ported from herdr-claude-teams'
     `ThreadingUnixStreamServer` fake, reimplemented in Rust;
   - optional gated JSONL protocol trace (request id, method, result type,
     latency, error code — never prompt/message text by default).
5. **Not stolen** from the competitor: the fake-tmux architecture, the
   herdr 0.6.10 ID grammar, per-operation `pane.list` scans, silent success
   on unsupported operations, raw payloads in traces
   (`docs/research/herdr-claude-teams-analysis-2026-07-15.md` §5).

## Consequences

- `HerdrApi` becomes a genuine two-backend seam; the near-duplicate
  `HerdrApi` surfaces in `spawn.rs`/`msg.rs` and the four `FakeHerdr` test
  fakes (2026-07-15 architecture review) should consolidate as prerequisite
  refactoring in the implementing ticket.
- If upstream ever ships aggregate waits or a public resume/subscribe CLI
  that removes the need, shrink the socket backend rather than defending
  it (ADR-0007 discipline).
- Windows: the public socket is a named pipe; the backend must go through
  the same abstraction or defer Windows to the CLI fallback.
