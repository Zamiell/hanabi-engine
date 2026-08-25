#!/usr/bin/env bash

set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"

if [[ -d "$HOME/.cargo/bin" ]]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

echo 'Running the exhaustive H-Group profile matrix...'
cargo test --locked -p hanabi-search profile_rollout_ -- --ignored

echo 'All exhaustive checks passed.'
