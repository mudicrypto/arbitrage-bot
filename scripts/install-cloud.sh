#!/usr/bin/env bash
set -euo pipefail
export CARGO_HOME=/workspace/.tools/cargo
export RUSTUP_HOME=/workspace/.tools/rustup
export PATH="$CARGO_HOME/bin:$PATH"
export npm_config_cache=/workspace/.tools/npm-cache
mkdir -p /workspace/.tools
if ! command -v rustup >/dev/null 2>&1; then
  curl --fail --silent --show-error https://sh.rustup.rs -o /workspace/.tools/rustup-init.sh
  sh /workspace/.tools/rustup-init.sh -y --no-modify-path --profile minimal --default-toolchain 1.90.0
fi
cd /workspace/arbitrage-bot
rustup toolchain install 1.90.0 --profile minimal --component rustfmt --component clippy
npm ci --prefix control-api --ignore-scripts
cargo build --workspace --locked
bash scripts/check.sh
docker compose pull postgres
