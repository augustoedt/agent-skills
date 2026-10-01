#!/usr/bin/env bash
# Builds and installs the docs-search binary from this repository.

set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$REPO_DIR/tools/docs-search/Cargo.toml"

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found; install Rust before installing docs-search" >&2
  exit 1
fi
if [ ! -f "$MANIFEST" ]; then
  echo "error: docs-search manifest not found at $MANIFEST" >&2
  exit 1
fi

echo "installing docs-search from $MANIFEST"
INSTALL_ROOT=""
if command -v asdf >/dev/null 2>&1; then
  INSTALL_ROOT="$(asdf where rust 2>/dev/null || true)"
fi

if [ -n "$INSTALL_ROOT" ]; then
  cargo install --locked --force --no-default-features --root "$INSTALL_ROOT" --path "$REPO_DIR/tools/docs-search"
  asdf reshim rust
else
  INSTALL_ROOT="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}"
  cargo install --locked --force --no-default-features --root "$INSTALL_ROOT" --path "$REPO_DIR/tools/docs-search"
fi

hash -r
BINARY="$(command -v docs-search 2>/dev/null || true)"
if [ -z "$BINARY" ]; then
  echo "error: docs-search was installed but is not available on PATH" >&2
  echo "       add $INSTALL_ROOT/bin to PATH or reshim your Rust version manager" >&2
  exit 1
fi

echo
echo "installed: $BINARY"
"$BINARY" --version
