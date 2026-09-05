#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
# Keep the caller's working directory so relative replay paths remain valid.
# Cargo status goes to stderr; stdout contains only the URL.
exec cargo run --quiet --locked --manifest-path "$repository_root/Cargo.toml" \
  -p hanabi-cli --bin hanabi-engine -- replay-link "$@"
