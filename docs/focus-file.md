# The focus-file contract (D3, issue #86, ADR-0012 §3)

The focus file is the human's single-surface state: current task, the one
next concrete action, and a decision queue. It is a **plain-file
contract**, not an API — that's the whole point (ADR-0012: "editable by
hand, survives upstream churn").

## Path

```
~/.local/share/herdmates/focus.md
```

Not XDG-resolved, not overridable via env var (unlike the plugin's own
`HERDR_PLUGIN_STATE_DIR`/`HERDR_PLUGIN_CONFIG_DIR`) — this path is fixed
per ADR-0012's decision record. It sits outside the plugin's own state
directory on purpose: the file's usefulness comes from being addressable
by a human, an agent, or a script without going through the plugin at
all.

## Ownership rule

**Anything may write it. The focus pane only ever renders it, never owns
it.** Concretely:

- A human can open it in an editor and type directly.
- The atomizer skill (commit 8) writes it after atomizing a task dump.
- Any other agent or script is free to write it too — there is no lock,
  no daemon, no "only the plugin may touch this" rule.
- The focus pane (commit 6/7) reads it on a debounced file-watch and
  renders — it never writes back on its own initiative. The only writes
  the pane itself triggers are explicit human actions inside the TUI
  (e.g. marking a decision resolved), which round-trip through the same
  parse/render contract described below.

This is why the parser (`src/focusfile.rs`) is deliberately tolerant: a
hand-edit is not an edge case to guard against, it's the primary way this
file gets written.

## Format

A Markdown file with three fixed sections, in any order, each optional:

```markdown
# Focus

## Task
<free text — the current task, one or more lines>

## Next Action
<free text — the single next concrete action>

## Decisions
- [ ] <pending decision text>
- [x] <resolved decision text>
```

- The `# Focus` title line is cosmetic — present for readability when
  opened in a normal Markdown viewer, ignored by the parser.
- `## Task` and `## Next Action` bodies are free text, joined verbatim
  (newlines preserved) and trimmed. An empty or whitespace-only section
  parses to `None`, not `Some("")`.
- `## Decisions` entries are standard Markdown checkbox list items:
  `- [ ]` (pending) or `- [x]` / `- [X]` (resolved). Anything else in the
  Decisions section — a note, a malformed checkbox, a blank line — is
  silently skipped, not an error and not a decision entry.
- Section headings are case-insensitive (`## task` works); unrecognized
  `##` headings and any content before the first recognized heading are
  ignored, not fatal.

### Decision entry ids

Decision entries are addressed by a stable `id` used elsewhere in the
system (the attention queue, commit 3; the audit log, commit 4) — but
that id is **never written by hand**. It is derived automatically by the
parser from a hash of the entry's trimmed text (FNV-1a, 16 hex chars),
so:

- The same decision text always gets the same id, across every parse —
  including after the binary itself has been rebuilt (this rules out
  Rust's `std::DefaultHasher`, whose algorithm is explicitly unspecified
  and free to change between Rust releases).
- Two entries with identical text in the same file get disambiguated with
  a `-2`, `-3`, ... suffix on the second and later occurrences.
- Editing an entry's text changes its id. That is intentional: the id
  identifies *this specific decision as currently phrased*, not a
  slot in the list. If a human rewords a pending decision, anything that
  referenced the old id (e.g. an audit-log entry) is referencing the
  decision as it used to read, which is the correct semantics for an
  append-only audit trail.

## Examples

Minimal (fresh file, nothing recorded yet — this is also exactly what
`read_focus_file` returns for a **missing** file, which is not an error):

```markdown
# Focus

## Task

## Next Action

## Decisions
```

Typical in-progress state:

```markdown
# Focus

## Task
Ship the D3 focus pane foundation

## Next Action
Write the focusfile parser

## Decisions
- [ ] Ship split or overlay placement first?
- [x] Focus file lives at ~/.local/share/herdmates/focus.md
- [ ] Should jump wrap around the queue or stop at the ends?
```

Hand-edited, with stray content the parser tolerates and ignores (see
`tests/fixtures/focusfile/hand-edited.md` for the exact fixture this
behavior is tested against):

```markdown
# Focus

some human scribbled a note up here before the first heading, ignore it

## Task
Investigate the flaky CI run

  (yeah I know, again)

## Random Notes
This heading isn't part of the contract at all — should be ignored.

## Decisions
- [ ] Retry the job or bisect the commit range?
- not a checkbox line, just a note, skip me
- [X] Uppercase X should still count as checked
-[ ] malformed checkbox (no space after dash), silently skipped
```

## What re-serialization does and doesn't preserve

`render_focus_file` (used by any writer that reads, modifies, and writes
back through the Rust API rather than editing the file by hand) produces
a **canonical** re-serialization: the three sections in fixed order, one
decision per line. It does **not** preserve a hand-editor's stray notes,
extra headings, or original whitespace — those are contract-external and
were never part of the parsed model to begin with. Round-tripping through
`parse_focus_file_str` → `render_focus_file` → `parse_focus_file_str`
is stable for the contract fields (task, next action, decision
text/resolved state) — it is not byte-stable against arbitrary hand-edited
input, and isn't meant to be.
