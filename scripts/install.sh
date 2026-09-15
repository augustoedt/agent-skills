#!/usr/bin/env bash
# Syncs skills/ from this repo into ONE canonical directory and links them
# into every agent on this machine. One-way: repo is the source of truth.
#
# Layout:
#   ~/.agents/skills/            canonical copy (real files, via rsync)
#   ~/.pi/agent/skills/<skill>   symlink -> ~/.agents/skills/<skill>   (pi)
#   ~/.claude/skills/<skill>     symlink (Claude Code)
#   ~/.codex/skills/<skill>      symlink (Codex CLI)
#   ~/.grok/skills/<skill>       symlink (Grok CLI)
#   ~/.copilot/skills/<skill>    symlink (GitHub Copilot CLI)
#   ~/.cursor/skills/<skill>     symlink (Cursor)
#   ~/.agent/skills/<skill>      symlink (agent; skipped if ~/.agent missing)
#
# Only agents whose base folder exists in $HOME get links; the rest are
# skipped (reported at the end). Third-party skills (managed by `npx skills`)
# are never touched: rsync --delete is scoped per skill and symlinks are only
# managed for names that come from this repo.
#
# Usage:
#   scripts/install.sh                          # sync every skill under skills/
#   scripts/install.sh phoenix-ash-admin-ui     # sync just one skill
#
# Env overrides:
#   AGENT_SKILLS_DEST=/some/dir   # use this dir as the canonical copy instead
#
# If an agent does not follow symlinks, append ":copy" to its entry in
# SKILL_TARGETS below to give it a real copy instead.

set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_DIR="$REPO_DIR/skills"

CANONICAL_DIR="${AGENT_SKILLS_DEST:-$HOME/.agents/skills}"

# Agent targets: "<base-dir-in-HOME>:<skills-subdir>[:copy]" (bash 3.2-safe:
# plain array of pairs, no associative arrays). "symlink" is the default mode.
SKILL_TARGETS=(
  ".pi:agent/skills"
  ".claude:skills"
  ".codex:skills"
  ".grok:skills"
  ".copilot:skills"
  ".cursor:skills"
  ".agent:skills"
)

# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------
relpath() {
  # relpath <from-dir> <to-dir>: prints relative path from -> to (both abs)
  local from="$1" to="$2" i=0 n_from n_to j result=""
  local from_parts=() to_parts=()
  local IFS="/"
  read -a from_parts <<< "${from#/}"
  read -a to_parts <<< "${to#/}"
  n_from=${#from_parts[@]}
  n_to=${#to_parts[@]}
  while [ "$i" -lt "$n_from" ] && [ "$i" -lt "$n_to" ] \
      && [ "${from_parts[$i]}" = "${to_parts[$i]}" ]; do
    i=$((i + 1))
  done
  j=$((n_from - i))
  while [ "$j" -gt 0 ]; do result="../$result"; j=$((j - 1)); done
  while [ "$i" -lt "$n_to" ]; do result="${result}${to_parts[$i]}/"; i=$((i + 1)); done
  printf '%s' "${result%/}"
}

# ---------------------------------------------------------------------------
# 1. validate user folder + source
# ---------------------------------------------------------------------------
if [ -z "${HOME:-}" ] || [ ! -d "$HOME" ]; then
  echo "error: HOME ('${HOME:-}') is not set or is not a directory" >&2
  echo "       run this script from your user account" >&2
  exit 1
fi
if [ ! -w "$HOME" ]; then
  echo "error: $HOME is not writable" >&2
  exit 1
fi
if [[ "$REPO_DIR" != "$HOME"/* ]]; then
  echo "warning: repo at $REPO_DIR is outside your home folder" >&2
fi
if [ ! -d "$SOURCE_DIR" ]; then
  echo "error: $SOURCE_DIR not found" >&2
  exit 1
fi
if ! command -v rsync >/dev/null 2>&1; then
  echo "error: rsync not found" >&2
  exit 1
fi

echo "system:    $(uname -s) ($(uname -m))"
echo "home:      $HOME"
echo "source:    $SOURCE_DIR"
echo "canonical: $CANONICAL_DIR"
echo

# skill selection: all of skills/ unless names given on the command line
targets=("$@")
if [ "${#targets[@]}" -eq 0 ]; then
  for skill_path in "$SOURCE_DIR"/*/; do
    targets+=("$(basename "$skill_path")")
  done
fi
for name in "${targets[@]}"; do
  if [ ! -d "$SOURCE_DIR/$name" ]; then
    echo "error: skills/$name not found in repo, skipping" >&2
  fi
done

# ---------------------------------------------------------------------------
# 2. sync canonical copy
# ---------------------------------------------------------------------------
mkdir -p "$CANONICAL_DIR"
for name in "${targets[@]}"; do
  skill_source="$SOURCE_DIR/$name"
  [ -d "$skill_source" ] || continue
  echo "sync:     $name -> $CANONICAL_DIR/$name"
  # --delete is scoped to this single skill's directory only: it never
  # touches sibling skills at the destination (e.g. third-party skills
  # this repo does not version - see README).
  rsync -a --delete "$skill_source/" "$CANONICAL_DIR/$name/"
done

# ---------------------------------------------------------------------------
# 3. detect agents and prepare per-agent destinations
# ---------------------------------------------------------------------------
link_dirs=()
copy_dirs=()
skipped=""
for entry in "${SKILL_TARGETS[@]}"; do
  base="${entry%%:*}"
  rest="${entry#*:}"
  subdir="${rest%%:*}"
  mode="symlink"
  case "$rest" in
    *:*) mode="${rest##*:}" ;;
  esac
  base_dir="$HOME/$base"
  if [ ! -d "$base_dir" ]; then
    skipped="$skipped $base"
    continue
  fi
  dest="$base_dir/$subdir"
  if [ "$dest" = "$CANONICAL_DIR" ]; then
    continue   # canonical itself: nothing to link
  fi
  mkdir -p "$dest"
  if [ "$mode" = "copy" ]; then
    copy_dirs+=("$dest")
  else
    link_dirs+=("$dest")
  fi
  echo "found:    ~/$base -> $dest (mode: $mode)"
done
echo

if [ "${#link_dirs[@]}" -eq 0 ] && [ "${#copy_dirs[@]}" -eq 0 ]; then
  echo "warning: no agent folders found in $HOME; only the canonical copy was updated" >&2
fi

# ---------------------------------------------------------------------------
# 4a. symlink mode
# ---------------------------------------------------------------------------
if [ "${#link_dirs[@]}" -gt 0 ]; then
for dest in "${link_dirs[@]}"; do
  for name in "${targets[@]}"; do
    skill_source="$SOURCE_DIR/$name"
    [ -d "$skill_source" ] || continue
    link="$dest/$name"
    target_rel="$(relpath "$dest" "$CANONICAL_DIR")/$name"
    if [ -L "$link" ]; then
      cur="$(readlink "$link" 2>/dev/null || true)"
      if [ "$cur" = "$target_rel" ]; then
        echo "ok:       $link"
        continue
      fi
      echo "repair:   $link -> $target_rel"
      rm -f "$link"
      ln -s "$target_rel" "$link"
    elif [ -e "$link" ]; then
      if [ -d "$link" ] && diff -rq "$link" "$CANONICAL_DIR/$name" >/dev/null 2>&1; then
        echo "convert:  $link (identical legacy copy) -> symlink"
        rm -rf "$link"
        ln -s "$target_rel" "$link"
      else
        echo "warning:  $link already exists and differs; leaving it in place" >&2
      fi
    else
      echo "link:     $link -> $target_rel"
      ln -s "$target_rel" "$link"
    fi
  done
done
fi

# ---------------------------------------------------------------------------
# 4b. copy mode (agents that don't follow symlinks)
# ---------------------------------------------------------------------------
if [ "${#copy_dirs[@]}" -gt 0 ]; then
for dest in "${copy_dirs[@]}"; do
  for name in "${targets[@]}"; do
    skill_source="$SOURCE_DIR/$name"
    [ -d "$skill_source" ] || continue
    echo "copy:     $name -> $dest/$name"
    rsync -a --delete "$skill_source/" "$dest/$name/"
  done
done
fi

# ---------------------------------------------------------------------------
# 4c. stale links: symlinks into canonical whose skill no longer exists
# ---------------------------------------------------------------------------
if [ "${#link_dirs[@]}" -gt 0 ]; then
for dest in "${link_dirs[@]}"; do
  for link in "$dest"/*; do
    [ -L "$link" ] || continue
    name="$(basename "$link")"
    [ -e "$CANONICAL_DIR/$name" ] && continue
    expected="$(relpath "$dest" "$CANONICAL_DIR")/$name"
    cur="$(readlink "$link" 2>/dev/null || true)"
    if [ "$cur" = "$expected" ]; then
      echo "remove:   stale link $link"
      rm -f "$link"
    fi
  done
done
fi

# ---------------------------------------------------------------------------
# report
# ---------------------------------------------------------------------------
echo
if [ "${#link_dirs[@]}" -gt 0 ]; then
for dest in "${link_dirs[@]}"; do
  echo "linked:   $dest"
done
fi
if [ "${#copy_dirs[@]}" -gt 0 ]; then
for dest in "${copy_dirs[@]}"; do
  echo "copied:   $dest"
done
fi
if [ -n "$skipped" ]; then
  echo "skipped (folder not found in $HOME):$skipped"
fi
echo
echo "done. verify with:"
echo "  diff -rq \"$SOURCE_DIR\" \"$CANONICAL_DIR\""
if [ "${#link_dirs[@]}" -gt 0 ]; then
for dest in "${link_dirs[@]}"; do
  echo "  ls -l \"$dest\""
done
fi
