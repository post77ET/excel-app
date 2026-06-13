#!/usr/bin/env bash
set -euo pipefail

python -m pip install --upgrade pip
pip install -r requirements.txt

export CARGO_HOME="${CARGO_HOME:-/tmp/cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-/tmp/rustup}"
export PATH="$CARGO_HOME/bin:$PATH"

if ! command -v cargo >/dev/null 2>&1; then
  curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path
fi

if [ -f "$CARGO_HOME/env" ]; then
  . "$CARGO_HOME/env"
fi
export PATH="$CARGO_HOME/bin:$PATH"

rustup default stable
cargo --version

cd rust_core
cargo build --release --bin core1_etb

test -x target/release/core1_etb || test -f target/release/core1_etb.exe
