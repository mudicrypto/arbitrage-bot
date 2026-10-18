#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -d /workspace/.tools/cargo/bin ]; then
  export CARGO_HOME=/workspace/.tools/cargo RUSTUP_HOME=/workspace/.tools/rustup
  export PATH="$CARGO_HOME/bin:$PATH"
fi
if [ "$#" -eq 0 ]; then set -- research; fi
exec cargo run --quiet --locked -p mm-backtest -- paper "$@"
