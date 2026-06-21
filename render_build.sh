#!/usr/bin/env bash
set -euo pipefail

python -m pip install --upgrade pip
pip install -r requirements.txt

# 書き込み可能な /tmp に Rust を固定導入する。
export CARGO_HOME="/tmp/cargo"
export RUSTUP_HOME="/tmp/rustup"
mkdir -p "$CARGO_HOME" "$RUSTUP_HOME"

# Render 既設の古い Rust を使わず、常に新しい stable を /tmp に導入して使う。
# （古い rustc だと icu 等の新しい依存がビルドできず、古いバイナリのまま稼働してしまう）
curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --default-toolchain stable
. "$CARGO_HOME/env"
export PATH="$CARGO_HOME/bin:$PATH"

rustup default stable
rustc --version
cargo --version

cd rust_core
# 確実に最新ソースで作り直す。
cargo clean
cargo build --release --bin core1_etb

test -x target/release/core1_etb
