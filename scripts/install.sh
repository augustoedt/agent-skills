#!/usr/bin/env bash
# Syncs skills/ from this repo into the local agent skills directory
# (default: ~/.pi/agent/skills). One-way: repo is the source of truth.
#
# Usage:
#   scripts/install.sh              # sync every skill under skills/
#   scripts/install.sh phoenix-ash-admin-ui   # sync just one skill
#
# Override destination with AGENT_SKILLS_DEST.

set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_DIR="$REPO_DIR/skills"
DEST_DIR="${AGENT_SKILLS_DEST:-$HOME/.pi/agent/skills}"

if [ ! -d "$SOURCE_DIR" ]; then
  echo "error: $SOURCE_DIR not found" >&2
  exit 1
fi

mkdir -p "$DEST_DIR"

targets=("$@")
if [ "${#targets[@]}" -eq 0 ]; then
  for skill_path in "$SOURCE_DIR"/*/; do
    targets+=("$(basename "$skill_path")")
  done
fi

for name in "${targets[@]}"; do
  skill_source="$SOURCE_DIR/$name"
  if [ ! -d "$skill_source" ]; then
    echo "error: skills/$name not found in repo, skipping" >&2
    continue
  fi
  echo "syncing $name -> $DEST_DIR/$name"
  # --delete is scoped to this single skill's directory only: it never
  # touches sibling skill directories at the destination (e.g. third-party
  # skills this repo does not version - see README).
  rsync -a --delete "$skill_source/" "$DEST_DIR/$name/"
done

echo
echo "done. verify with:"
echo "  diff -rq \"$SOURCE_DIR\" \"$DEST_DIR\""
