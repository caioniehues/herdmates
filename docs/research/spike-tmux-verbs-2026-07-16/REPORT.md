# REPORT — tmux verb inventory for herdmates shim (ADR-0012)

Generated: 2026-07-16  
Raw log line count: **36** (2 teammates × full lifecycle)

---

## 1. Inventory

All 36 calls were `stdin=notty` with `$TMUX` set (inside the outer default tmux session).  
No control mode (`-C`/`-CC`) detected anywhere.

All calls used `-S /tmp/tmux-1000/default` to target a specific socket (except the 9 startup probes below), meaning the shim must also handle the `-S` global flag transparently or parse it out.

### Deduped verb table

| # | Verb + arg shape | Count | Phase | Notes |
|---|---|---|---|---|
| 1 | `show -Av mouse` | 3 | startup probe | Once per claude process launch; reads current mouse setting |
| 2 | `show -gv focus-events` | 3 | startup probe | Once per claude process launch |
| 3 | `display-message -p #{client_termtype}` | 3 | startup probe | Once per claude process launch; reads terminal type |
| 4 | `display-message -t %N -p #{window_id}` | 1 | pane setup | Gets window ID of current pane — **output consumed** |
| 5 | `list-panes -t @N -F #{pane_id}` | 5 | pane setup + layout | Enumerate pane IDs — **output consumed every call** |
| 6 | `split-window -d -t %N -h -l 70% -P -F #{pane_id} -- cat` | 1 | spawn (1st teammate) | Horizontal split; -P prints new pane ID — **output consumed** |
| 7 | `split-window -d -t %N -v -P -F #{pane_id} -- cat` | 1 | spawn (2nd teammate) | Vertical split off prior pane; -P prints pane ID — **output consumed** |
| 8 | `set-option -p -t %N window-style bg=default,fg=COLOR` | 2 | styling | Per-pane foreground color (blue=alpha, green=beta) |
| 9 | `set-option -p -t %N pane-border-style fg=COLOR` | 2 | styling | Border color |
| 10 | `set-option -p -t %N pane-active-border-style fg=COLOR` | 2 | styling | Active border color |
| 11 | `select-pane -t %N -T NAME` | 2 | title | Set pane title to teammate name |
| 12 | `set-option -p -t %N pane-border-format #{...}` | 2 | styling | Format string with color + title |
| 13 | `set-option -w -t @N pane-border-status top` | 1 | window config | Show pane title bar; window-level, set once |
| 14 | `set-option -p -t %N remain-on-exit failed` | 2 | reliability | Keep pane alive after process exits (on failure) |
| 15 | `respawn-pane -k -t %N -- cd PATH && env ... claude ...` | 2 | launch | Replace `cat` placeholder with actual teammate process |
| 16 | `select-layout -t @N main-vertical` | 1 | layout | Apply `main-vertical` preset after both panes exist |
| 17 | `resize-pane -t %0 -x 30%` | 1 | layout | Shrink lead pane to 30% width |
| 18 | `kill-pane -t %N` | 2 | teardown | One per teammate on disband |

**Total unique verb shapes: 18**

---

## 2. Kill-signal checks

### Control mode (`-C` / `-CC` / `control-mode`)?

**NO.** Not a single `-C` flag appears in the 36 log lines. Claude Code does not use tmux control mode.

Evidence: `grep '\-C' tmux-calls.log` → zero matches.

### Output-consuming queries (`display-message -p`, `list-panes -F`, `split-window -P`, `show`)?

**YES — multiple.** Claude Code consumes output from these calls:

| Call | What it reads | How used |
|---|---|---|
| `show -Av mouse` | current mouse setting | startup capability probe |
| `show -gv focus-events` | focus-events setting | startup capability probe |
| `display-message -p #{client_termtype}` | terminal type string | startup capability probe |
| `display-message -t %0 -p #{window_id}` | window ID (`@0`) | target for subsequent pane ops |
| `list-panes -t @N -F #{pane_id}` (×5) | list of pane IDs (`%0`, `%1`, `%2`) | target for split-window, set-option, etc. |
| `split-window ... -P -F #{pane_id}` (×2) | newly created pane ID | immediately used to style/title/respawn that pane |

`wait-for` and hooks: **NO** — not seen.

**The pane-ID feedback loop is the shim's hardest problem.** Claude Code reads the pane ID from `split-window -P` and uses it in the very next call. The shim must intercept the response, translate tmux `%N` IDs to herdr pane IDs, and return plausible `%N` values back to the caller.

---

## 3. Verb → herdr mapping

Herdr CLI tested: `herdr pane`, `herdr tab`, `herdr workspace` (all confirmed running).

| tmux verb | herdr equivalent | Gap? |
|---|---|---|
| `show -Av mouse` | NONE-EXISTS | Startup probe only — shim can return a static safe value |
| `show -gv focus-events` | NONE-EXISTS | Same — shim can return `0` |
| `display-message -p #{client_termtype}` | NONE-EXISTS | Shim returns static string (e.g. `xterm-256color`) |
| `display-message -t %N -p #{window_id}` | `herdr tab get` (returns tab JSON with ID) | Needs ID translation; response format differs |
| `list-panes -t @N -F #{pane_id}` | `herdr pane list --workspace W` | Output format differs; shim must emit `%N` lines |
| `split-window -d ... -h/v -P -F #{pane_id} -- cat` | `herdr pane split --direction right\|down --cwd PATH` | herdr returns pane ID in JSON, not bare `%N`; shim must translate |
| `set-option -p window-style fg=COLOR` | NONE-EXISTS | Per-pane foreground color; no herdr equivalent |
| `set-option -p pane-border-style fg=COLOR` | NONE-EXISTS | Border colors; no herdr equivalent |
| `set-option -p pane-active-border-style fg=COLOR` | NONE-EXISTS | Active border; no herdr equivalent |
| `select-pane -t %N -T NAME` | `herdr pane rename PANE_ID LABEL` | Supported; shim translates `%N` → herdr pane ID |
| `set-option -p pane-border-format ...` | NONE-EXISTS | Format strings; herdr uses its own title display |
| `set-option -w pane-border-status top` | NONE-EXISTS | Window-level config; no herdr equivalent |
| `set-option -p remain-on-exit failed` | NONE-EXISTS | Pane persistence policy; no herdr equivalent |
| `respawn-pane -k -t %N -- CMD` | `herdr pane run PANE_ID CMD` | Closest match; `pane run` executes in an existing pane |
| `select-layout main-vertical` | NONE-EXISTS (no layout presets) | Work around with `herdr pane split` + `herdr pane resize` |
| `resize-pane -t %0 -x 30%` | `herdr pane resize --direction right --amount 0.30` | Supported; direction convention differs |
| `kill-pane -t %N` | `herdr pane close PANE_ID` | Supported; shim translates `%N` → herdr pane ID |

**Gaps summary:** 7 verbs have no herdr equivalent (all styling/config options). 3 verbs have partial equivalents requiring ID translation. 3 are static-fakeable startup probes. Only `pane rename`, `pane resize`, `pane close`, and `pane split`/`pane run` map cleanly.

---

## 4. Verdict

**BUILD-WITH-RISKS**

The structural operations (split, rename, resize, close, run) all have herdr equivalents, so the shim's core behavior is implementable. However, the **pane-ID feedback loop** (lines 4–5 above) is a non-trivial translation layer the shim must maintain across calls, and the **7 missing styling verbs** mean teammate panes will appear unstyled (no color-coded borders, no title bars) in herdr — users lose the visual differentiation Claude Code relies on. The kill-signal checks are clean: no control mode, no hooks, so there are no hidden protocol surfaces to shim; all interactions are discrete CLI invocations.

---

## 5. Environment record

| Field | Value |
|---|---|
| `claude --version` | `2.1.211 (Claude Code)` |
| `tmux -V` | `tmux 3.7b` |
| Date | 2026-07-16 |
| Teammate mode | `--settings '{"teammateMode":"tmux"}'` (no `--teammate-mode` CLI flag exists in v2.1.211) |
| Env var used | `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` |
| Full launch command | `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1 claude --settings /home/caio/Projects/herdmates-spike-recon/spike-settings.json --dangerously-skip-permissions` |
| `--teammate-mode` flag? | **NOT PRESENT** in v2.1.211 — `teammateMode` is a settings-file key only |
| Teammate model | `claude-opus-4-8` (set by lead automatically) |
| Teammates spawned | alpha (blue), beta (green) |
| Lifecycle exercised | spawn → task → SendMessage follow-up → wait → kill-pane teardown |
