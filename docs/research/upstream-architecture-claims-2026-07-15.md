# Upstream Herdr architecture and claim verification (2026-07-15)

Scope: local upstream checkout `/home/caio/Projects/herdr-upstream` (Herdr
0.7.3) and the plugin baseline in `CLAUDE.md` “Verified facts” plus
`docs/spec.md` section 9. All verdicts below are based on the checked-out source,
not inferred from behavior. Context7 resolved the official project as
`/ogulcancelik/herdr`, but its protocol snippets were stale at version 15;
the local source and this plugin's checked-in schema snapshot both say 16, so
the local source is decisive here.

Source paths below are relative to `/home/caio/Projects/` and use the prefixes
`herdr-upstream/` and `herdr-agent-team/`.

## Part A — architecture map

### 1. Language and subsystem ownership

The premise “Zig core + Rust” needs qualification. The Herdr multiplexer and
application core in this checkout are Rust. Zig owns the vendored
`libghostty-vt` terminal emulation engine, which is compiled as a static library
and called through Rust FFI; Zig does **not** own workspace/tab/pane state,
agent detection, plugins, CLI, or Herdr IPC.

| Surface | Owner and flow | Primary evidence |
|---|---|---|
| Process/application composition | Rust `main.rs` declares the app, CLI, client, server, PTY, workspace, detection, plugin, API and protocol modules. | `herdr-upstream/src/main.rs:57-101` |
| Multiplexer/session runtime | Rust owns workspace/tab/pane state, PTY spawning/runtime, layouts, persistence, headless server and TUI/client rendering. The headless server owns PTYs and state, accepts clients, renders frames, and survives client disconnect. | `herdr-upstream/src/server/headless.rs:1-15`; `herdr-upstream/src/pane.rs:1536-1604`; `herdr-upstream/src/main.rs:77-101` |
| Terminal emulation (“Zig core”) | Cargo's Rust build script invokes Zig with `-Demit-lib-vt`, links `libghostty-vt`, and Rust wraps generated bindings as `ghostty::ffi`. This is the parser/screen/terminal engine under panes. | `herdr-upstream/build.rs:32-47`; `herdr-upstream/build.rs:49-92`; `herdr-upstream/src/ghostty/mod.rs:1-29`; `herdr-upstream/src/pane.rs:1850-1864` |
| Agent identity/status detection | Rust identifies foreground processes, reads the bottom-of-buffer snapshot, and evaluates bundled/remote/override TOML screen manifests plus hook-reported lifecycle state. The internal detector has `Idle/Working/Blocked/Unknown`; UI/API derives `Done` from idle + unseen attention state. | `herdr-upstream/src/detect/mod.rs:1-20`; `herdr-upstream/src/detect/mod.rs:41-65`; `herdr-upstream/src/detect/mod.rs:154-227`; `herdr-upstream/src/detect/manifest.rs:16-47`; `herdr-upstream/src/app/api_helpers.rs:62-72` |
| Plugin subsystem | Rust parses `herdr-plugin.toml`, persists the registry, validates hook names, computes invocation context, launches commands/panes, injects environment, and records command logs. There is no plugin SDK; plugin code uses commands plus the Herdr socket/CLI. | `herdr-upstream/src/app/api/plugins/manifest.rs:11-87`; `herdr-upstream/src/app/api/plugins/runtime.rs:15-81`; `herdr-upstream/src/app/api/plugins/runtime.rs:183-227`; `herdr-upstream/src/api/schema.rs:206-227` |
| CLI | Rust hand-parses/dispatches commands and maintains a Clap command specification for help/completions. CLI calls the JSON API for runtime operations. | `herdr-upstream/src/cli/spec.rs:3-45`; `herdr-upstream/src/api/client.rs:32-62` |
| IPC/API | Rust exposes two local sockets: newline-delimited JSON for public automation, and a length-prefixed bincode client protocol for the TUI/terminal stream. | `herdr-upstream/src/server/headless.rs:1-13`; `herdr-upstream/src/api/client.rs:32-62`; `herdr-upstream/src/protocol/wire.rs:306-398`; `herdr-upstream/src/protocol/wire.rs:597-667` |

### 2. Plugin subsystem: complete manifest surface

The manifest is `herdr-plugin.toml` (directory links resolve to that filename).
Root required fields are `id`, `name`, `version`, and `min_herdr_version`.
The raw deserializer represents `min_herdr_version` as optional, but validation
rejects a missing or empty value. Optional root fields are `description`,
`platforms`, and zero or more `build`, `actions`, `events`, `panes`, and
`link_handlers` tables.
(`herdr-upstream/src/app/api/plugins/manifest.rs:11-32`,
`herdr-upstream/src/app/api/plugins/manifest.rs:109-117`,
`herdr-upstream/src/app/api/plugins/manifest.rs:207-218`)

Full table fields:

| TOML surface | Fields |
|---|---|
| root | `id`, `name`, `version`, `min_herdr_version`, `description?`, `platforms?` |
| `[[build]]` | `command`, `platforms?` |
| `[[actions]]` | `id`, `title`, `description?`, `contexts?`, `platforms?`, `command` |
| `[[events]]` | `on`, `platforms?`, `command` |
| `[[panes]]` | `id`, `title`, `description?`, `platforms?`, `placement?`, `width?`, `height?`, `command` |
| `[[link_handlers]]` | `id`, `title`, `pattern`, `action`, `platforms?` |

Evidence: `herdr-upstream/src/app/api/plugins/manifest.rs:34-87` and
`herdr-upstream/src/api/schema/plugins.rs:227-280`. Platform values are
`linux`, `macos`, and `windows`; action contexts are `global`, `workspace`,
`tab`, `pane`, and `selection`
(`herdr-upstream/src/api/schema/plugins.rs:336-352`). Pane placements are
`overlay`, `popup`, `split`, `tab`, and `zoomed`; width/height are accepted only
for `popup` manifests (`herdr-upstream/src/api/schema/plugins.rs:432-443`,
`herdr-upstream/src/app/api/plugins/manifest.rs:384-417`).

#### All hookable `[[events]] on =` names

The complete allowlist has 21 names:

- workspace: `workspace.created`, `workspace.updated`, `workspace.closed`,
  `workspace.renamed`, `workspace.moved`, `workspace.focused`
- worktree: `worktree.created`, `worktree.opened`, `worktree.removed`
- tab: `tab.created`, `tab.closed`, `tab.renamed`, `tab.moved`, `tab.focused`
- pane: `pane.created`, `pane.closed`, `pane.focused`, `pane.moved`,
  `pane.exited`, `pane.agent_detected`, `pane.agent_status_changed`

This is the source allowlist at
`herdr-upstream/src/api/schema/events.rs:281-303`, mapped to dot names by
`herdr-upstream/src/api/schema/events.rs:220-250`. It is intentionally narrower
than the socket `EventKind`: `workspace.metadata_updated`, `pane.updated`,
`pane.output_changed`, and `layout.updated` are **not** plugin-hookable
(`herdr-upstream/src/api/schema/events.rs:190-218`,
`herdr-upstream/src/api/schema/events.rs:314-323`). A test explicitly protects
those exclusions (`herdr-upstream/src/api/schema/events.rs:344-352`).

#### Injected environment

For action, event, and link-handler commands, Herdr injects:

- Always: `HERDR_SOCKET_PATH`, `HERDR_ENV=1`, `HERDR_PLUGIN_ID`,
  `HERDR_PLUGIN_ROOT`, `HERDR_PLUGIN_CONFIG_DIR`,
  `HERDR_PLUGIN_STATE_DIR`, `HERDR_PLUGIN_CONTEXT_JSON`.
- When the executable path is available: `HERDR_BIN_PATH`.
- When context contains them: `HERDR_WORKSPACE_ID`, `HERDR_TAB_ID`,
  `HERDR_PANE_ID`.
- Action-only: `HERDR_PLUGIN_ACTION_ID`.
- Event-only: `HERDR_PLUGIN_EVENT`, `HERDR_PLUGIN_EVENT_JSON`.
- Link-handler context: `HERDR_PLUGIN_CLICKED_URL`,
  `HERDR_PLUGIN_LINK_HANDLER_ID`.

Evidence: path directories are built at
`herdr-upstream/src/app/api/plugins/env.rs:15-29`; the remaining command
environment is assembled at
`herdr-upstream/src/app/api/plugins/runtime.rs:32-81`.

Plugin pane commands receive `HERDR_SOCKET_PATH`, `HERDR_ENV=1`,
`HERDR_PLUGIN_ID`, `HERDR_PLUGIN_ROOT`, `HERDR_PLUGIN_CONFIG_DIR`,
`HERDR_PLUGIN_STATE_DIR`, `HERDR_PLUGIN_ENTRYPOINT_ID`,
`HERDR_PLUGIN_CONTEXT_JSON`, and normally `HERDR_BIN_PATH`
(`herdr-upstream/src/app/api/plugins/panes.rs:230-261`). Because they are
managed panes, the generic launch layer also injects `HERDR_WORKSPACE_ID`,
`HERDR_TAB_ID`, and `HERDR_PANE_ID`, and sets `TERM=xterm-256color` plus
`COLORTERM=truecolor` (`herdr-upstream/src/pane.rs:56-63`,
`herdr-upstream/src/pane.rs:111-131`).

`HERDR_PLUGIN_CONTEXT_JSON` itself can carry `workspace_id`,
`workspace_label`, `workspace_cwd`, `worktree`, `tab_id`, `tab_label`,
`focused_pane_id`, `focused_pane_cwd`, `focused_pane_agent`,
`focused_pane_status`, `selected_text`, `invocation_source`, `correlation_id`,
`clicked_url`, and `link_handler_id`
(`herdr-upstream/src/api/schema/plugins.rs:354-386`).

No source injects `HERDR_CELL_WIDTH_PX` or `HERDR_CELL_HEIGHT_PX`; the plugin
pane test deliberately prints them with an `unset` fallback
(`herdr-upstream/src/app/api/plugins/mod.rs:1320-1320`). `HERDR_SESSION` can be
present by ordinary parent-environment inheritance for a named session, but it
is session selection state rather than a per-pane/plugin injection
(`herdr-upstream/src/session.rs:80-101`).

### 3. CLI command tree

Global flags/options: `-h|--help`, `--no-session`, `--session NAME`,
`--remote TARGET`, `--remote-keybindings local|server`, `--handoff`,
`--default-config`, and `-V|--version`
(`herdr-upstream/src/cli/spec.rs:3-25`).

Complete public tree (arguments abbreviated but behavior-bearing flags retained):

```text
herdr
├── completion|completions SHELL
├── update [--handoff]
├── status [--json]
│   ├── server [--json]
│   └── client [--json]
├── config
│   ├── check
│   └── reset-keys
├── channel
│   ├── show
│   └── set stable|preview
├── server
│   ├── stop
│   ├── reload-config
│   ├── agent-manifests [--json]
│   ├── update-agent-manifests [--json]
│   └── reload-agent-manifests
├── api
│   ├── snapshot
│   └── schema [--json] [--output PATH]
├── workspace
│   ├── list
│   ├── create [--cwd] [--label] [--env] [--focus|--no-focus]
│   ├── get ID
│   ├── focus ID
│   ├── rename ID LABEL...
│   ├── report-metadata ID [--source] [--token] [--clear-token] [--seq] [--ttl-ms]
│   └── close ID
├── worktree
│   ├── list [--workspace] [--cwd] [--json]
│   ├── create [--workspace] [--cwd] [--branch] [--base] [--path] [--label] [--focus|--no-focus] [--json]
│   ├── open [--workspace] [--cwd] [--path] [--branch] [--label] [--focus|--no-focus] [--json]
│   └── remove [--workspace] [--force] [--json]
├── tab
│   ├── list [--workspace]
│   ├── create [--workspace] [--cwd] [--label] [--env] [--focus|--no-focus]
│   ├── get ID
│   ├── focus ID
│   ├── rename ID LABEL...
│   └── close ID
├── notification
│   └── show TITLE [--body] [--position] [--sound none|done|request]
├── agent
│   ├── list
│   ├── get TARGET
│   ├── read TARGET [--source] [--lines] [--format] [--ansi]
│   ├── send TARGET TEXT
│   ├── rename TARGET [NAME|--clear]
│   ├── focus TARGET
│   ├── wait TARGET --status idle|working|blocked|unknown [--timeout]
│   ├── attach TARGET [--takeover]
│   ├── start NAME [--cwd] [--workspace] [--tab] [--split] [--env] [--focus|--no-focus] -- ARGV...
│   └── explain [TARGET|--file PATH --agent LABEL] [--json] [--format] [-v|--verbose]
├── pane
│   ├── list [--workspace]
│   ├── current|get|layout|process-info|neighbor|edges|focus|resize|zoom
│   ├── read|rename|split|swap|move|close
│   ├── send-text|send-keys|run
│   ├── report-agent
│   ├── report-agent-session
│   ├── release-agent
│   └── report-metadata
├── wait
│   ├── output ID [--match] [--source] [--lines] [--timeout] [--regex] [--raw]
│   └── agent-status ID --status idle|working|blocked|done|unknown [--timeout]
├── terminal
│   ├── attach TERMINAL_ID [--takeover]
│   ├── session observe TARGET [--cols] [--rows]
│   └── title set TITLE|clear
├── session
│   ├── list [--json]
│   ├── attach NAME
│   ├── stop NAME [--json]
│   └── delete NAME [--json]
├── integration
│   ├── install TARGET
│   ├── uninstall TARGET
│   └── status [--outdated-only]
└── plugin
    ├── install OWNER/REPO[/SUBDIR] [--ref] [-y|--yes]
    ├── uninstall PLUGIN
    ├── link PATH [--disabled|--enabled]
    ├── unlink PLUGIN_ID
    ├── enable PLUGIN_ID
    ├── disable PLUGIN_ID
    ├── list [--plugin] [--json]
    ├── config-dir PLUGIN_ID
    ├── action list [--plugin]
    ├── action invoke ACTION_ID [--plugin]
    ├── log|logs list [--plugin] [--limit]
    ├── pane open [--plugin] [--entrypoint] [--placement] [--workspace]
    │              [--target-pane] [--direction] [--cwd] [--env]
    │              [--focus|--no-focus]
    ├── pane focus PANE_ID
    └── pane close PANE_ID
```

Evidence by range: top-level and non-multiplexer verbs
`herdr-upstream/src/cli/spec.rs:26-140`; workspace/worktree/tab/notification
`herdr-upstream/src/cli/spec.rs:142-265`; agent
`herdr-upstream/src/cli/spec.rs:267-335`; pane including lifecycle/metadata
verbs `herdr-upstream/src/cli/spec.rs:337-523`; wait/terminal/session/integration
`herdr-upstream/src/cli/spec.rs:525-621`; plugin
`herdr-upstream/src/cli/spec.rs:623-733`.

### 4. IPC sockets and protocol 16

Herdr runs two local IPC surfaces, both owner-only (`0600`) sockets on Unix and
platform-native local sockets/named pipes on Windows.

1. **Public JSON API socket** — normally `herdr.sock`, overridden by
   `HERDR_SOCKET_PATH`. It is newline-delimited JSON. Each request has `id` plus
   serde's `{ "method": ..., "params": ... }`; ordinary calls return one
   success (`id` + tagged `result`) or error (`id` + `{code,message}`) line.
   `events.subscribe` first returns an acknowledgment and then pushes event
   lines. Source: `herdr-upstream/src/api/mod.rs:20-20`,
   `herdr-upstream/src/api/schema.rs:33-45`,
   `herdr-upstream/src/api/schema/response.rs:24-44`,
   `herdr-upstream/src/api/client.rs:93-107`,
   `herdr-upstream/src/api/client.rs:207-222`,
   `herdr-upstream/src/api/server.rs:139-174`.
2. **Private client/render socket** — normally `herdr-client.sock`; an API
   socket override derives the `-client.sock` path. This is TUI/direct-terminal
   IPC. Frames are `[u32 little-endian length][bincode payload]`. A client starts
   with `ClientMessage::Hello { version, dimensions, cell pixel size,
   requested_encoding, keybindings, launch_mode }`; the server answers
   `ServerMessage::Welcome { version, encoding, error }`. It then transports
   input, resize, attach/observe/control requests, semantic/ANSI render frames,
   graphics, notifications, clipboard, title and shutdown messages. Source:
   `herdr-upstream/src/server/socket_paths.rs:14-47`,
   `herdr-upstream/src/protocol/wire.rs:306-398`,
   `herdr-upstream/src/protocol/wire.rs:597-667`,
   `herdr-upstream/src/protocol/wire.rs:811-878`.

`PROTOCOL_VERSION` is **16** in the current upstream source, and exact equality
is required; older and newer clients are rejected
(`herdr-upstream/src/protocol/wire.rs:15-25`,
`herdr-upstream/src/protocol/wire.rs:906-932`). The public `ping` response
reports this same constant (`herdr-upstream/src/api/server.rs:307-325`). Our
checked-in schema snapshot independently has `"protocol": 16`
(`herdr-agent-team/docs/herdr-api-schema.snapshot.json:1-4`).

## Part B — baseline claim verification

Verdicts mean: **CONFIRMED** = the claim is represented by current upstream
source; **DIVERGES** = current source contradicts or materially narrows it;
**NOT-FOUND** = the behavior/location is not implemented or knowable in this
checkout.

### 1. Plugin event dot form and underscore JSON payload

**Verdict: CONFIRMED, with optional-field caveat.**

- `EventKind` serializes with snake_case, so the envelope's `event` is
  `pane_agent_status_changed`; `EventData` is tagged as `type` with the same
  snake_case spelling (`herdr-upstream/src/api/schema/events.rs:190-218`,
  `herdr-upstream/src/api/schema/events.rs:414-416`).
- The manifest/runtime dot name is explicitly
  `pane.agent_status_changed`; the runtime passes that to
  `HERDR_PLUGIN_EVENT` and serializes the whole envelope into
  `HERDR_PLUGIN_EVENT_JSON`
  (`herdr-upstream/src/api/schema/events.rs:220-250`,
  `herdr-upstream/src/app/api/plugins/runtime.rs:55-63`,
  `herdr-upstream/src/app/api/plugins/runtime.rs:183-224`).
- The `PaneAgentStatusChanged` data variant contains `pane_id`,
  `workspace_id`, `agent_status`, and optional `agent`, plus optional/empty-
  skipped `title`, `display_agent`, and `state_labels`
  (`herdr-upstream/src/api/schema/events.rs:524-535`). Therefore the verified
  fixture shape `{type,pane_id,workspace_id,agent_status,agent}` is valid, but
  `agent` may be omitted when `None`, and current source may add the three
  presentation fields. The current source does **not** contain the spec's
  mentioned `custom_status` field.

Baseline evidence: `herdr-agent-team/CLAUDE.md:38-44` and
`herdr-agent-team/docs/spec.md:185-200`.

### 2. Status enum is exactly idle/working/blocked/done/unknown

**Verdict: CONFIRMED.**

`AgentStatus` has exactly five variants, serialized snake_case:
`Idle`, `Working`, `Blocked`, `Done`, and `Unknown`
(`herdr-upstream/src/api/schema/common.rs:142-150`). `Done` is an API/UI
attention state derived from internal `Idle` when the pane is unseen; the
detector itself has only idle/working/blocked/unknown
(`herdr-upstream/src/app/api_helpers.rs:62-72`,
`herdr-upstream/src/detect/mod.rs:9-20`). This distinction explains why
`agent wait` rejects `done` while `wait agent-status` accepts it
(`herdr-upstream/src/cli/agent.rs:643-665`,
`herdr-upstream/src/cli/spec.rs:765-780`).

### 3. `pane run`, `agent send`, and paste debounce

**Compound verdict: DIVERGES (two CONFIRMED subclaims; debounce NOT-FOUND).**

- **CONFIRMED — `pane run` submits in one API call.** The CLI constructs one
  `PaneSendInput` request containing the joined text and `keys: ["Enter"]`
  (`herdr-upstream/src/cli/pane.rs:929-942`). The server encodes/writes the text
  and then the key within that one request
  (`herdr-upstream/src/app/api/panes.rs:1491-1518`).
- **CONFIRMED — `agent send` writes without submitting.** Its request contains
  only `target` and `text`; the server writes exactly those text bytes with no
  Enter (`herdr-upstream/src/cli/agent.rs:565-578`,
  `herdr-upstream/src/app/api/agents.rs:182-195`). Upstream help says this
  explicitly (`herdr-upstream/src/cli/agent.rs:668-684`).
- **NOT-FOUND — “paste-debounce swallows immediate Enter.”** Herdr has no
  debounce timer or double-Enter workaround in this path. When bracketed paste
  is active it wraps text as `ESC[200~...ESC[201~`, then immediately encodes
  and writes Enter (`herdr-upstream/src/app/api_helpers.rs:25-48`,
  `herdr-upstream/src/app/api/panes.rs:1502-1516`). Swallowing that Enter is
  downstream agent-TUI behavior established by the plugin's live test, not an
  upstream Herdr source claim.

Baseline evidence: `herdr-agent-team/CLAUDE.md:45-48` and
`herdr-agent-team/docs/spec.md:207-213`.

### 4. Where mid-turn `pane run` queueing lives

**Verdict: NOT-FOUND in Herdr; it lives downstream in the agent TUI.**

Herdr does not inspect agent status or maintain a logical user-message queue in
the `pane run` path. It immediately converts the request to bracketed-paste text
and Enter and sends those byte chunks to the PTY
(`herdr-upstream/src/cli/pane.rs:929-942`,
`herdr-upstream/src/app/api/panes.rs:1491-1518`). The PTY actor has only a
generic FIFO of pending byte writes (`pending_writes: VecDeque<Bytes>`), not a
turn-aware message queue (`herdr-upstream/src/pty/actor/unix.rs:396-415`). Thus
Claude Code/Codex deciding to display “queued message” and submit it after the
active turn is behavior inside those TUIs. Their source is not in this repo, so
the exact downstream implementation location cannot be cited here.

This does not invalidate the plugin's live verification; it locates its
authority correctly. Baseline evidence:
`herdr-agent-team/CLAUDE.md:49-54` and
`herdr-agent-team/docs/spec.md:201-206`,
`herdr-agent-team/docs/spec.md:214-221`.

### 5. Pane/plugin environment injection and missed variables

**Verdict: CONFIRMED, with additional injected variables.**

All managed panes explicitly receive `HERDR_ENV=1`, `HERDR_SOCKET_PATH`,
`HERDR_WORKSPACE_ID`, `HERDR_TAB_ID`, and `HERDR_PANE_ID`
(`herdr-upstream/src/pane.rs:111-131`,
`herdr-upstream/src/integration/env.rs:8-22`). Therefore the baseline's four
identity/context variables are confirmed, and it misses `HERDR_SOCKET_PATH`.
Herdr also sets `TERM=xterm-256color` and `COLORTERM=truecolor`
(`herdr-upstream/src/pane.rs:52-63`).

For this plugin's event-hook process, the fuller additional set is:
`HERDR_PLUGIN_ID`, `HERDR_PLUGIN_ROOT`, `HERDR_PLUGIN_CONFIG_DIR`,
`HERDR_PLUGIN_STATE_DIR`, `HERDR_PLUGIN_CONTEXT_JSON`, normally
`HERDR_BIN_PATH`, and event-specific `HERDR_PLUGIN_EVENT` plus
`HERDR_PLUGIN_EVENT_JSON`; available workspace/tab/pane IDs are derived from
the event target context (`herdr-upstream/src/app/api/plugins/env.rs:15-29`,
`herdr-upstream/src/app/api/plugins/runtime.rs:39-81`,
`herdr-upstream/src/app/api/plugins/context.rs:136-181`). Action, pane, and
link-handler-only additions are catalogued in Part A.

READY FOR REVIEW
