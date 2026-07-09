# excel-app 修正版レポート 2026-05-24

## 実施済み

- root `src/` を削除し、Rust source of truth を `rust_core/` のみに統一。
- root `Cargo.toml` / `Cargo.lock` を削除し、二重Cargo構造を排除。
- `input/`, `output/`, `uploads/`, `working/`, `server_originals/` の過去実行残骸を削除し、`.gitkeep` のみ残存。
- Flask `/generate` と Rust `generate-select` の契約を維持。
- `ETB_BIN_PATH` 未設定時は `rust_core/target/.../core1_etb` のみ探索し、未存在なら明示エラー化。
- Apply後の同一ファイル `shutil.copy2()` による `SameFileError` リスクを除去。
- Apply列Lは `Y/y/Ｙ/ｙ` を許可。
- `render_build.sh` は `rust_core` をビルド対象に固定。
- Dockerfile は `rust_core` をビルド対象に固定。
- `ETB_FORCE_MOCK_TRANSLATORS=1` によるローカル検証用Mock翻訳経路を追加。
- `RELEASE_CHECK_20260524.ps1` を追加。

## 未保証

- この実行環境には `cargo` が無いため、こちら側では Rust 実ビルド未実行。
- この実行環境には Flask が無いため、こちら側では Web E2E 未実行。
