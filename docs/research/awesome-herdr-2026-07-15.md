# Awesome Herdr ecosystem research — 2026-07-15

## Scope and method

This report evaluates every detailed entry in [`awesome-herdr`](https://github.com/yigitkonur/awesome-herdr/blob/e4823a51f3a65c525ff350c826bd8955a8fbe4ce/README.md) at commit [`e4823a5`](https://github.com/yigitkonur/awesome-herdr/commit/e4823a51f3a65c525ff350c826bd8955a8fbe4ce), last changed 2026-07-01. The live README contains 133 entries in seven sections. The at-a-glance table is not exhaustive and was not used as the inventory.

For every entry, the linked repository README was fetched through the GitHub API where available. Repositories involving orchestration, messaging, worktrees, lifecycle events, dashboards, or plugin authoring also received targeted source/manifests inspection. A 404 is reported rather than silently substituting the curated description. “Surface” means the Herdr capability actually used: CLI, socket/MCP, manifest action/event/pane, integration hook, worktree API, metadata, or terminal attach.

Verdicts are relative to `herdr-agent-team`, not general project quality:

- **steal-pattern** — adopt a bounded design pattern, not the product.
- **competitor** — overlaps ownership of teams, delegation, task state, messaging, or worker lifecycle.
- **integration-target** — should compose with or expose `herdr-agent-team`; do not absorb it.
- **irrelevant** — useful Herdr work, but outside this plugin's domain.

## Executive findings

`herdr-agent-team` remains differentiated by the combination of heterogeneous Claude/Codex workers, an existing god pane, explicit star/mesh topology, spawned and adopted workers, durable run state, worktree isolation, pointer-based reports, event-driven wakeups, and a verified `msg`/outbox path. Several projects implement subsets; the closest strategic competitors are `herdr-factory`, `dual-author`, `herdr-orchestrator`, `Shepherd`, `herdr-symphony`, `herdr-factory-loop-skill`, and `herdr-claude-teams`.

The ecosystem strongly validates the roadmap. Dashboard panes are already a convention; blocking waits and callback/report protocols recur; stable agent session IDs are used for restoration; task boards increasingly model queues, pipeline stages, and dependencies; progress belongs in display metadata, not a competing lifecycle authority.

Three curated orchestration links (`yigitkonur/herdr-pm`, `SecretAardvark/pi-overseer`, `bakescakes/claude-orchestration`) and one WIP link (`rbb/herdr-cursor`) returned README 404s. `deepin-herdr` intentionally has no README. The WIP description for `raycast-herdr` is stale: the current README documents implemented commands.

## Roadmap overlaps and recommended response

| Our roadmap | Strongest evidence | Recommendation |
|---|---|---|
| Dashboard pane | `dual-author`, `herdr-factory`, `pi-herdr-subagents`, `herdr-insight`, `homestead`, `herdr-agent-dashboard` | Ship a `[[panes]]` run dashboard with team/task/status/elapsed/attention columns; keep collection outside rendering; add dependency view later. |
| `team wait` | Official skill, `herdr-orchestrator-skill`, `herdr-mesh`, `herdr-symphony` | Wrap a long blocking wait, inspect before waiting, accept idle/done semantics, and add `--until report`; never tight-poll from the god. |
| `team restart` | `herdr-session-restore`, `ask-fable`, Shepherd, `sean1588/herdr-orchestrator` | Persist launcher-specific session IDs continuously; treat pane IDs as volatile; re-resolve or recreate panes, then use `resume_command`. |
| Task boards | `dual-author`, `herdr-factory`, `herdr-symphony`, Obsidian bridge, aiki | Make the run-board the single writer of claimable tasks with status, assignee, blocked-by edges, timestamps, and audit events. Add adapters for external trackers. |
| Progress pings | `herdwatch`, Letta/MiMo/TraeX integrations, token dashboards | Use source-scoped `pane.report_metadata --custom-status`; never seize `pane.report_agent` lifecycle authority from the harness. |
| Messaging | `yangyang0507/herdr-skill`, `herdr-mesh`, `herdres`, `tinysend-herdr`, `kirel/herdr-subagents` | Add message IDs/kind/task/reply-to to the envelope and optional delivery audit; retain `pane run`, durable file pointers, ambiguity refusal, and non-blocking send. |
| Worktrees | lifecycle, bootstrap, event-hook, fresh-worktree, workspace-manager | Add teardown policy, setup logs, idempotency/serialization, cached removal config, and explicit provider seams while preserving dirty-tree salvage. |

## Ecosystem conventions to adopt

1. Require `HERDR_ENV=1` for in-pane control; use explicit/current targets, treat IDs as opaque, and parse mutation responses.
2. Use `pane run` for text plus submit. Inspect current state before waiting; prefer long blocking waits and pushed events over polling.
3. Store large payloads in files and inject pointers. Keep durable domain state outside model context and write snapshots atomically.
4. Refuse ambiguous routing. A successful transport call is not an application acknowledgement; record delivery and task/report state separately.
5. Separate mechanism from policy: a deterministic run/task engine owns transitions; agents supply bounded judgment; irreversible actions pass authoritative gates.
6. Close only resources the plugin created. Release borrowed/adopted resources. Dirty or unmerged worktrees fail closed unless force is explicit.
7. Plugin processes start in the plugin root and receive bare argv with an unreliable PATH. Use absolute shims/binaries and explicit cwd/event paths.
8. Cover both `worktree.created` and `worktree.opened`; deduplicate concurrent hooks and avoid multiple plugins racing to mutate a fresh layout.
9. Use `HERDR_PLUGIN_STATE_DIR` for runtime state and `HERDR_PLUGIN_CONFIG_DIR` or an explicit repo file for policy. Cache teardown data before removal deletes the checkout.
10. Dashboards should consume snapshots/events and never block the renderer on many CLI calls. Priority order is blocked, done, working, idle.
11. Lifecycle integrations aggregate and debounce child events, report under a source ID, and release authority on exit/crash. Display-only progress uses metadata.
12. Expose least authority by role. Coordinator/worker/observer profiles should fail closed and mark destructive actions.

### Deep source checks behind the recommendations

- Official control discipline: upstream [`SKILL.md`](https://github.com/ogulcancelik/herdr/blob/master/SKILL.md).
- Structured non-blocking messaging: [`herdr-msg`](https://github.com/yangyang0507/herdr-skill/blob/main/herdr/scripts/herdr-msg); generic MCP relay/handoff behavior: [`composite.ts`](https://github.com/runchr-works/herdr-mesh/blob/main/src/tools/composite.ts).
- Dashboard collector/render separation and dependency graph: [`dual-author/scripts/dashboard.py`](https://github.com/Tudor0404/dual-author/blob/main/scripts/dashboard.py); actionable factory dashboard: [`src/tui/dashboard.ts`](https://github.com/razajamil/herdr-factory/blob/main/src/tui/dashboard.ts).
- Stable task state and volatile pane identity: [`internal/store/task.go`](https://github.com/sean1588/herdr-orchestrator/blob/main/internal/store/task.go); its current shared polling event hub: [`internal/exec/events.go`](https://github.com/sean1588/herdr-orchestrator/blob/main/internal/exec/events.go).
- Least-authority MCP manifest and strict validation: [`src/manifest.rs`](https://github.com/54rt1n/herdr-simple-mcp/blob/master/src/manifest.rs).
- Official plugin-shape references: GitHub Start [`herdr-plugin.toml`](https://github.com/ogulcancelik/herdr-plugin-github-start/blob/master/herdr-plugin.toml), Herdr Plus [`herdr-plugin.toml`](https://github.com/cloudmanic/herdr-plus/blob/main/herdr-plugin.toml), and official examples [`agent-telegram-notify/herdr-plugin.toml`](https://github.com/ogulcancelik/herdr-plugin-examples/blob/main/agent-telegram-notify/herdr-plugin.toml).
- Event/daemon/dashboard split: telemetry bridge [`herdr-plugin.toml`](https://github.com/CodyBontecou/herdr-telemetry-bridge/blob/main/herdr-plugin.toml); metadata/lifecycle daemon surface: herdwatch [`herdr-plugin.toml`](https://github.com/vaclavik-xyz/herdwatch/blob/main/herdr-plugin.toml).
- Worktree setup/teardown event coverage: lifecycle [`herdr-plugin.toml`](https://github.com/qdentity/herdr-worktree-lifecycle/blob/main/herdr-plugin.toml); guarded reset contract: [`specs/worktree-fresh.md`](https://github.com/persiyanov/herdr-fresh-worktree/blob/main/specs/worktree-fresh.md); cached create/remove commands: event-hook [`herdr-plugin.toml`](https://github.com/ynny-github/herdr-event-hook/blob/main/herdr-plugin.toml); layout deduplication/event coverage: workspace-manager [`herdr-plugin.toml`](https://github.com/razajamil/herdr-plugin-workspace-manager/blob/main/herdr-plugin.toml).

## Run & orchestrate agents (36)

- [`ogulcancelik/herdr · SKILL.md`](https://github.com/ogulcancelik/herdr/blob/master/SKILL.md) — Canonical in-pane control guidance. **Surface:** CLI workspace/tab/pane/agent/read/wait/worktree. **Verdict: steal-pattern** — adopt its safety, targeting, and wait rules as baseline.
- [`yigitkonur/herdr-pm`](https://github.com/yigitkonur/herdr-pm) — Curated as a per-tab technical-PM conductor. **Surface:** agent/tab inspection and steering. **Verdict: competitor** — orchestration overlap, but README currently 404; unverified beyond the list.
- [`msadig/herdr-peer-agents-skill`](https://github.com/msadig/herdr-peer-agents-skill) — Skill/wrapper for named peer spawn, prompt, wait, and read. **Surface:** `agent start/get/wait/read`, pane fallback. **Verdict: competitor** — a thin delegation alternative without durable team state.
- [`hcaiano/skills`](https://github.com/hcaiano/skills) — Skill collection containing Claude/Codex peer pairing and structured exchanges. **Surface:** pane/agent CLI plus skill protocols. **Verdict: competitor** — heterogeneous peer protocol overlaps mesh teams.
- [`SecretAardvark/pi-overseer`](https://github.com/SecretAardvark/pi-overseer) — Curated as role-constrained Pi fleets in Jujutsu worktrees with durable state. **Surface:** workspaces/worktrees and role guards. **Verdict: competitor** — strong team/task overlap; README currently 404.
- [`Jackliu-miaozi/pi-herdr-workflow-kit`](https://github.com/Jackliu-miaozi/pi-herdr-workflow-kit) — Planner/coder/reviewer pipeline with file handoffs and phase gates. **Surface:** Herdr panes plus `.pi-herdr` artifacts. **Verdict: competitor** — narrower, Pi-specific team workflow.
- [`mcdonc/mcdonc-pi-herdr`](https://github.com/mcdonc/mcdonc-pi-herdr) — Pi background jobs and conversation forks surfaced as panes/tabs. **Surface:** socket API pane/tab creation and eventual resume. **Verdict: integration-target** — useful Pi launcher/resume adapter.
- [`ogulcancelik/pi-extensions`](https://github.com/ogulcancelik/pi-extensions) — First-party Pi extension suite with Herdr-native orchestration. **Surface:** pane/tab/workspace CLI/API. **Verdict: integration-target** — reference Pi adapter and distribution channel, not our run-board.
- [`aldrickdev/herdr_subagents`](https://github.com/aldrickdev/herdr_subagents) — Visible Pi children in a shared `subagents` tab with spawn/steer/read tools. **Surface:** Herdr-managed panes and Pi event/tool APIs. **Verdict: competitor** — Pi-only subteam lifecycle.
- [`LittleDrinks/herdr-orchestrator-skill`](https://github.com/LittleDrinks/herdr-orchestrator-skill) — Coordinator-only skill with bounded workers, handoffs, review gates, and blocking waits. **Surface:** raw Herdr CLI. **Verdict: competitor** — closest skill-form god/worker model.
- [`luweiCN/herdr-ops`](https://github.com/luweiCN/herdr-ops) — Progressive-disclosure natural-language operations, including worktrees. **Surface:** official-skill CLI commands. **Verdict: integration-target** — could expose team verbs through its conversational layer.
- [`sarmientoF/herdr-pr-loop`](https://github.com/sarmientoF/herdr-pr-loop) — Durable tester/coder/reviewer local and PR loops. **Surface:** Herdr tabs/workspaces plus file state. **Verdict: competitor** — pipeline task lifecycle overlaps teams.
- [`david-lutz/herdr-claude-teams`](https://github.com/david-lutz/herdr-claude-teams) — Translates Claude experimental team tmux calls into native Herdr panes. **Surface:** Herdr socket, metadata, notifications, resume. **Verdict: competitor** — strongest Claude-only native-team alternative.
- [`ogulcancelik/herdr-plugin-github-start`](https://github.com/ogulcancelik/herdr-plugin-github-start) — Starts/renames/prompts an agent tab from a GitHub item. **Surface:** manifest action/overlay and `pane run` with idle gate. **Verdict: integration-target** — natural `team spawn` issue intake.
- [`cloudmanic/herdr-plus`](https://github.com/cloudmanic/herdr-plus) — Declarative projects and quick-action launcher. **Surface:** manifest actions/panes and worktree events. **Verdict: integration-target** — distribute team actions and layouts through it.
- [`firegnu/herdr-loop-lab`](https://github.com/firegnu/herdr-loop-lab) — Mechanical-gate/adversarial-judge loops, parallel fleets, and epic integration. **Surface:** worktrees, tabs, agent CLI, disk state. **Verdict: competitor** — broader convergence engine; steal its acceptance gates.
- [`Tudor0404/dual-author`](https://github.com/Tudor0404/dual-author) — Issue→Claude implementation→dual review→merge pipeline with dependency dashboard. **Surface:** worktrees, panes, renames, reads/focus, Textual pane. **Verdict: competitor** — direct roadmap overlap across roles, DAG, and dashboard.
- [`razajamil/herdr-factory`](https://github.com/razajamil/herdr-factory) — Heterogeneous, schema-checked ticket-to-PR belts with queue, attention, resume, and dashboard. **Surface:** worktrees/workspaces/panes plus server/TUI. **Verdict: competitor** — broadest production orchestration overlap.
- [`tomoasleep/herdr-symphony`](https://github.com/tomoasleep/herdr-symphony) — Tracker-driven headless issue workers with report-file completion. **Surface:** workspace create, agent start/wait/send, pane read, worktrees. **Verdict: competitor** — task-board and report protocol overlap.
- [`madarco/agentbox-herdr-plugin`](https://github.com/madarco/agentbox-herdr-plugin) — AgentBox VM overlay/launcher/link handler. **Surface:** manifest actions/pane/link and generated shim. **Verdict: integration-target** — optional sandboxed launcher backend.
- [`joelhooks/pi-bellwether`](https://github.com/joelhooks/pi-bellwether) — Generic Pi tools for start/send/read/focus/stop. **Surface:** agent/pane/session CLI. **Verdict: integration-target** — could wrap team operations rather than reinventing control.
- [`NickPittas/pi-herdr-subagents`](https://github.com/NickPittas/pi-herdr-subagents) — Dashboard over async Pi subagent events and session files. **Surface:** Pi event bus, tabs/panes/focus. **Verdict: steal-pattern** — non-invasive tracking and open/focus UX for our dashboard.
- [`kirel/herdr-subagents`](https://github.com/kirel/herdr-subagents) — Per-child Pi panes/tabs/workspaces with parent completion callbacks. **Surface:** pane/session launch and injected callback. **Verdict: competitor** — Pi-specific child lifecycle and push completion.
- [`gustavocaiano/opencode-herdr`](https://github.com/gustavocaiano/opencode-herdr) — Mirrors OpenCode subagents into tiled attach panes. **Surface:** OpenCode events plus pane split/close. **Verdict: integration-target** — visibility adapter for an eventual OpenCode launcher.
- [`machine-machine/herdr-factory-loop-skill`](https://github.com/machine-machine/herdr-factory-loop-skill) — Spec-driven mixed-agent fleet with disk context, dispatch/collect, hooks, and TUI. **Surface:** workspaces/worktrees, hooks, file protocols. **Verdict: competitor** — direct fleet/run-board overlap.
- [`machine-machine/ask-fable-skill`](https://github.com/machine-machine/ask-fable-skill) — Delegates a complex task by file protocol to resumable Claude. **Surface:** agent pane, file sentinel, session UUID. **Verdict: competitor** — single-worker subset; strong restart/payload pattern.
- [`yangyang0507/herdr-skill`](https://github.com/yangyang0507/herdr-skill) — Non-blocking structured `request|reply|update` messages with task/reply-to. **Surface:** `pane run` and target resolution. **Verdict: steal-pattern** — enrich our `msg` envelope and keep deliver-then-stop semantics.
- [`bakescakes/claude-orchestration`](https://github.com/bakescakes/claude-orchestration) — Curated as five Claude orchestration skills and lifecycle hooks. **Surface:** panes/worktrees/hooks. **Verdict: competitor** — broad overlap, but README currently 404.
- [`0x5c0f/herdr-insight`](https://github.com/0x5c0f/herdr-insight) — Multi-workspace agent-state timeline with history. **Surface:** plugin pane and agent/session status. **Verdict: steal-pattern** — dashboard timeline/history reference.
- [`rohanthewiz/herdr-todo`](https://github.com/rohanthewiz/herdr-todo) — Atomic project/global prompt backlog that dispatches into existing/new agents. **Surface:** plugin pane, socket, tab creation/input. **Verdict: integration-target** — intake source for task-board tickets.
- [`freewillythe4th/action-button-agent`](https://github.com/freewillythe4th/action-button-agent) — Phone dictation→Telegram→operator→Herdr lanes. **Surface:** wrapper-driven agent lanes. **Verdict: integration-target** — remote team-task intake, not core orchestration.
- [`erwins-enkel/shepherd`](https://github.com/erwins-enkel/shepherd) — Browser mission control for worktree agents with plan/review/merge gates and resume. **Surface:** Herdr worktrees/panes/status polling and PTY bridge. **Verdict: competitor** — supervised heterogeneous fleet product.
- [`carze/herdr-smolmachine`](https://github.com/carze/herdr-smolmachine) — MicroVM-sandboxed agent launcher with Herdr state hinting. **Surface:** manifest actions, generated shim, `HERDR_AGENT`. **Verdict: integration-target** — isolation launcher option.
- [`sean1588/herdr-orchestrator`](https://github.com/sean1588/herdr-orchestrator) — Deterministic YAML state graph, SQLite single-writer audit, GitHub gates, recovery. **Surface:** Herdr execution backend and currently shared pane-list poller. **Verdict: competitor** — deepest task-state/restart overlap.
- [`saiashirwad/homestead`](https://github.com/saiashirwad/homestead) — Per-issue worktrees with isolated ports/env/setup, agents, landing, and dashboard. **Surface:** Herdr panes/worktrees/status. **Verdict: competitor** — worker worktree lifecycle plus fleet view.
- [`noor-latif/herd`](https://github.com/noor-latif/herd) — Project-scoped N-agent Pi grid with relaunch and state files. **Surface:** workspace/tab/pane creation and waits. **Verdict: competitor** — simple static team launcher.

## Connect over socket & MCP (21)

- [`54rt1n/herdr-python-client`](https://github.com/54rt1n/herdr-python-client) — Zero-dependency typed Python socket client with waits/subscriptions/raw calls. **Surface:** NDJSON Unix socket. **Verdict: integration-target** — external automation client for team verbs/events.
- [`eugeneb50/herdr-mcp`](https://github.com/eugeneb50/herdr-mcp) — MCP/HTTP bridge with recipes, scheduling, A2A tools, and playground. **Surface:** CLI-backed MCP tools. **Verdict: integration-target** — expose team actions through recipes rather than duplicate them.
- [`runchr-works/herdr-mesh`](https://github.com/runchr-works/herdr-mesh) — Agent-agnostic MCP spawn/read/relay/handoff/wait. **Surface:** MCP over Herdr CLI. **Verdict: competitor** — generic orchestration transport, lacking our team/run ownership.
- [`jerryfane/herdr-codex-usage-kit`](https://github.com/jerryfane/herdr-codex-usage-kit) — Codex quota labels and dashboard from local logs. **Surface:** pane metadata plus service/pane. **Verdict: irrelevant** — accounting, not team lifecycle.
- [`ogulcancelik/herdr-plugin-examples`](https://github.com/ogulcancelik/herdr-plugin-examples) — Official small event/action/pane/link/build examples. **Surface:** plugin manifest API. **Verdict: steal-pattern** — canonical authoring and packaging reference.
- [`gaijinjoe/herdres`](https://github.com/gaijinjoe/herdres) — Telegram topic bridge with structured turns, pointer delivery, and event-triggered reconcile. **Surface:** status event hook, pane input, state mapping. **Verdict: integration-target** — remote team inbox/approval channel.
- [`54rt1n/herdr-simple-mcp`](https://github.com/54rt1n/herdr-simple-mcp) — Stateless direct-socket MCP with coordinator/client/observer profiles. **Surface:** 75 socket methods and role filters. **Verdict: steal-pattern** — least-authority role surfaces and strict manifests.
- [`lib-x/herdr-sock-go`](https://github.com/lib-x/herdr-sock-go) — Generated typed Go socket client with protocol check/subscriptions/raw calls. **Surface:** NDJSON socket. **Verdict: integration-target** — Go consumers of future team API.
- [`CodyBontecou/herdr-telemetry-bridge`](https://github.com/CodyBontecou/herdr-telemetry-bridge) — Versioned, privacy-first NDJSON telemetry to file/webhook/stdin. **Surface:** multiple events plus daemon/dashboard panes. **Verdict: steal-pattern** — event envelopes, redaction, sink separation, reconciliation.
- [`junliu-mde/mimo-code-herdr-plugin`](https://github.com/junliu-mde/mimo-code-herdr-plugin) — Aggregate MiMo lifecycle reporter with crash watchdog. **Surface:** `pane.report_agent/release_agent`. **Verdict: integration-target** — launcher/status support; source for lifecycle coexistence rules.
- [`zom-2018/herdr-ntfy-notify`](https://github.com/zom-2018/herdr-ntfy-notify) — Cross-device blocked/done notifications. **Surface:** status event manifest and plugin config. **Verdict: integration-target** — optional team attention sink.
- [`tiny-send/tinysend-herdr`](https://github.com/tiny-send/tinysend-herdr) — Status email plus correlated reply watcher. **Surface:** event hook, watcher pane, pane input. **Verdict: integration-target** — human approval/reply channel.
- [`yankewei/herdr-focus-notify`](https://github.com/yankewei/herdr-focus-notify) — Focus-aware clickable macOS alerts. **Surface:** status event and `agent focus`. **Verdict: integration-target** — team blocked/done notification sink.
- [`dot/herdr-terminal-notifier`](https://github.com/dot/herdr-terminal-notifier) — Branded, templated, debounced click-to-pane macOS alerts. **Surface:** status event, plugin state/build. **Verdict: steal-pattern** — debounce/grouping and event diagnostics.
- [`dcolinmorgan/herdr-push`](https://github.com/dcolinmorgan/herdr-push) — Zero-dependency event relay to mobile approval clients. **Surface:** status event JSON and HTTP. **Verdict: integration-target** — forward team attention events.
- [`razajamil/herdr-hex-browser-voice-command`](https://github.com/razajamil/herdr-hex-browser-voice-command) — URL-context voice routing to workspace/tab/pane. **Surface:** CLI resolution and pane submit. **Verdict: integration-target** — voice dispatch into named workers.
- [`klittle32/letta-herdr-mod`](https://github.com/klittle32/letta-herdr-mod) — Debounced Letta lifecycle integration with source-scoped release. **Surface:** socket report/release plus custom status. **Verdict: integration-target** — future Letta launcher policy.
- [`Phoobobo/herdr-traex-integration`](https://github.com/Phoobobo/herdr-traex-integration) — TraeX hooks plus bounded question-modal watcher. **Surface:** report/release socket calls and plugin installer action. **Verdict: integration-target** — future TraeX launcher/status support.
- [`vaclavik-xyz/herdwatch`](https://github.com/vaclavik-xyz/herdwatch) — Extends visible work past agent idle for CI/reviews and publishes progress. **Surface:** snapshot/subscriptions, report-agent or metadata, daemon pane. **Verdict: steal-pattern** — authoritative evidence for metadata-only progress pings.
- [`carsonjones/herdr-agent-dashboard`](https://github.com/carsonjones/herdr-agent-dashboard) — Live all-agent terminal table. **Surface:** CLI polling and plugin action. **Verdict: steal-pattern** — minimal dashboard baseline.
- [`alexei-led/ccgram`](https://github.com/alexei-led/ccgram) — Telegram terminal topics for several agents via Herdr/tmux. **Surface:** pane output/input and hooks. **Verdict: integration-target** — remote team steering adapter.

## Editor integrations (7)

- [`devxplay/herdr.nvim`](https://github.com/devxplay/herdr.nvim) — Crosses Neovim/Herdr navigation with marker/cache files. **Surface:** socket focus/layout/send-text. **Verdict: irrelevant** — editor navigation.
- [`MomePP/herd.nvim`](https://github.com/MomePP/herd.nvim) — Neovim frontend over Herdr-owned persistent agents. **Surface:** agent list/start/attach/send and workspaces/tabs. **Verdict: irrelevant** — editor UX, not team semantics.
- [`paulbkim-dev/vim-herdr-navigation`](https://github.com/paulbkim-dev/vim-herdr-navigation) — vim-tmux-navigator-style pane crossing. **Surface:** process-info, send-keys, focus actions. **Verdict: irrelevant**.
- [`lmilojevicc/herdr-splits.nvim`](https://github.com/lmilojevicc/herdr-splits.nvim) — Unified split navigation/resizing and unzoom. **Surface:** plugin actions and pane focus/resize. **Verdict: irrelevant**.
- [`luiarthur/herdr.vim`](https://github.com/luiarthur/herdr.vim) — Sends lines/files to Herdr-hosted REPL panes. **Surface:** pane split/input. **Verdict: irrelevant**.
- [`UN-9BOT/sidekick_herdr`](https://github.com/UN-9BOT/sidekick_herdr) — Herdr session backend for sidekick.nvim. **Surface:** agent start/list, pane send/read/get. **Verdict: irrelevant** — generic editor agent launcher.
- [`Daniel-Steinberger/obsidian-herdr`](https://github.com/Daniel-Steinberger/obsidian-herdr) — Dispatches Markdown checklist items and checks them on completion. **Surface:** socket/CLI workspace matching, send, `agent wait`. **Verdict: integration-target** — external authoritative task-board adapter.

## Sessions: switch & restore (8)

- [`ridho9/switchr`](https://github.com/ridho9/switchr) — Session/tree picker and attach launcher. **Surface:** session list/attach/restart. **Verdict: irrelevant**.
- [`j0urneyk/herdrctx`](https://github.com/j0urneyk/herdrctx) — TUI session create/attach/stop/delete with confirmation. **Surface:** session CLI. **Verdict: irrelevant**.
- [`nickmaglowsch/herdr-session-restore`](https://github.com/nickmaglowsch/herdr-session-restore) — Clean-stop layout snapshot and Claude `--resume`. **Surface:** wrapper over server/session/workspace/tab/agent. **Verdict: steal-pattern** — stable session ID plus layout restoration.
- [`thanhdat77/herdr-picker-plus`](https://github.com/thanhdat77/herdr-picker-plus) — Unified navigator for workspaces, agents, session IDs, projects, SSH, and plugins. **Surface:** plugin overlay plus broad CLI discovery. **Verdict: integration-target** — expose teams/workers in its index.
- [`andrewchng/herdr-sessionizer`](https://github.com/andrewchng/herdr-sessionizer) — Fuzzy project/worktree opener with TOML layouts. **Surface:** plugin actions and workspace/worktree creation. **Verdict: integration-target** — optional team layout/intake surface.
- [`alon-z/herdr-command-palette`](https://github.com/alon-z/herdr-command-palette) — Lightweight workspace/directory switcher. **Surface:** workspace list/focus/create. **Verdict: irrelevant**.
- [`third774/herdr-last-workspace`](https://github.com/third774/herdr-last-workspace) — Stable-ID previous-workspace toggle. **Surface:** workspace focus/closed events. **Verdict: irrelevant**.
- [`maayanyosef/herdr-aws-ssm`](https://github.com/maayanyosef/herdr-aws-ssm) — SSM-tunneled remote Herdr sessions. **Surface:** `herdr --remote`. **Verdict: irrelevant** — transport infrastructure.

## Worktrees, config & terminal UX (41)

- [`noamsiegel/git-wt-herdr`](https://github.com/noamsiegel/git-wt-herdr) — Reference `git-wt.plugin.v0` lifecycle bridge. **Surface:** workspace/tab create/close/focus. **Verdict: integration-target** — pluggable worktree provider.
- [`SirTenzin/superherd`](https://github.com/SirTenzin/superherd) — Superset worktree/setup-terminal bridge. **Surface:** Herdr workspace/tab CLI. **Verdict: integration-target** — external provisioning backend.
- [`justcyl/pi-herdr-tab-sync`](https://github.com/justcyl/pi-herdr-tab-sync) — Renames tabs from Pi session names. **Surface:** direct socket tab rename. **Verdict: irrelevant**.
- [`yigitkonur/native-shortcuts-herd`](https://github.com/yigitkonur/native-shortcuts-herd) — Ghostty/macOS shortcut installer with backup/uninstall. **Surface:** Herdr config/key actions. **Verdict: irrelevant**.
- [`Taeyoung96/herdr-dotfiles`](https://github.com/Taeyoung96/herdr-dotfiles) — Prefix-free config/theme/global agent panel. **Surface:** config/keybindings. **Verdict: irrelevant**.
- [`mattarau/wt-herdr`](https://github.com/mattarau/wt-herdr) — Worktrunk worktree/workspace sync, health, hooks, focus, clean. **Surface:** workspace/session CLI and Worktrunk events. **Verdict: integration-target** — alternative provider.
- [`liu-qingyuan/herdr-tmux-local-config`](https://github.com/liu-qingyuan/herdr-tmux-local-config) — Dotfiles and Codex/OMX status hooks. **Surface:** report-agent hooks/config. **Verdict: irrelevant** — workstation pack.
- [`qdentity/herdr-worktree-lifecycle`](https://github.com/qdentity/herdr-worktree-lifecycle) — Repo-owned setup/teardown ABI with per-path serialization and logs. **Surface:** worktree events, metadata, log pane. **Verdict: steal-pattern** — upgrade our setup lifecycle.
- [`shizlie/herdr-setup-bootstrap`](https://github.com/shizlie/herdr-setup-bootstrap) — TOML setup and gitignored-file copy with idempotency marker. **Surface:** workspace create/focus events. **Verdict: steal-pattern** — bootstrap/copy semantics.
- [`persiyanov/herdr-fresh-worktree`](https://github.com/persiyanov/herdr-fresh-worktree) — Guarded reset of truly fresh worktrees to remote HEAD. **Surface:** `worktree.created` event. **Verdict: steal-pattern** — explicit concurrency/idempotency/salvage safety bar.
- [`razajamil/herdr-plugin-workspace-manager`](https://github.com/razajamil/herdr-plugin-workspace-manager) — Branch-selected YAML layouts, blocking setup, gone-worktree cleanup. **Surface:** worktree/workspace events, actions, panes. **Verdict: integration-target** — layout provider; also race-handling reference.
- [`alon-z/herdr-devup`](https://github.com/alon-z/herdr-devup) — Project dev-stack up/sync/down with owned-resource state. **Surface:** actions, tabs/panes, plugin state. **Verdict: steal-pattern** — “close exactly what you created” and executable-config warnings.
- [`peterferguson/herdr-conductor-worktree`](https://github.com/peterferguson/herdr-conductor-worktree) — Conductor/Herdr worktree creation, registration, archive, reconciliation. **Surface:** worktree/workspace CLI and external DB. **Verdict: integration-target**.
- [`NathanFlurry/herdr-plugin-jj-workspace`](https://github.com/NathanFlurry/herdr-plugin-jj-workspace) — Jujutsu workspace create/remove in Herdr tab/workspace. **Surface:** plugin actions and workspace/tab creation. **Verdict: integration-target** — future non-git provider.
- [`devashish2203/herdr-worktrunk`](https://github.com/devashish2203/herdr-worktrunk) — Interactive Worktrunk create/switch/remove with hooks and dirty gates. **Surface:** actions/panes and worktree open. **Verdict: integration-target**.
- [`kkckkc/herdr-plugin-gh-workflow`](https://github.com/kkckkc/herdr-plugin-gh-workflow) — Issue→branch→worktree→configured workspace. **Surface:** actions/overlay, worktree/workspace/tab CLI. **Verdict: integration-target** — issue-backed team spawn.
- [`persiyanov/herdr-reviewr`](https://github.com/persiyanov/herdr-reviewr) — Read-only diff/PR sidebar with comments sent to an agent. **Surface:** worktree event, plugin pane, agent input. **Verdict: integration-target** — reviewer-worker surface.
- [`arjenblokzijl/herdr-launcher`](https://github.com/arjenblokzijl/herdr-launcher) — Typed TOML workflow forms available as TUI and CLI. **Surface:** plugin action/pane. **Verdict: integration-target** — interactive team-spec launcher.
- [`JanTvrdik/herdr-command-palette`](https://github.com/JanTvrdik/herdr-command-palette) — Fuzzy overlay over all plugin actions. **Surface:** action list/invoke and overlay pane. **Verdict: integration-target** — discoverability for team actions.
- [`smarzban/herdr-file-viewer`](https://github.com/smarzban/herdr-file-viewer) — Safe read-only git/file/diff TUI. **Surface:** plugin split/tab and worktree selection. **Verdict: irrelevant**.
- [`devskale/herdr-flist`](https://github.com/devskale/herdr-flist) — Cwd-following local/SSH file sidebar. **Surface:** focused pane/cwd and split. **Verdict: irrelevant**.
- [`x0d7x/herdr-fzf-url`](https://github.com/x0d7x/herdr-fzf-url) — URL picker across pane output. **Surface:** pane list/read and plugin pane. **Verdict: irrelevant**.
- [`rmarganti/herdr-pluck`](https://github.com/rmarganti/herdr-pluck) — Keyboard-hint token copier. **Surface:** pane capture and overlay/action. **Verdict: irrelevant**.
- [`beomjungil/herdr-lazygit-overlay`](https://github.com/beomjungil/herdr-lazygit-overlay) — Cwd-preserving lazygit overlay. **Surface:** plugin pane/action. **Verdict: irrelevant**.
- [`edmundmiller/herdr-plugin-hunk`](https://github.com/edmundmiller/herdr-plugin-hunk) — Hunk worktree/staged/branch diff panes/tabs. **Surface:** plugin actions. **Verdict: integration-target** — optional reviewer view.
- [`carsonjones/herdr-plugin-tiles`](https://github.com/carsonjones/herdr-plugin-tiles) — Named split-ratio actions. **Surface:** pane layout/plugin actions. **Verdict: irrelevant**.
- [`kamaaina/herdr_sync`](https://github.com/kamaaina/herdr_sync) — Broadcasts a composed command to peer panes. **Surface:** pane input across current tab. **Verdict: irrelevant** — unsafe/unstructured compared with team `msg`.
- [`twadams21/cc-controller`](https://github.com/twadams21/cc-controller) — Game-controller mapping to local/remote Herdr. **Surface:** socket/CLI navigation/input. **Verdict: irrelevant**.
- [`rjyo/herdr-window-title-sync`](https://github.com/rjyo/herdr-window-title-sync) — Outer terminal title from metadata/session prompts. **Surface:** focus events, pane metadata/session files. **Verdict: irrelevant**.
- [`krystof018/herdr-git-status`](https://github.com/krystof018/herdr-git-status) — Fleet-wide CI/PR status labels and detail pane. **Surface:** workspace labels, poller, plugin pane. **Verdict: integration-target** — authoritative task/merge-gate status feed.
- [`sohanemon/herdr-helpr`](https://github.com/sohanemon/herdr-helpr) — Workspace/tab naming and close-others overlays. **Surface:** plugin actions/panes. **Verdict: irrelevant**.
- [`fkiene/llmtrim-herdr`](https://github.com/fkiene/llmtrim-herdr) — Proxy bootstrap, savings metadata, and dashboard. **Surface:** workspace/agent events, custom status, pane. **Verdict: irrelevant** — token accounting.
- [`Davidcreador/herdr-token-dashboard`](https://github.com/Davidcreador/herdr-token-dashboard) — Live per-agent cost/token/tool dashboard. **Surface:** session files/APIs, status events/polling, pane. **Verdict: steal-pattern** — dashboard data-source fallback and completion toast design.
- [`wyattjoh/herdr-plugin-renamer`](https://github.com/wyattjoh/herdr-plugin-renamer) — First-prompt tab/branch/workspace renamer. **Surface:** status event, session metadata, plugin state. **Verdict: steal-pattern** — hot-path bail, atomic claim, detached cold path.
- [`ynny-github/herdr-event-hook`](https://github.com/ynny-github/herdr-event-hook) — Repo TOML commands on worktree create/remove. **Surface:** worktree events and cached plugin state. **Verdict: steal-pattern** — cache teardown policy before checkout deletion.
- [`mkdir700/herdr-config`](https://github.com/mkdir700/herdr-config) — Portable config plus local diff/path/lazygit/PR plugins. **Surface:** config/actions/metadata. **Verdict: irrelevant**.
- [`alexjsp/herdr-scrollback-capture`](https://github.com/alexjsp/herdr-scrollback-capture) — Saves pane scrollback to HTML/text. **Surface:** pane read and notification. **Verdict: irrelevant**.
- [`akhillb/herdr-attention`](https://github.com/akhillb/herdr-attention) — Persistent NOW/SOON/WATCHING attention feed. **Surface:** auto-docked plugin pane and state. **Verdict: steal-pattern** — task-board triage/attention UX.
- [`ppggff/herdr-plugin`](https://github.com/ppggff/herdr-plugin) — Per-pane macOS input-source keeper/dashboard. **Surface:** focus events and pane. **Verdict: irrelevant**.
- [`astkaasa/herdr-tokscale-dashboard`](https://github.com/astkaasa/herdr-tokscale-dashboard) — Thin adapter opening Tokscale TUI/JSON action. **Surface:** plugin pane/action. **Verdict: irrelevant**.
- [`aiki-sh/aiki-integration-herdr`](https://github.com/aiki-sh/aiki-integration-herdr) — Aiki epic sidebar plus companion session-identity hook bootstrap. **Surface:** plugin build/pane and harness hook. **Verdict: integration-target** — external epic/task-board adapter.

## Desktop apps & packaging (14)

- [`hmu332233/herdr-menu-bar`](https://github.com/hmu332233/herdr-menu-bar) — Workspace-grouped ambient agent status and focus. **Surface:** CLI/socket discovery with adaptive polling. **Verdict: integration-target** — display team/run grouping later.
- [`re2zero/deepin-herdr`](https://github.com/re2zero/deepin-herdr) — Deepin/UOS embedded-terminal package; repository intentionally has no README. **Surface:** bundled/launched Herdr client/server. **Verdict: irrelevant**.
- [`AodhanHayter/herdr-nix`](https://github.com/AodhanHayter/herdr-nix) — Auto-updated Nix packaging/cache. **Surface:** Herdr binary distribution. **Verdict: irrelevant**.
- [`re2zero/zenix`](https://github.com/re2zero/zenix) — GPUI desktop frontend with system metrics/themes. **Surface:** bundled binary/socket/PTY. **Verdict: irrelevant**.
- [`kcosr/herdr-web`](https://github.com/kcosr/herdr-web) — Experimental browser terminal/agent UI using vendored private compatibility code. **Surface:** socket, attach, events. **Verdict: integration-target** — possible external team UI, but API drift risk.
- [`lachieh/vfox-herdr`](https://github.com/lachieh/vfox-herdr) — Verified mise/vfox installer and dynamic completions. **Surface:** release API and CLI completion. **Verdict: irrelevant**.
- [`alecuba16/herdr-webui`](https://github.com/alecuba16/herdr-webui) — Browser UI with built-in or external Herdr-compatible backend. **Surface:** terminal/API sockets and worktree UI. **Verdict: irrelevant** — it substitutes/generalizes the terminal backend.
- [`dcolinmorgan/herdr-remote`](https://github.com/dcolinmorgan/herdr-remote) — Menu bar/PWA/Telegram remote status, terminal, and approvals. **Surface:** push relay, WebSocket, pane input. **Verdict: integration-target** — remote team attention/approval.
- [`zackbart/herdr-ios`](https://github.com/zackbart/herdr-ios) — Native iOS client speaking NDJSON over SSH. **Surface:** socket methods/events/scrollback/input. **Verdict: integration-target** — team dashboard client after a team API exists.
- [`aviz85/herdr-controller`](https://github.com/aviz85/herdr-controller) — Web agent grid/message/spawn plus 3D office. **Surface:** HTTP/SSE façade over Herdr CLI. **Verdict: steal-pattern** — thin external dashboard API and status stream.
- [`timvdhoorn/stream-deck-herdr-plugin`](https://github.com/timvdhoorn/stream-deck-herdr-plugin) — Physical status/focus/attention pager. **Surface:** pushed socket events plus slow poll. **Verdict: steal-pattern** — priority ordering and push-plus-reconcile.
- [`zhongpei/herdr-ulanzi-deck`](https://github.com/zhongpei/herdr-ulanzi-deck) — Multi-machine LCD agent dashboard. **Surface:** CLI polling, NATS snapshots, SSH. **Verdict: irrelevant** — hardware UI.
- [`jgwesterlund/agent-view`](https://github.com/jgwesterlund/agent-view) — Pixel-art ambient agent office. **Surface:** agent-list polling/focus. **Verdict: irrelevant**.
- [`AltanS/collie`](https://github.com/AltanS/collie) — Tailscale PWA for status, terminal mirror, replies, and push. **Surface:** thin plugin launcher and one socket bridge. **Verdict: integration-target** — remote team steering surface.

## Work in progress (6)

- [`rbb/herdr-cursor`](https://github.com/rbb/herdr-cursor) — Curated as a Cursor lifecycle-reporting design. **Surface:** planned status integration. **Verdict: integration-target** — future Cursor launcher, but README currently 404.
- [`shippy/raycast-herdr`](https://github.com/shippy/raycast-herdr) — Raycast commands now implement Ask Claude, run action, and workspace focus. **Surface:** Herdr CLI actions/workspaces. **Verdict: integration-target** — launch team actions; curated “empty scaffold” text is stale.
- [`SuperInstance/herdr-cocapn`](https://github.com/SuperInstance/herdr-cocapn) — Fleet fork with device tiers, deadband escalation, and crossfade. **Surface:** forked core agent/pane management. **Verdict: steal-pattern** — capability/cost escalation idea only; hardcoded dependency makes it non-buildable.
- [`rohanthewiz/herdr-web`](https://github.com/rohanthewiz/herdr-web) — Alpha browser renderer/terminal transport. **Surface:** terminal attach wire protocol. **Verdict: irrelevant** — not orchestration and input remains gated in live use.
- [`Matovidlo/herdr-pr-tracker`](https://github.com/Matovidlo/herdr-pr-tracker) — Polling PR board with plan notes and merge/checkout actions. **Surface:** agent list/pane read/event hook plus `gh`. **Verdict: steal-pattern** — dashboard PR correlation, but install/namespace placeholders show it is unfinished.
- [`makyinmars/muster`](https://github.com/makyinmars/muster) — Planned native command center with Herdr sessions and MCP coordination. **Surface:** proposed socket/CLI/MCP/terminal UI. **Verdict: integration-target** — future external orchestration UI, currently documentation only.

## Bottom line

Do not turn `herdr-agent-team` into a general Herdr dashboard, worktree manager, remote terminal, or ticket-to-merge factory. Keep the deep module boundary: team topology, owned/borrowed worker lifecycle, durable run/task/message state, and a small set of composable verbs/events. Integrate outward through tracker adapters, MCP/tool profiles, worktree providers, notification sinks, and dashboard clients.

The next highest-value sequence is: (1) `team wait`, because it closes a demonstrated coordination failure; (2) a thin dashboard over the existing run-board; (3) stable-session restart; (4) task-board state and dependencies; (5) metadata-only progress. Before adding more launchers, harden worktree teardown/logging/idempotency and enrich `msg` with message/task/reply correlation.

READY FOR REVIEW
