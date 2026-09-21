#!/usr/bin/env bash
# Updates third-party skills (NOT versioned in skills/) by re-running each
# one's own update command, as recorded in third-party.json at the repo root.
#
# This repo never stores third-party skill content — only where it came from
# and how to refresh it. Each entry's update_cmd is whatever that skill's own
# vendor uses to update itself (marketplace install, a vendor CLI's own
# upgrade command, etc.) — this script does not know or care how each one
# works internally, it just runs the recorded command.
#
# Usage:
#   scripts/update-third-party.sh                # update every entry
#   scripts/update-third-party.sh use-railway     # update just one

set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$REPO_DIR/third-party.json"

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq not found (needed to read third-party.json)" >&2
  exit 1
fi
if [ ! -f "$MANIFEST" ]; then
  echo "error: $MANIFEST not found" >&2
  exit 1
fi

all_names=()
while IFS= read -r name; do
  all_names+=("$name")
done < <(jq -r 'keys[]' "$MANIFEST")

targets=("$@")
if [ "${#targets[@]}" -eq 0 ]; then
  targets=("${all_names[@]}")
fi

fail=0
for name in "${targets[@]}"; do
  entry="$(jq -r --arg n "$name" '.[$n] // empty' "$MANIFEST")"
  if [ -z "$entry" ]; then
    echo "error: '$name' not found in $MANIFEST, skipping" >&2
    fail=1
    continue
  fi
  cmd="$(jq -r '.update_cmd' <<< "$entry")"
  source="$(jq -r '.source' <<< "$entry")"
  echo "=== $name ==="
  echo "source: $source"
  echo "run:    $cmd"
  if eval "$cmd"; then
    echo "ok:     $name updated"
  else
    echo "error:  $name update command failed" >&2
    fail=1
  fi
  echo
done

exit "$fail"
