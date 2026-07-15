# `herdr-claude-teams` dedicated review

Date: 2026-07-15  
Target: `david-lutz/herdr-claude-teams` at `2c141f039434dad05780bd7aae2f25b805f5bfc7`  
Local comparison: `herdr-agent-team` at `10a855ae95ee8ca6cb22b7fb89e1eda748a01987` plus the present uncommitted working tree  
Verdict: **pattern-source, with a narrow Claude-only overlap; not a strategic threat**

## Scope and evidence

I shallow-cloned the target to `/tmp/herdr-claude-teams` and read all ten runtime
package modules, the trace helper, all twelve test/support modules, the launcher
script, README, package metadata, current OpenSpec specifications, and archived
design/change records. I also inspected the target's complete public commit list
and current repository metadata through the GitHub API. The local comparison is
against the Rust implementation, not only its README/spec.

Validation on this Linux host:

- Target: `uv run --quiet pytest` — **106 passed, 1 skipped** in 0.47 s. The
  skipped test is the opt-in live-Herdr smoke test, so this does not establish
  live compatibility with our current Herdr.
- Local: `cargo test --all-targets` — **98 passed, 0 failed**.

Citation convention: `target:` paths refer to the pinned target commit; `local:`
paths refer to this repository. Citations are `file:start-end` and, for target
files, link to the pinned GitHub blob.

## Executive assessment

`herdr-claude-teams` does one clever, narrow thing: it makes Claude Code believe
it is inside tmux, then translates the tmux subprocess calls made by Claude's
native agent-team backend into Herdr Unix-socket operations. Claude still owns
team identity, tasks, teammate prompts, and `SendMessage`; Herdr supplies visible
panes and agent chrome. The adapter itself is not a general orchestrator
([`target: README.md:3-10`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/README.md#L3-L10),
[`target: launcher.py:59-87`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/launcher.py#L59-L87)).

That gives it one material advantage over us: a Claude-only team uses Claude's
native mailbox and task model while still appearing in visible Herdr panes. This
is an architectural inference from preserving Claude's real `--agent-id`
teammate processes, not a mailbox test in the target repository. Our plugin
approximates mailbox delivery well, but it deliberately owns a separate,
heterogeneous protocol and still lacks the native shared task board and typed
control messages (`local: docs/research/native-teammate-parity-2026-07-15.md:12-25`).

Our plugin is much broader and operationally more serious: heterogeneous agents,
data-driven launchers, isolated worktrees and setup, durable runs/reports,
star/mesh routing, status hooks, verified delivery/outboxes, adoption of existing
panes, and safe teardown. Those capabilities are outside the target's model
(`local: src/types.rs:7-79`, `local: src/spawn.rs:416-473`,
`local: src/adopt.rs:275-352`, `local: src/msg.rs:237-377`).

The strategic value is therefore the **raw socket client and fake-socket test
pattern**, not its tmux emulation. We should add a protocol-16 socket transport
behind our existing Herdr abstraction, using the current schema and stable public
IDs. We should not copy the target's 0.6.10 ID grammar, incomplete tmux parser,
or experimental-Claude coupling.

## 1. Architecture and exact tmux mapping

### 1.1 Bootstrap and process topology

Inside Herdr, the launcher:

1. Requires `HERDR_ENV=1` and a non-empty `HERDR_SOCKET_PATH`; otherwise it
   transparently `exec`s ordinary `claude`
   ([`target: launcher.py:45-46,69-81`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/launcher.py#L45-L81)).
2. Calls `pane.get` with `HERDR_PANE_ID` and reads the returned pane's stable
   `terminal_id`
   ([`target: launcher.py:49-56`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/launcher.py#L49-L56)).
3. Creates a private executable named `tmux` whose entire body execs the current
   Python interpreter with `-m herdr_claude_teams.shim`
   ([`target: launcher.py:21-36`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/launcher.py#L21-L36)).
4. Exports `TMUX_PANE=%<terminal_id>`, a fake
   `TMUX=<HERDR_SOCKET_PATH>,<pid>,herdr`,
   `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`, and prepends the shim directory to
   `PATH`; it then `exec`s Claude with all original arguments
   ([`target: launcher.py:59-65,83-90`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/launcher.py#L59-L90)).

Claude Code 2.1.174 was observed issuing this launch sequence:
`display-message`, `split-window`, literal `send-keys` of an `env ... claude`
command, `send-keys Enter`, repeated `capture-pane`, and cosmetic
layout/resize/title calls
([`target: archived design:3-12`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/openspec/changes/archive/2026-06-12-herdr-teams/design.md#L3-L12)).
The new pane starts as a fresh login shell with Herdr's `HERDR_*` variables; the
command Claude types supplies the teammate's own Claude environment
([`target: archived design:14-21`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/openspec/changes/archive/2026-06-12-herdr-teams/design.md#L14-L21)).

### 1.2 CLI parsing and tmux-shaped behavior

The shim strips tmux global options `-L`, `-S`, `-f`, and `-T`, reports the
fictional version `tmux 3.5a`, dispatches the first remaining token, and maps only
`HerdrError` and unsupported verbs to exit 1
([`target: shim.py:15-54`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/shim.py#L15-L54)).

Argument parsing is per verb because the same flag changes meaning: for example,
`-l` consumes a size for `split-window` but is a boolean literal-mode switch for
`send-keys`. It supports clustered short flags and `--`; unknown short/long flags
are accepted as booleans rather than rejected
([`target: argparse_tmux.py:33-69,96-141`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/argparse_tmux.py#L33-L141)).
It recognizes aliases including `splitw`, `send`, `capturep`, `displayp`, `lsp`,
`lsw`, `selectp`, `selectw`, `killp`, `killw`, `resizep`, `renamew`, `neww`,
`new`, and `has`
([`target: argparse_tmux.py:13-31`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/argparse_tmux.py#L13-L31)).

The format renderer is deliberately tiny: every `#{field}` is replaced by a
string from a supplied context and unknown fields become empty. It has no tmux
conditionals, modifiers, escaping, or nested expressions
([`target: format.py:1-16`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/format.py#L1-L16)).

### 1.3 Complete implemented verb map

| tmux verb | Herdr socket calls | Translation and returned tmux shape | Fidelity / issue |
|---|---|---|---|
| `split-window` / `splitw` | optional `pane.list`, then `pane.split` | `-h` -> `direction:"right"`; otherwise `"down"`; always `focus:false`; `-t` becomes `target_pane_id`; `-l`/`-p` become float `ratio`; `-P` renders `-F` (default `#{pane_id}`) from returned pane | Good for observed Claude call. Missing/invalid ratios are silently ignored. [`target: verbs.py:91-132`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L91-L132) |
| `send-keys` / `send` | `pane.list`, then `pane.send_text` | `-l` concatenates positional arguments and sends literal text without Enter; other tokens are translated to bytes and concatenated. Recognized names cover Enter/C-m/KPEnter, Tab/C-i, Space, backspace, Escape, C-c/d/z/l, plus general `C-a..C-z` | Uses `pane.send_text` even for control bytes. The teammate-launch rewriter only recognizes exact `cd ... && env ...` shape and injects `TMUX_PANE=%<terminal_id>`. [`target: verbs.py:107-145`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L107-L145), [`target: keys.py:10-36`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/keys.py#L10-L36) |
| `capture-pane` / `capturep` | `pane.list`, then `pane.read` | `-S` selects `source:"recent"`; otherwise `"visible"`; always `format:"text"`, `strip_ansi:true`; prints `result.read.text` with no added newline | Ignores `-E`, numeric ranges, line count, `-J`, and most capture flags. The observed Claude call used only `-p -t`. [`target: verbs.py:148-154`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L148-L154), [`target: archived design:132-139`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/openspec/changes/archive/2026-06-12-herdr-teams/design.md#L132-L139) |
| `display-message` / `display` / `displayp` | for targeted `-p`: `pane.list`, then `pane.get` | Without `-p`, success/no output. With `-p`, renders `-F` or final positional format. Target context exposes pane/window/session fields | With no target the context is empty, so all fields render empty. [`target: verbs.py:157-168`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L157-L168) |
| `list-panes` / `lsp` | `pane.list` | Filters by exact `tab_id` or `workspace_id`; renders `-F` (default `#{pane_id}`) once per pane | Ignores `-a`, `-s`, and `-f` semantics. [`target: verbs.py:171-179`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L171-L179) |
| `list-windows` / `lsw` | `tab.list` | Derives optional workspace from `-t`; renders `window_id`, `window_index`, and `session_name` | Minimal fields and filtering only. [`target: verbs.py:182-194`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L182-L194) |
| `select-pane` / `selectp` | targeted: `pane.list`, `pane.get`, then `tab.focus` | `-P`/`-T` are cosmetic no-ops; otherwise focuses the pane's tab | Cannot focus an absolute sibling pane because the old API exposed no such operation. [`target: verbs.py:197-204`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L197-L204) |
| `select-window` / `selectw` | `tab.focus` | Strips `@` and focuses exact tab id | Relative selectors are not implemented. [`target: verbs.py:207-211`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L207-L211) |
| `kill-pane` / `killp` | `pane.list`, then optional `pane.close` | Resolves stable terminal id to current pane id; a missing target is idempotent success | `-a` is parsed but ignored. [`target: verbs.py:214-219`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L214-L219) |
| `kill-window` / `killw` | `tab.close` | Strips `@` and closes the tab | `-a` ignored. [`target: verbs.py:222-224`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L222-L224) |
| `resize-pane` / `resizep` | directional: `pane.list`, then `pane.resize` | `-L/-R/-U/-D` map to left/right/up/down; first positional parses as float `amount` | Absolute `-x/-y` is a success/no-op even though Claude's observed flow sends `resize-pane -x 30%`. [`target: verbs.py:227-238`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L227-L238) |
| `rename-window` / `renamew` | `tab.rename` | Strips target sigil; joins positional text with spaces into `label` | Minimal. [`target: verbs.py:241-244`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L241-L244) |
| `new-window` / `neww` | `tab.create` | `focus:false`; optional workspace and label; `-P` prints a `%`-prefixed root **pane_id** | Latent bug: every other Claude-facing pane id is a stable `terminal_id`, so the printed id will not resolve later. It also ignores requested `-F`. [`target: verbs.py:247-258`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L247-L258) |
| `new-session` / `new` | `workspace.create` | `focus:false`; `-s` or `-n` becomes label; `-P` prints a `%`-prefixed root **pane_id** | Same stable-id bug; ignores `-F`, cwd/env/size/attach flags. [`target: verbs.py:261-268`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L261-L268) |
| `has-session` / `has` | `workspace.list` | Strips `$`, derives workspace id, returns 1 if no exact id match, otherwise 0 | Checks Herdr id, not tmux session name/label/pattern semantics. [`target: verbs.py:271-277`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L271-L277) |
| option/layout/client verbs | none | `select-layout`, set/show option variants, `source-file`, `refresh-client`, `attach-session`, and `detach-client` return 0 | Explicitly hides unsupported cosmetic/option behavior. [`target: verbs.py:18-35,299-306`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L18-L35) |

### 1.4 Identity mapping

The Claude-facing pane identity is `%<terminal_id>`, not Herdr's positional
`pane_id`. Window and session IDs are `@<tab_id>` and `$<workspace_id>`.
Sigil handling itself is pure string prefix/strip logic
([`target: ids.py:1-48`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/ids.py#L1-L48)).

Before every targeted pane operation, the shim calls `pane.list`, scans for a
matching `terminal_id`, and uses that record's current `pane_id`. This was added
because Herdr 0.6.10 renumbered positional pane IDs after a sibling closed, while
Claude cached tmux pane IDs and killed them sequentially. A missing target is
benign only for `kill-pane`; read/send/resize/select fail with
`pane_not_found`
([`target: verbs.py:44-73`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L44-L73),
[`target: stable-id design:10-24`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/openspec/changes/archive/2026-06-12-stable-pane-ids/design.md#L10-L24)).

The rendered context is:

- `pane_id = %<terminal_id>`
- `pane_index = final '-' component of pane_id`
- `window_id = @<tab_id>`
- `window_index = final ':' component of tab_id`
- `window_name = workspace_id`
- `session_name = workspace_id`
- `session_id = $<workspace_id>`

([`target: verbs.py:76-88`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L76-L88)).
This exposes a current-version incompatibility: our protocol-16 sample uses pane
IDs like `wG:p6`, so `rsplit("-", 1)` returns `wG:p6`, not pane index `6`
(`local: src/herdr.rs:550-551`). The target's live-only assertion likewise
expects `w<workspace>-N` and was skipped in our run
([`target: test_live_smoke.py:30-53`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/tests/test_live_smoke.py#L30-L53)).

## 2. Herdr socket protocol dossier

This is the reusable intelligence in the project. Unless stated otherwise, the
contract was recorded against Herdr 0.6.10, not our current 0.7.x protocol 16
([`target: archived design:103-114`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/openspec/changes/archive/2026-06-12-herdr-teams/design.md#L103-L114)).

### 2.1 Transport and framing

- Address: path passed explicitly or read from `HERDR_SOCKET_PATH`; absence raises
  `RuntimeError` before connection.
- Transport: Unix-domain `AF_UNIX`, `SOCK_STREAM`.
- Connection model: one new socket per RPC; no pooling or shared state.
- Timeout: 5 seconds applied to the socket.
- Request framing: UTF-8 JSON followed by exactly one newline.
- Response framing: read chunks of up to 65,536 bytes until the accumulated
  buffer ends in newline or the server closes the stream, then close the socket.
- Request envelope:
  `{"id":"herdr-claude-teams:<process-local-counter>","method":"<method>","params":{...}}`.
  There is no `jsonrpc:"2.0"` member. A falsey/missing params object becomes `{}`.
- Success envelope: `{"id":...,"result":{...}}`; the client returns `result`,
  defaulting to `{}` if omitted.
- Error envelope:
  `{"id":...,"error":{"code":"<code>","message":"<message>"}}`; this becomes
  `HerdrError(code, message)`.
- Decoding: UTF-8 followed by `json.loads(..., strict=False)`.
- The implementation does **not** compare response `id` with request `id`, check
  the result `type`, enforce a maximum response size, retry, or handle more than
  one response frame.

All of those implementation details are visible in
[`target: socket_client.py:18-68`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/socket_client.py#L18-L68).
The fake server confirms newline-delimited requests and flushes a same-id success
or error response per line
([`target: tests/_fake_herdr.py:182-205`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/tests/_fake_herdr.py#L182-L205)).

### 2.2 Production-emitted methods and shapes

| Method | Params emitted or accepted | Result fields consumed / recorded |
|---|---|---|
| `pane.get` | `{pane_id}` | `{type:"pane_info", pane:{pane_id, terminal_id, tab_id, workspace_id, ...}}`; used both to resolve the leader and render a targeted pane. [`target: launcher.py:49-56`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/launcher.py#L49-L56) |
| `pane.list` | `{}` | `{type:"pane_list", panes:[{pane_id, terminal_id, tab_id, workspace_id, focused, ...}]}`. It is both enumeration and the stable-ID bridge. [`target: verbs.py:44-61`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L44-L61) |
| `pane.split` | `{target_pane_id?, direction:"right"|"down", ratio?:float, focus:false}` | `{type:"pane_info", pane:{...}}`. Critical detail: old Herdr required `target_pane_id`; a `pane_id` field was silently ignored and fell back to focused pane. [`target: archived design:107-109`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/openspec/changes/archive/2026-06-12-herdr-teams/design.md#L107-L109) |
| `pane.send_text` | `{pane_id, text}` | `{type:"ok"}`. Used for literal commands and translated control bytes. [`target: verbs.py:135-145`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L135-L145) |
| `pane.read` | `{pane_id, source, format:"text", strip_ansi:true}` | `{type:"pane_read", read:{pane_id?, text, source?, truncated, revision?, ...}}`. Recorded source vocabulary is `visible`, `recent`, `recent_unwrapped`, and `detection`; format is `text` or `ansi`; optional `lines`. Production only emits visible/recent text. [`target: archived design:109-111`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/openspec/changes/archive/2026-06-12-herdr-teams/design.md#L109-L111) |
| `pane.close` | `{pane_id}` | Success shape treated as irrelevant; fake returns `{type:"ok"}`. [`target: verbs.py:214-219`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L214-L219) |
| `pane.resize` | `{pane_id, direction, amount?:float}` | `{type:"pane_resize", resize:{...}}`. [`target: verbs.py:227-237`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L227-L237) |
| `tab.focus` | `{tab_id}` | Result ignored; fake returns `{type:"ok"}`. [`target: verbs.py:197-211`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L197-L211) |
| `tab.list` | `{workspace_id}` or `{}` | `{type:"tab_list", tabs:[{tab_id, workspace_id, ...}]}`. [`target: verbs.py:182-194`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L182-L194) |
| `tab.close` | `{tab_id}` | Result ignored. [`target: verbs.py:222-224`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L222-L224) |
| `tab.rename` | `{tab_id, label}` | Result ignored. [`target: verbs.py:241-244`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L241-L244) |
| `tab.create` | `{focus:false, workspace_id?, label?}` | `{type:"tab_created", tab, root_pane}`; handler consumes `root_pane`. [`target: verbs.py:247-258`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L247-L258) |
| `workspace.create` | `{focus:false, label?}` | `{type:"workspace_created", workspace, tab, root_pane}`. [`target: verbs.py:261-268`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L261-L268) |
| `workspace.list` | `{}` | `{type:"workspace_list", workspaces:[{workspace_id, label, focused, ...}]}`. [`target: verbs.py:271-277`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/verbs.py#L271-L277) |

### 2.3 Additional protocol facts revealed outside production handlers

- `ping {}` returned
  `{type:"pong", version:"0.6.10", protocol:13}` in the fake that mirrors the
  validated environment
  ([`target: tests/_fake_herdr.py:101-105`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/tests/_fake_herdr.py#L101-L105)).
  Production never calls `ping`, so it performs no compatibility handshake.
- The fake models `pane.send_keys {pane_id, ...}` -> `{type:"ok"}`, but the
  production shim never emits that method. The archived live contract is more
  specific: params are `{pane_id, keys:[...]}`
  ([`target: tests/_fake_herdr.py:126-135`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/tests/_fake_herdr.py#L126-L135),
  [`target: openspec/changes/archive/2026-06-12-herdr-teams/design.md:109-110`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/openspec/changes/archive/2026-06-12-herdr-teams/design.md#L109-L110)).
- The recorded old `pane.resize` contract allowed
  `{pane_id?, direction, amount?}` and returned
  `{type:"pane_resize", resize:{...}}`; production always supplies `pane_id` for
  directional resize
  ([`target: openspec/changes/archive/2026-06-12-herdr-teams/design.md:111-113`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/openspec/changes/archive/2026-06-12-herdr-teams/design.md#L111-L113)).
- The live smoke additionally uses `workspace.close {workspace_id}` and
  `workspace.focus {workspace_id}`; it expects `workspace.create` to return
  `workspace.workspace_id` and `root_pane.pane_id`
  ([`target: test_live_smoke.py:34-55`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/tests/test_live_smoke.py#L34-L55)).
- Old Herdr accepted the environment's `p_N` pane alias as a target for
  `pane.split`, `pane.send_text`, and `pane.read`; bogus targets returned error
  code `pane_not_found`. `pane.get(p_N)` returned the canonical positional
  `pane_id`, `tab_id`, and `workspace_id`
  ([`target: archived design:77-80`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/openspec/changes/archive/2026-06-12-herdr-teams/design.md#L77-L80)).
- There was no absolute `pane.focus` by ID in 0.6.10. Recorded focus operations
  were `pane.focus_direction`, `tab.focus`, and `workspace.focus`
  ([`target: archived design:111-114`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/openspec/changes/archive/2026-06-12-herdr-teams/design.md#L111-L114)).
- The project observed `terminal_id` as stable across positional renumbering, but
  noted that whether terminal IDs are never reused remained an open question
  ([`target: stable-id design:130-140`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/openspec/changes/archive/2026-06-12-stable-pane-ids/design.md#L130-L140)).
- Trace output is append-only JSONL. Records carry epoch time `t`, an `event`, and
  fields such as raw argv, canonical verb/args, method/params, result type, error
  code, or exit code. A configured path of `1` maps to the temp directory. Trace
  write failures are swallowed
  ([`target: trace.py:16-34`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/trace.py#L16-L34),
  [`target: socket_client.py:45-68`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/socket_client.py#L45-L68)).

## 3. Capability comparison

### 3.1 What it gets that our plugin does not

| Capability | Target advantage | Qualification |
|---|---|---|
| Native Claude `SendMessage` and mailbox semantics | **Yes.** The shim only changes Claude's pane backend; it does not replace Claude's team runtime. Native leader/teammate message queuing, teammate-to-leader and peer messaging therefore remain Claude-owned while panes are visible. The target itself contains no mailbox implementation. | This is Claude-only and coupled to an experimental flag and observed private tmux contract. The evidence that the target delegates team logic is its environment-only launcher and tmux-only verb layer, not a target mailbox module (`target: launcher.py:59-87`, `target: verbs.py:280-306`). |
| Native shared task board | **Yes.** Because these are native Claude teammates, they retain Claude's TaskCreate/List/Update/dependency/claiming model. Our `run.toml` tracks worker lifecycle, not shared claimable tasks. | Explicitly still roadmap for us (`local: docs/spec.md:159-174`; `local: docs/research/native-teammate-parity-2026-07-15.md:19-21`). |
| Typed native control messages | **Yes.** Native plan-approval and shutdown request/response semantics remain available. | Our mesh envelope is opaque text and has no typed control-state machine (`local: src/agents_md.rs:128-150`). |
| Native teammate resume semantics | Potentially yes through Claude's own team runtime. | Our plugin records native agent session IDs but restart/resume is roadmap (`local: docs/spec.md:161-165`). |
| Claude's own team UX and mental model | **Yes.** Users prompt Claude's normal team feature; Claude chooses, spawns, names, and coordinates teammates. | Less deterministic and less cross-agent controllable than a checked-in `herdr-team.toml`. |
| Multiple Claude teammates as sibling splits in the leader's tab | **Yes.** `pane.split` targets the leader and keeps focus false. | Our v1 allocates one Herdr workspace per worker, making workers visible but spatially separate (`local: src/spawn.rs:443-471`, `local: src/spawn.rs:539-560`). |

The key nuance is that “native mailbox semantics” are **not implemented by this
repository**. Claude Code implements them; this project preserves them by
intercepting only tmux. If Claude changes team internals or stops shelling out to
tmux, the advantage disappears until the adapter is updated.

### 3.2 What ours gets that it cannot

| Capability | Our implementation evidence | Why target cannot provide it |
|---|---|---|
| Heterogeneous agents | Shipped Claude and Codex launcher entries, plus config-extensible launchers (`local: src/launcher.rs:26-83`; `local: src/types.rs:58-79`). | Target always execs `claude`; Claude's native team owns teammate process construction. |
| Codex workers | Codex is first-class, launch/submission tested, with per-agent mid-turn policy (`local: src/launcher.rs:37-44`; `local: src/spawn.rs:621-669`). | Not part of Claude native agent teams. |
| Declarative roster and roles | `TeamSpec` includes name, topology, cwd, setup, god, and typed worker records (`local: src/types.rs:7-55`). | Target has no team spec or roster layer. |
| Per-worker worktrees and setup | Worktrees are created before launch, setup runs inside them, and paths are persisted (`local: src/spawn.rs:539-618`). | Target splits the current Herdr tab and accepts the cwd/command Claude supplies; no repo isolation policy. |
| Star/mesh topology under our control | Generated protocols expose only god in star or an explicit peer table/envelope in mesh (`local: src/agents_md.rs:88-150`). | Native Claude owns team topology and protocol. |
| Durable run board, protocols, reports, and event log | Each run persists state; protocols are create-new immutable snapshots; workers write report files and the hook injects only a pointer (`local: src/spawn.rs:435-471,673-700`; `local: src/hook.rs:94-133`). | Target is deliberately stateless across shim calls and adds no durable orchestration state. |
| `msg` verb for any supported agent | Name-to-pane resolution, immediate verified `pane run`, or durable per-target outbox (`local: src/msg.rs:237-377`). | Target relies exclusively on Claude's internal `SendMessage`. |
| Delivery policy and hook-driven outbox drain | Queueability is per launcher; unsafe mid-turn targets enqueue; idle/done hook drains in order and audits success/failure (`local: src/msg.rs:345-430`; `local: src/hook.rs:108-178`). | No non-Claude messaging and no target-owned delivery durability. |
| Push completion/reporting | Herdr status events append durable events and wake the god with an absolute report pointer (`local: src/hook.rs:94-133`). | Native Claude owns notifications; target does not persist completion reports. |
| Team adoption | Existing detected-agent panes can join an active star run or bootstrap an ad-hoc run; they receive protocol/prompt and persist as adopted (`local: src/adopt.rs:235-352`). | Target only wraps the Claude process it launches. |
| Safe ownership-aware teardown | Owned workspaces close; adopted panes survive and receive a release notice; dirty worktrees are preserved (`local: src/status_kill.rs:316-445`). | Target only proxies Claude's `kill-pane`; teammate panes can fall back to live shells. |
| Current Herdr plugin integration | Plugin manifest targets Herdr >=0.7.0 on Linux/macOS and hooks `pane.agent_status_changed` (`local: herdr-plugin.toml:1-6,29-34`). | Target is a standalone Python wrapper, macOS-only by stated support, validated on Herdr 0.6.10. |

### 3.3 Areas of overlap

Both approaches produce visible, inspectable Herdr panes and benefit from Herdr's
agent detection/sidebar/notifications. Both send submitted text through a pane,
but at different layers: the target emulates tmux's `send-keys`; ours calls the
supported `herdr pane run` CLI and verifies the recipient reaches `working`
(`local: src/herdr.rs:203-247`, `local: src/spawn.rs:746-845`). Both learned that
stable identity and teardown matter, but the target solved an old positional-ID
problem with `terminal_id` rescans while our current plugin persists the opaque
public pane IDs returned by protocol 16 (`local: src/types.rs:81-104`,
`local: src/herdr.rs:622-650`).

## 4. Robustness, limitations, and maintenance

### 4.1 What is robust

- **Small, isolated runtime:** Python >=3.11, no runtime dependencies, one private
  cache shim, and no shell/Herdr/Claude config mutation
  ([`target: pyproject.toml:1-16`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/pyproject.toml#L1-L16),
  [`target: README.md:75-83`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/README.md#L75-L83)).
- **Graceful non-Herdr behavior:** outside a Herdr pane it behaves as plain
  `claude`, so the wrapper can be used as an alias without breaking other shells
  ([`target: launcher.py:69-75`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/launcher.py#L69-L75)).
- **Correctly targets the leader:** `pane.get(HERDR_PANE_ID)` avoids a focus
  assumption; the authors specifically observed a valid unfocused leader
  ([`target: archived design:77-80`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/openspec/changes/archive/2026-06-12-herdr-teams/design.md#L77-L80)).
- **Stable teardown identity:** fresh `pane.list` resolution and idempotent
  missing close directly regression-test the sequential-close leak
  ([`target: test_regression.py:50-78`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/tests/test_regression.py#L50-L78)).
- **Useful hermetic test spine:** a threaded fake Unix socket records requests,
  models stable terminal IDs plus renumbering, and tests end-to-end shim output
  ([`target: tests/_fake_herdr.py:19-99`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/tests/_fake_herdr.py#L19-L99),
  [`target: test_regression.py:14-47`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/tests/test_regression.py#L14-L47)).
- **Debuggability:** unknown verbs hard-fail instead of silently corrupting flow,
  and JSONL traces expose raw argv and RPC method/params/results
  ([`target: shim.py:47-54`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/shim.py#L47-L54),
  [`target: trace.py:16-34`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/herdr_claude_teams/trace.py#L16-L34)).

### 4.2 Material limitations and defects

1. **Version drift is already material.** The README pins validation to Herdr
   0.6.10 and Claude Code 2.1.174 and states macOS-only support
   ([`target: README.md:12-16,57-73`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/README.md#L12-L16)).
   Our local live samples are Herdr 0.7.3/protocol 16 and use a different public
   pane-ID grammar (`local: docs/spec.md:185-201`; `local: src/herdr.rs:550-551`).
   The target never calls `ping`, never checks protocol/version, and its only live
   test was skipped here.

2. **It depends on private/experimental Claude behavior.** The native tmux backend
   is interactive-only; headless `claude -p` chooses the in-process backend, and
   forcing tmux hits a TTY dialog. The shim's automated test therefore replays a
   captured sequence instead of exercising current Claude end-to-end
   ([`target: archived design:132-140`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/openspec/changes/archive/2026-06-12-herdr-teams/design.md#L132-L140)).

3. **Socket handling is thin rather than hardened.** It has a timeout and clean
   server-error mapping, but no response-ID validation, size bound, compatibility
   handshake, retry, typed result validation, or graceful handling for connection,
   timeout, UTF-8, empty-frame, or malformed-JSON exceptions. `shim._run` catches
   only `UnsupportedVerb` and `HerdrError`, so those other failures escape as
   Python tracebacks (`target: socket_client.py:37-68`; `target: shim.py:41-54`).

4. **Stable-ID resolution still has a time-of-check/time-of-use race.** A pane can
   close after `pane.list` and before the actual operation. The design acknowledges
   this; only a close whose lookup already missed is converted to success, not a
   `pane.close` that races and returns `pane_not_found`
   ([`target: stable-id design:105-117`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/openspec/changes/archive/2026-06-12-stable-pane-ids/design.md#L105-L117)).

5. **The tmux compatibility surface is intentionally incomplete.** Unsupported
   formats and target grammar, ignored flags, success/no-op resize/layout calls,
   and approximate session names are acceptable only while Claude emits the exact
   observed sequence. Unknown options can be silently accepted by the parser
   (`target: argparse_tmux.py:96-141`; `target: format.py:10-16`).

6. **The launch-command rewrite is brittle.** `TMUX_PANE` injection requires the
   literal payload to start with `cd ` and contain exact ` && env ` spacing. It
   does not parse shell quoting, detect an existing assignment, or inject `TMUX`.
   Nested teams remain untested (`target: verbs.py:107-113`; `target: README.md:57-73`).

7. **Teammate panes do not self-close on agent exit.** Claude types the agent
   command into a live shell instead of making it the pane exec target. If the
   process exits without Claude teardown, the pane returns to a prompt
   ([`target: README.md:66-72`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/README.md#L66-L72)).

8. **Concrete untested lifecycle bugs exist.** `new-window -P` and
   `new-session -P` emit positional root `pane_id`s although later target
   resolution accepts only terminal IDs. The fake server does not implement
   `tab.create`, `workspace.create`, `tab.close`, or `tab.rename`, and the verb
   suite does not exercise those handlers. The green suite therefore strongly
   covers the observed teammate path, not the whole advertised verb table
   (`target: verbs.py:241-277`; `target: tests/_fake_herdr.py:101-173`;
   `target: tests/test_verbs.py:22-141`).

9. **Trace logs may contain sensitive prompts/commands.** RPC tracing records the
   full `params`, including `pane.send_text` payloads. This is opt-in, but there is
   no redaction (`target: socket_client.py:45-46`; `target: trace.py:25-34`).

10. **Shim installation has a small race.** Every in-Herdr launch directly
    rewrites and then chmods the same cache-path `tmux` file without a temporary
    file, atomic rename, or lock (`target: launcher.py:21-36,83-85`).

### 4.3 Maintenance state as of 2026-07-15

GitHub API snapshot:

- repository created 2026-06-14; last push 2026-06-14;
- 22 commits, all by one contributor, concentrated from June 12-14;
- 1 star, 0 forks, 0 open issues, no tags, no releases, one unprotected `main`
  branch;
- not archived, MIT licensed;
- package version remains `0.0.1`
  ([`target: pyproject.toml:1-13`](https://github.com/david-lutz/herdr-claude-teams/blob/2c141f039434dad05780bd7aae2f25b805f5bfc7/pyproject.toml#L1-L13));
- no GitHub Actions workflow or other CI configuration is present in the tree.

This reads as a careful, intensely built **prototype/reference implementation**,
not an actively maintained compatibility product. “Abandoned” would be too
strong after only one month, but there is no evidence of ongoing adaptation to
Herdr 0.7.x or newer Claude releases.

## 5. Strategic verdict and recommendations

### Verdict: pattern-source

It is not irrelevant: it proves that Claude's native agent-team control plane can
be retained while its panes are projected into Herdr, and it contains the clearest
source-level description I found of the old Herdr socket protocol. It is also not
a broad threat: it serves one agent, one experimental backend, one validated
platform/version pair, with no worktrees, adoption, durable runs, configurable
roster, cross-agent messaging, or plugin lifecycle.

For a user whose only requirement is “Claude native teams, including native
SendMessage/tasks, but visible in Herdr,” it is a **narrowly superior experience**
to our abstraction. That is product signal, not a reason to converge architectures.

### What to steal, in priority order

1. **Direct socket transport behind our existing `HerdrApi` seam — steal the
   pattern, rederive the contract.** Our adapter currently spawns `herdr` CLI
   subprocesses for every operation (`local: src/herdr.rs:130-260`). A Rust
   newline-framed Unix-socket client would reduce startup/parse overhead, expose
   operations not yet surfaced by CLI, and make event/wait handling more direct.
   It must be generated/validated from our checked-in protocol-16 schema and
   current `herdr api` output, with `ping` negotiation, response-ID checking,
   typed `result.type` validation, maximum frame size, and structured errors.

2. **Hermetic fake socket plus contract fixtures.** Port the target's
   `ThreadingUnixStreamServer` idea into a small Rust fake: record method/params,
   inject typed errors, and replay protocol-16 fixtures. This would test our
   transport independent of the CLI and complement `docs/herdr-api-schema.snapshot.json`.

3. **Per-call JSONL protocol tracing, with redaction.** A gated trace of request
   ID, method, result type, latency, and error code would make version drift
   obvious. Do not log prompt/message text by default.

4. **A capability handshake.** The target's design says protocol drift matters
   but its code never calls `ping`. We should do the missing part: record server
   version/protocol, reject unsupported schemas clearly, and allow capability
   checks rather than version-only branching.

5. **Keep an optional Claude-native visible-team experiment separate.** If demand
   appears, prototype a launcher mode that preserves Claude's native team/mailbox
   while using current Herdr panes. Treat it as a Claude provider/compatibility
   mode, not the core of `herdr-agent-team`, because Claude would own spawning,
   naming, task state, and teardown. Re-capture the current Claude tmux contract
   first; do not vendor this shim unchanged.

6. **Retain its stable-identity lesson, not its resolver.** Never derive durable
   identity from display order or positional indexes. On current Herdr, prefer
   the documented opaque stable public pane ID. Only add terminal-ID resolution
   if live protocol-16 evidence shows a real need.

### What not to steal

- The fake tmux environment as our primary architecture.
- The Herdr 0.6.10 `w<hex>-N` / `p_N` ID grammar.
- The hand-written partial tmux parser/format renderer.
- The `cd ... && env ...` string-rewrite heuristic.
- Per-operation `pane.list` scans unless the current API truly lacks a stable
  operation handle.
- Silent success for unsupported resize/layout/option behavior.
- Raw prompt payloads in trace logs.

### Recommended next move

Open one bounded engineering issue: **“Add an experimental protocol-16 direct
socket backend behind `HerdrApi`, preserving the CLI backend as fallback.”** The
acceptance bar should be parity for `workspace_create/close`, `pane_run/get`,
`agent_list/wait`, worktree operations, and event envelopes against a fake socket
and one live Herdr session. This captures the target's durable value without
coupling our heterogeneous orchestration model to Claude's private tmux behavior.

READY FOR REVIEW
