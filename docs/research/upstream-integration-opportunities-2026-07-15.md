# Upstream Herdr Integration Opportunities

Date: 2026-07-15  
Scope: read-only comparison of `/home/caio/Projects/herdr-upstream` against `/home/caio/Projects/herdr-agent-team`  
Audience: the god + worker team plugin roadmap

## Executive conclusion

The plugin should become a thin team-domain layer over Herdr, not a parallel terminal orchestrator. Herdr already owns pane/worktree lifecycle, agent detection and rollups, compact status presentation, single-pane waits, notifications, layout, and cold session restore. The plugin's durable value is the part Herdr does not model: team membership, god/worker roles, task dependencies and claims, mailbox/report protocols, aggregate waits, targeted restart policy, and run-scoped fan-out.

The most valuable missed integrations, in order, are:

| Rank | Priority | Opportunity | Decision |
|---:|:---:|---|---|
| 1 | P0 | Reconcile lifecycle hooks beyond status | Add move/exit/close/workspace/worktree hooks before expanding features; a pane move changes its public id and can silently stale the run board. |
| 2 | P0 | Publish team/task/progress into native Agent sidebar metadata | Use version-gated metadata tokens, with `display_agent` or title as the zero-config fallback; do not build another statusline. |
| 3 | P0 | Preserve the complete read-only `agent_session` | Store `source`, `agent`, `kind`, and `value`; build restart only for explicitly supported launchers because upstream exposes no targeted resume method. |
| 4 | P0 | Make the task board/dashboard a native plugin pane | Declare a durable tab entrypoint plus a quick popup/overlay action; let Herdr own placement and pane lifecycle. |
| 5 | P0 | Use one direct event subscription for dashboard refresh and team waits | Bootstrap with `session.snapshot`, subscribe for every run member, and reconcile after reconnect; do not launch one CLI process per worker wait. |
| 6 | P1 | Make runtime version/session identity explicit | Probe the installed schema, persist session/socket identity, and gate preview-only fields such as metadata tokens. |
| 7 | P1 | Emit aggregate notifications | Notify for team completion, prolonged blocking, or unrecoverable exit, not every native status change. |
| 8 | P1 | Add run-scoped broadcast and richer inspection | Fan out explicitly to board members with per-target results; keep durable report files because Herdr has no broadcast or transcript export. |
| 9 | P2 | Evaluate layout/graphics/live-terminal adjuncts later | `layout.apply`, Kitty graphics, and terminal control are optional presentation/topology tools, not team semantics. |

## 1. P0 — reconcile lifecycle hooks and changing pane identity

The current manifest registers only `pane.agent_status_changed` (`/home/caio/Projects/herdr-agent-team/herdr-plugin.toml:32-34`). Upstream allows 21 low-volume manifest hook kinds spanning workspace, worktree, tab, and pane lifecycle (`/home/caio/Projects/herdr-upstream/src/api/schema/events.rs:281-303`).

Add these first:

- `pane.moved`: update the run member's pane, tab, and workspace ids atomically. The event supplies `previous_pane_id`, previous workspace/tab, and the new `PaneInfo` (`/home/caio/Projects/herdr-upstream/src/api/schema/events.rs:495-508`). Cross-workspace moves preserve the terminal while assigning a new public pane id (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/socket-api.mdx:289-296`). This is an immediate correctness hole: later status events and commands will target a stale id.
- `pane.exited` and `pane.closed`: distinguish a dead process from explicit pane removal and mark the worker dead/orphaned (`/home/caio/Projects/herdr-upstream/src/api/schema/events.rs:484-487,514-517`).
- `workspace.closed` and `worktree.removed`: reconcile a whole removed team allocation (`/home/caio/Projects/herdr-upstream/src/api/schema/events.rs:426-429,452-457`).
- `pane.agent_detected`: bind native agent identity/session data earlier and reduce launch polling (`/home/caio/Projects/herdr-upstream/src/api/schema/events.rs:518-520`).

Do not attempt high-frequency UI refresh through manifest hooks. Upstream deliberately excludes `pane.output_changed`, `pane.updated`, `layout.updated`, and `workspace.metadata_updated` until suitable hook semantics exist (`/home/caio/Projects/herdr-upstream/src/api/schema/events.rs:314-352`). Those belong on a direct subscription used by the dashboard process.

## 2. P0 — use native metadata as the compact team dashboard

Herdr describes its Agent sidebar as the main dashboard and already presents attention-sorted state across all agents (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/configuration.mdx:255-300`). Current preview docs and source add arbitrary `$name` tokens populated through `pane report-metadata`; tokens support sequence numbers and TTLs (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/configuration.mdx:323-343`; `/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/cli-reference.mdx:229-252`).

Publish only compact, presentation-only facts:

- `team`: run/team name
- `role`: god or worker role
- `task`: current task id/short title
- `progress`: short phase or bounded progress message, with `--seq` to reject stale updates and `--ttl-ms` for transient pings

Keep lifecycle truth in the run board. Upstream explicitly separates presentation metadata from semantic agent state: lifecycle state drives waits, notifications, and rollups, while display text/tokens do not (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/agents.mdx:115-132`).

Two caveats materially change the implementation:

1. Arbitrary tokens do not appear in the default sidebar rows; defaults show state/workspace/tab and agent (`/home/caio/Projects/herdr-upstream/src/config/sidebar.rs:213-227`). Provide a documented sidebar-row snippet and use `display_agent` or pane title as a zero-config fallback.
2. This is a preview/current-source surface, not confirmed in the plugin's recorded runtime. The plugin snapshot expects `custom_status` (`/home/caio/Projects/herdr-agent-team/docs/herdr-api-schema.snapshot.json:5265-5273`), while current upstream `PaneInfo` exposes `tokens` and no `custom_status` (`/home/caio/Projects/herdr-upstream/src/api/schema/panes.rs:395-427`). Probe the installed schema before sending token fields; remove the roadmap assumption around `--custom-status` (`/home/caio/Projects/herdr-agent-team/docs/spec.md:194-200,220-224`).

Obsolescence decision: cancel any generic plugin statusline. Native sidebar/rollups are already the right compact view. The plugin dashboard should show only team-specific tasks, dependencies, reports, mailbox state, and controls.

## 3. P0 — preserve full `agent_session`; keep restart conservative

Upstream's read-only `agent_session` contains `source`, `agent`, `kind`, and `value` (`/home/caio/Projects/herdr-upstream/src/api/schema/agents.rs:50-92`). The current plugin retains only `.value` (`/home/caio/Projects/herdr-agent-team/src/herdr.rs:66-119`), which loses the information required to choose a safe resume mechanism.

Native automatic resume is useful but narrower than the roadmap needs:

- Herdr restores supported agent conversations while reconstructing panes after a cold server restart; it is enabled by default (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/session-state.mdx:50-88`).
- Upstream has private, official-integration-gated mappings for Claude, Codex, Copilot, Devin, Droid, Kimi, MastraCode, Pi, OMP, Hermes, OpenCode, Qoder, Kilo, and Cursor (`/home/caio/Projects/herdr-upstream/src/agent_resume.rs:53-70,94-197,206-223`).
- There is no public `agent resume` CLI/API for targeted worker restart. `pane report-agent-session` cannot mint arbitrary resumable sessions because native restore accepts only official source/agent pairs.

Recommendation: persist the full session object in worker state now. For explicit `worker restart`/`team restart`, support only launchers whose resume argv is deliberately implemented and tested by this plugin, and record unsupported outcomes per worker. Monitor for a future public upstream resume method; if it appears, delete the plugin's mappings and delegate to it.

Do not use terminal replay as restart. `agent read` returns recent display text, and experimental pane history restores screen contents rather than a conversation/process (`/home/caio/Projects/herdr-upstream/src/app/api/agents.rs:61-108`; `/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/session-state.mdx:37-48`).

## 4. P0 — implement the roadmap board as a native plugin pane

Plugin v1 supports manifest actions, events, panes, and link handlers (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/plugins.mdx:55-118`). A declared pane can open as overlay, popup, split, tab, or zoomed (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/plugins.mdx:257-301`; `/home/caio/Projects/herdr-upstream/src/api/schema/plugins.rs:409-443`). Non-popup entries become normal Herdr panes and retain plugin ownership while users move, resize, swap, or zoom them.

Recommended shape:

- Declare a `board` pane entrypoint.
- Add an `open-board` action, with a durable `tab` placement for normal work.
- Optionally expose a popup/overlay variant for quick inspection.
- Bind it through `[[keys.command]] type = "plugin_action"` (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/plugins.mdx:303-313`).
- Use the invocation context rather than rediscovering selection: it can carry workspace/worktree/tab/pane ids, agent/status, selected text, correlation id, and clicked URL (`/home/caio/Projects/herdr-upstream/src/api/schema/plugins.rs:344-386`). A link handler could make report/task pointers open directly in the board.

There is no native non-terminal widget or statusline API. Plugin panes are terminal programs; runtime action registration is also out of scope for v1 (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/plugins.mdx:24-34`). This fits a text board well and avoids designing against a UI seam that does not exist.

## 5. P0 — direct subscription for dashboard and aggregate waits

The current wrapper starts a Herdr CLI subprocess for every request (`/home/caio/Projects/herdr-agent-team/src/herdr.rs:262-284`) and waits one agent at a time (`/home/caio/Projects/herdr-agent-team/src/herdr.rs:222-248`). That remains acceptable for low-rate mutations but is the wrong shape for a live board or `team wait`.

Use a small direct client for exactly two long-lived workloads:

1. Board cache: call `session.snapshot`, then subscribe, and re-snapshot after reconnect or suspected staleness. Upstream documents this cache pattern (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/socket-api.mdx:116-123`). `herdr api snapshot` returns one consistent version/protocol/focus/workspace/tab/pane/layout/agent view (`/home/caio/Projects/herdr-upstream/src/cli/api.rs:56-65`; `/home/caio/Projects/herdr-upstream/src/api/schema/session.rs:8-23`).
2. Team wait: send one `events.subscribe` request containing status subscriptions for every run-board worker. The server checks current state while anchoring sequence, so already-matching states and subsequent changes are both covered (`/home/caio/Projects/herdr-upstream/src/api/subscriptions.rs:49-57,251-281`). Implement `all`, `any`, `blocked`, deadline, and per-worker terminal outcomes in the plugin domain.

Do not build team wait on `events.wait`; its current implementation supports only a single pane-agent-status match (`/home/caio/Projects/herdr-upstream/src/api/wait.rs:177-195`). Native CLI waits remain useful for one-shot cases: `wait agent-status` includes `done`, and `wait output --regex` can wait for protocol sentinels (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/cli-reference.mdx:302-316`).

### IPC boundary

| Workload | Use | Why |
|---|---|---|
| Dashboard snapshot + event stream | Direct IPC | Long-lived connection, low latency, no polling/process churn. |
| Aggregate team wait | Direct IPC | One multiplexed subscription and run-aware fan-in. |
| High-frequency metadata updates | Direct IPC optionally | Avoid repeated process launch; retain CLI fallback while schema varies. |
| Workspace/worktree/pane creation, rename, close | CLI | Low rate; upstream owns cross-platform transport, validation, and diagnostics. |
| One-shot reads/status/notification | CLI by default | Simpler and portable unless profiling proves process cost significant. |

The public transport is newline-delimited JSON: one request per connection for ordinary calls, while subscription calls retain the stream (`/home/caio/Projects/herdr-upstream/src/api/client.rs:32-107,155-169,207-239`; `/home/caio/Projects/herdr-upstream/src/api/server.rs:139-159`). Upstream explicitly recommends CLI wrappers for normal automation and the socket for direct request/response or subscriptions (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/socket-api.mdx:6-18`).

Do not import the internal Rust client: Herdr is a binary crate with private modules (`/home/caio/Projects/herdr-upstream/Cargo.toml:1-19`). Implement only the minimal schema-versioned NDJSON subset. Use the exact injected `HERDR_SOCKET_PATH`; transport is a local Unix socket on Unix and a namespaced local socket/named pipe on Windows (`/home/caio/Projects/herdr-upstream/src/ipc.rs:29-46`). Keep `HERDR_BIN_PATH` CLI fallbacks for portability. Never use the private TUI client socket; upstream's architecture requires shared facts to flow through the public JSON API/events (`/home/caio/Projects/herdr-upstream/AGENTS.md:36-51`).

## 6. P1 — schema, session, and snapshot awareness

The checkout declares package version 0.7.3 (`/home/caio/Projects/herdr-upstream/Cargo.toml:1-4`) but also contains preview documentation under `docs/next`; upstream explicitly separates released and preview docs (`/home/caio/Projects/herdr-upstream/AGENTS.md:151-159`). Do not infer runtime support from checkout source alone.

At plugin start or first advanced operation:

- Use `ping` for version/protocol, then `herdr api schema --json` to feature-detect fields and methods. Current server capabilities advertise only handoff/daemon flags, not metadata tokens (`/home/caio/Projects/herdr-upstream/src/api/schema/server.rs:16-21`; `/home/caio/Projects/herdr-upstream/src/api/status.rs:14-59`).
- Prefer one `api snapshot` over multiple lists for bootstrap. It is implemented and included in command specs (`/home/caio/Projects/herdr-upstream/src/cli/spec.rs:130-139`) but is semi-hidden because the prose CLI API examples omit it (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/cli-reference.mdx:37-47`).
- Persist the exact Herdr session/socket identity with each run. Named sessions can be selected by `--session`, `HERDR_SESSION`, or `session attach`, and each has a distinct data directory/socket (`/home/caio/Projects/herdr-upstream/src/session.rs:29-93,157-180`). Plugin state directories themselves are global rather than automatically session-scoped (`/home/caio/Projects/herdr-upstream/src/plugin_paths.rs:12-16`).

Named sessions could isolate multiple gods, but do not migrate teams silently: a god and its workers must remain in the same visible runtime/session unless explicit cross-session behavior is designed.

## 7. P1 — aggregate notifications, not duplicate agent noise

`notification show` supports title/body, corner position, and `none`, `done`, or `request` sound (`/home/caio/Projects/herdr-upstream/src/cli/notification.rs:23-41,50-106`). Delivery reports whether it was shown, disabled, rate-limited, lacked a foreground client, or was busy (`/home/caio/Projects/herdr-upstream/src/api/schema/common.rs:79-123`).

Use it for:

- team complete;
- a worker blocked beyond a configurable threshold;
- an unrecoverable worker exit/restart failure;
- a request that needs god/user attention.

Do not notify on every `agent_status_changed`: Herdr already owns native background agent notifications and rollups. Progress tokens should update the compact view; notification is the escalation channel.

## 8. P1 — fill orchestration gaps Herdr intentionally leaves generic

### Broadcast and messaging

There is no native broadcast verb. `agent send` and `pane run` each target one agent/pane (`/home/caio/Projects/herdr-upstream/src/api/schema.rs:112-119`; `/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/cli-reference.mdx:193-208`). Implement `team msg --all` as an explicit loop over run-board members, never over all Herdr agents, and return a per-target result. The current server handles one initial request per ordinary connection, so each mutation remains a separate request (`/home/caio/Projects/herdr-upstream/src/api/server.rs:139-159`).

Keep the existing pointer-only mailbox wakeup for durable coordination. Native `agent.send` is literal/unsubmitted terminal input; it is not a mailbox/report protocol.

### Reads and transcripts

There is no transcript export. `agent read`/`pane read` return bounded terminal display content, and terminal observe streams live ANSI frames (`/home/caio/Projects/herdr-upstream/src/app/api/agents.rs:61-108`; `/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/cli-reference.mdx:278-300`). Preserve inbox/outbox/report/run-board artifacts as durable truth.

For the board, add bounded `recent-unwrapped` reads only as a preview. Pane reads support source, line count, text/ANSI, and raw modes, while `process-info` and `agent explain` are better diagnostics than terminal-text heuristics (`/home/caio/Projects/herdr-upstream/src/cli/spec.rs:267-282,337-469`).

### Agent start and layout

`agent start` provides named argv-backed agent placement with cwd/env/focus (`/home/caio/Projects/herdr-upstream/src/cli/agent.rs:270-369`). When pointed at an existing workspace/tab it creates a split (`/home/caio/Projects/herdr-upstream/src/app/agents.rs:127-185`), so it does not replace today's dedicated-workspace create + root-pane run flow. It is useful only if the plugin adopts an alternate same-workspace team topology.

`layout.export`/`layout.apply` expose a portable declarative BSP tree (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/socket-api.mdx:200-213`; `/home/caio/Projects/herdr-upstream/src/api/schema.rs:132-137`). Consider them after team semantics stabilize, for deterministic dashboard/team layouts rather than iterative splits.

## 9. Native overlap and obsolescence watch

| Plugin roadmap/capability | Native overlap | Risk | Boundary to keep |
|---|---|:---:|---|
| Generic dashboard/status list | Agent sidebar scans all agents, attention-sorts them, focuses them, and rolls state up through panes/tabs/workspaces (`/home/caio/Projects/herdr-upstream/src/ui/sidebar.rs:126-182`; `/home/caio/Projects/herdr-upstream/src/workspace/aggregate.rs:80-90`). | High | Board shows team membership, tasks, dependencies, reports, mailbox, and actions only. |
| Progress/statusline | Metadata tokens, titles, state labels, and sidebar rows already provide display chrome. | High | Plugin derives team-aware metadata; no separate statusline system. |
| Spawn/status | Native workspace/worktree/pane/agent primitives and `api snapshot` cover mechanics. | Medium | Plugin owns heterogeneous team specs, role assignment, immutable protocols, and run membership. |
| Team wait | Native wait covers one pane/status or output match. | Medium | Plugin owns run-aware all/any/blocked/dead fan-in. |
| Team restart | Native cold restore owns official resume mappings, but no targeted public resume exists. | Medium/high future | Keep a narrow compatibility layer; delete it if upstream adds `agent.resume`. |
| Completion/progress pings | Native notifications and state rollups already handle generic attention. | High | Notify only on aggregate/team policy. |
| Task board | No native task, dependency, claim, or team-membership model was found. | Low | Core plugin domain. |
| Mailbox/reports/broadcast | No native mailbox, durable report, or multi-target send abstraction was found. | Low | Core plugin domain, layered over targeted pane/agent delivery. |

## 10. Plugin host surface missed today

Beyond the single event hook, current upstream exposes:

| Surface | Available integration | Value to this plugin |
|---|---|---|
| Manifest actions | Context-scoped commands with platform filters (`/home/caio/Projects/herdr-upstream/src/api/schema/plugins.rs:234-386`). | Add open-board, wait-team, restart-team, next-blocked, and selected-task actions. |
| Keybinds | Qualified `plugin_action` commands (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/plugins.mdx:303-313`). | Fast board/attention navigation without custom key handling. |
| Pane entrypoints | Popup/overlay/split/tab/zoomed terminal panes. | Native dashboard hosting and lifecycle. |
| Link handlers | URL-pattern actions receive clicked URL context (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/plugins.mdx:315-327`). | Make task/report pointers actionable from terminal output. |
| Event hooks | 21 low-volume lifecycle hooks. | Reconcile moves, exits, closures, and removal. |
| Config/state dirs | Stable injected directories; plugin owns schema, migration, and lifecycle (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/plugins.mdx:225-255,329-332`). | Store user config/run state safely, but build our own migrations. |
| Runtime logs | Host bounds command streams/concurrency and retains plugin logs. | Diagnostics; not durable team history. |
| UI/statusline | No native non-terminal widget or statusline extension in plugin v1. | Use terminal pane + metadata/sidebar instead. |

## 11. Experimental and hidden surface

- **Preview is the roadmap:** there is no upstream `ROADMAP`, project plan, or actionable core `TODO`/`FIXME` trail in the non-vendored tree. `docs/next`, its Unreleased changelog, schema changes, and comments around excluded hooks are the meaningful forward signals. The released/preview split is documented at `/home/caio/Projects/herdr-upstream/AGENTS.md:151-159`.
- **Metadata tokens are unreleased/unstable for our runtime:** they exist in current source/preview docs but are absent from the plugin's protocol-16 snapshot and not named in the staged Unreleased changelog (`/home/caio/Projects/herdr-upstream/docs/next/CHANGELOG.md:1-18`). Feature-detect them.
- **Semi-hidden `api snapshot`:** implemented and in generated help, but missing from the prose API command examples (`/home/caio/Projects/herdr-upstream/src/cli/spec.rs:130-139`; `/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/cli-reference.mdx:37-47`). Adopt it.
- **Integration-only session reporters:** `pane report-agent-session` and `pane release-agent` appear in parser/help source (`/home/caio/Projects/herdr-upstream/src/cli/pane.rs:17-40`; `/home/caio/Projects/herdr-upstream/src/cli/spec.rs:466-504`) but are not general lifecycle commands. Read official session data; do not claim ownership unless implementing a genuine agent integration.
- **Hidden internal modes:** `client` and `remote-client-bridge` are intentionally hidden (`/home/caio/Projects/herdr-upstream/src/main.rs:480-493`). Do not depend on them or the private client socket.
- **High-volume future seam:** comments say output/layout/update hooks remain excluded until output-change semantics are implemented (`/home/caio/Projects/herdr-upstream/src/api/schema/events.rs:314-316`). Watch this, but retain direct subscriptions now.
- **Experimental pane history:** off by default because it may retain secrets and only restores screen evidence, not transcripts (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/session-state.mdx:37-48`). Optional crash forensics only.
- **Experimental Kitty graphics:** pane graphics/streaming can enrich a future board but are feature-gated (`/home/caio/Projects/herdr-upstream/docs/next/website/src/content/docs/socket-api.mdx:166-187`). Keep a text dashboard as baseline.
- **Terminal control/observe:** runtime parsing exposes writable control and read-only observation (`/home/caio/Projects/herdr-upstream/src/cli.rs:528-576`), while generated command specs expose only observe (`/home/caio/Projects/herdr-upstream/src/cli/spec.rs:558-575`). Treat control as unstable and unnecessary for the task board.

## Recommended execution sequence

1. Add lifecycle reconciliation for move/exit/close/workspace/worktree removal and test id migration.
2. Persist full `agent_session` and exact Herdr session/socket identity in the run board.
3. Add schema-gated metadata publishing plus a default-sidebar fallback; use aggregate notifications for escalation.
4. Declare an `open-board` action and native board pane.
5. Add a minimal direct snapshot/subscription client used only by the board and team wait; retain CLI mutations/fallbacks.
6. Add run-scoped broadcast, bounded report previews, and conservative supported-launcher restart.
7. Evaluate declarative layouts and optional graphics only after the board/task model is stable.

READY FOR REVIEW
