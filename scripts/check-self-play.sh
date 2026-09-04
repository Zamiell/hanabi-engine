#!/usr/bin/env bash
set -euo pipefail
repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --release --locked -p hanabi-search --test self_play \
  h_group_max_self_play_200 -- --ignored --exact --nocapture
