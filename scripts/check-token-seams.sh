#!/usr/bin/env bash
# Boundary-crossing check for the sidebar-token seam (issue #127 pattern).
#
# Two sides have to agree and nothing enforced it:
#
#   src/tokens.rs             the producer: token names the board pump publishes
#   docs/sidebar-rows.toml    the consumer: the `$name` rows herdr renders
#
# Both failure directions are silent, which is why this class needs a
# cross-comparison rather than an existence check on one side:
#
#   - Published with no row: the token reaches herdr and renders NOWHERE. The
#     pump looks healthy, `pane list` shows the token, and the human sees
#     nothing. This is how `$reap` shipped invisible in the first draft of the
#     2026-09-03 reaping work.
#   - A row naming no producer: `herdr server reload-config` returns status
#     "partial" and keeps the OLD sidebar layout — no crash, no error, the UI
#     just stops responding to edits (sidebar-rows.toml documents this).
#
# Known limitation, mutation-proven rather than assumed: if the producer's
# construction shape changes for SOME tokens (say one switches to
# `String::from`), the parser misses those and the drift surfaces as
# TOKEN-ROW-PHANTOM — the wrong tag for the actual cause. It still fails, and
# TOKEN-PARSE catches the total-shape change, so no shape drift passes
# silently; only the diagnosis can mislead. Read the FIX line against
# src/tokens.rs before deleting a row it names.
#
# Output is agent-readable: [TAG] / File: / FIX:
# Exit 0 = seams agree, 1 = drift found.
set -uo pipefail
cd "$(dirname -- "$0")/.." || exit 1

fail=0
report() { printf '[%s]\nFile: %s\nFIX: %s\n\n' "$1" "$2" "$3"; fail=1; }

# --- Side A: names the producer publishes (the truth) ----------------------
# Every `name: "..."` field of a Token literal in tokens.rs.
# Production code only: everything from `#[cfg(test)]` down is fixtures, and a
# test teammate literally named `alpha` is not a token the pump publishes.
published=$(sed '/#\[cfg(test)\]/,$d' src/tokens.rs \
  | grep -oE 'name: "[a-z][a-z_]*"\.to_owned\(\)' \
  | grep -oE '"[a-z][a-z_]*"' | tr -d '"' | sort -u)
if [ -z "$published" ]; then
  report "TOKEN-PARSE" "src/tokens.rs" \
    "No Token name literals matched the parser. The token construction shape changed — update this script's regex before trusting it."
  exit 1
fi

# --- Side B: rows the shipped config renders ------------------------------
rendered=$(grep -oE '"\$[a-z][a-z_]*"' docs/sidebar-rows.toml \
  | tr -d '"$' | sort -u)
if [ -z "$rendered" ]; then
  report "TOKEN-ROWS-MISSING" "docs/sidebar-rows.toml" \
    "No \$token rows found. If the rows table moved, point this check at its new home."
  exit 1
fi

only_published=$(comm -23 <(echo "$published") <(echo "$rendered"))
only_rendered=$(comm -13 <(echo "$published") <(echo "$rendered"))

[ -n "$only_published" ] && report "TOKEN-ROW-MISSING" "docs/sidebar-rows.toml" \
  "Published by src/tokens.rs but rendered by no row: $(echo "$only_published" | tr '\n' ' ')— add a [\"\$name\"] row, or stop publishing it. A token with no row is invisible, not degraded."
[ -n "$only_rendered" ] && report "TOKEN-ROW-PHANTOM" "docs/sidebar-rows.toml" \
  "Rendered by a row but published by nothing: $(echo "$only_rendered" | tr '\n' ' ')— delete the row, or add the producer. An unknown name makes reload-config return \"partial\" and silently keep the old layout."

if [ "$fail" -eq 0 ]; then
  echo "OK: sidebar-token seams agree — $(echo "$published" | tr '\n' ' ' | sed 's/ $//')"
fi
exit "$fail"
