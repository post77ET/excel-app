#!/usr/bin/env bash
set -euo pipefail

python -m pip install --upgrade pip
pip install -r requirements.txt

# Render は CARGO_HOME=/usr/local/cargo（読み取り専用）を設定してくるため、
# クレートのダウンロード先を書き込み可能な /tmp に強制上書きする。
export CARGO_HOME="/tmp/cargo"
export RUSTUP_HOME="/tmp/rustup"
mkdir -p "$CARGO_HOME" "$RUSTUP_HOME"
export PATH="$CARGO_HOME/bin:$PATH"

if ! command -v cargo >/dev/null 2>&1; then
  # Rust が無い場合のみ、書き込み可能な /tmp に stable を導入する。
  curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --default-toolchain stable
  if [ -f "$CARGO_HOME/env" ]; then
    . "$CARGO_HOME/env"
  fi
fi
export PATH="$CARGO_HOME/bin:$PATH"

# 既存の Rust（読み取り専用の /usr/local/rustup 等）がある場合は
# rustup default を実行しない（settings.toml への書き込みで失敗するため）。
cargo --version

cd rust_core
cargo build --release --bin core1_etb

test -x target/release/core1_etb || test -f target/release/core1_etb.exe
