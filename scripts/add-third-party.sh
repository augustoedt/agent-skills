#!/usr/bin/env bash
# Installs a third-party skill (via its own vendor mechanism) and catalogs it
# in third-party.json, then commits the manifest change locally.
#
# This script never copies third-party skill content into this repo — it
# only runs the real install command and records where it came from, exactly
# like scripts/update-third-party.sh later re-runs to refresh it.
#
# Usage:
#   scripts/add-third-party.sh <name> <source-id> <description> [update_cmd]
#
#   <name>          catalog key, e.g. "use-railway"
#   <source-id>     "owner/repo@skill" as printed by `npx skills find <name>`
#   <description>   short human description
#   [update_cmd]    defaults to: npx skills add <source-id> -g -y
#
# Example:
#   scripts/add-third-party.sh use-railway railwayapp/railway-skills@use-railway \
#     "Railway CLI skill - infra/deploy"
#
# This script does NOT search the marketplace or disambiguate multiple
# matches — that's the job of the add-third-party-skill SKILL.md, which
# resolves <source-id> with the user before calling this script.

set -euo pipefail

if [ "$#" -lt 3 ]; then
  echo "usage: $0 <name> <source-id> <description> [update_cmd]" >&2
  exit 1
fi

NAME="$1"
SOURCE_ID="$2"
DESCRIPTION="$3"
UPDATE_CMD="${4:-npx skills add $SOURCE_ID -g -y}"

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$REPO_DIR/third-party.json"

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq not found (needed to write third-party.json)" >&2
  exit 1
fi
if [ ! -f "$MANIFEST" ]; then
  echo "error: $MANIFEST not found" >&2
  exit 1
fi

already_exists="false"
if jq -e --arg n "$NAME" 'has($n)' "$MANIFEST" >/dev/null 2>&1; then
  already_exists="true"
fi

# ---------------------------------------------------------------------------
# 1. install for real, via the vendor's own mechanism
# ---------------------------------------------------------------------------
echo "installing: $NAME"
echo "source:     skills.sh marketplace: $SOURCE_ID"
echo "run:        $UPDATE_CMD"
eval "$UPDATE_CMD"

# ---------------------------------------------------------------------------
# 2. catalog it (add or overwrite the entry)
# ---------------------------------------------------------------------------
installed_at="$(date +%Y-%m-%d)"
tmp="$(mktemp)"
jq --arg n "$NAME" \
   --arg desc "$DESCRIPTION" \
   --arg src "skills.sh marketplace: $SOURCE_ID" \
   --arg cmd "$UPDATE_CMD" \
   --arg at "$installed_at" \
   '.[$n] = {description: $desc, source: $src, update_cmd: $cmd, installed_at: $at}' \
   "$MANIFEST" > "$tmp"
mv "$tmp" "$MANIFEST"

if [ "$already_exists" = "true" ]; then
  echo "cataloged:  $NAME (entry updated in third-party.json)"
else
  echo "cataloged:  $NAME (new entry in third-party.json)"
fi

# ---------------------------------------------------------------------------
# 3. commit locally (never pushes)
# ---------------------------------------------------------------------------
cd "$REPO_DIR"
git add third-party.json
if git diff --cached --quiet; then
  echo "git:        nothing to commit (manifest unchanged)"
else
  verb="add"
  [ "$already_exists" = "true" ] && verb="update"
  git commit -m "third-party: $verb $NAME" >/dev/null
  echo "git:        committed locally ($(git rev-parse --short HEAD))"
fi
