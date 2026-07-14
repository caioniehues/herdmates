#!/usr/bin/env bash
# Test fixture: dump every HERDR_* env var plus the event payload to a log file,
# one JSON-ish block per invocation, so spec §9 payload-shape TODOs can be
# verified against a live event instead of the socket schema alone.
LOG="${HERDR_EVENT_LOG:-/tmp/herdr-event-logger.log}"
{
  echo "=== $(date -Is) ==="
  env | grep '^HERDR_' | sort
  echo "=== end ==="
} >> "$LOG"
