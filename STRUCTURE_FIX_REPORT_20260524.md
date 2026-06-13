# excel-app 構造修正版報告（2026-05-24）

## 修正対象
対象ZIP: `excel-app_structure_contract_fixed_20260524.zip`

## 修正方針
今回の修正は、報告書で指摘された最大危険「Apply契約崩壊」を主対象とした。Generate と Apply の責務を以下に固定した。

- Generate = 確認用 UI workbook を生成する工程
- Apply = サーバ保存原本 base workbook を土台に、UI の選択結果だけを反映する工程

## プログラム変更内容

### 1. Generate責務分離
対象: `rust_core/src/core2/generate_workbook_writer.rs`

Generate時に本体sheetへ翻訳結果を書き込む処理を除去した。Generateでは元ブックを読み、TRANSLATION_UI、SECURITY_REPORT、INTERNAL_METADATA、TRANSLATION_WARNINGSを追加するだけにした。

### 2. Applyをbase workbook土台へ変更
対象:
- `rust_core/src/app/apply_orchestrator.rs`
- `rust_core/src/core2/apply_workbook_writer.rs`

Applyの書込土台を UI workbook から base workbook に変更した。UI workbook は選択結果の読み取り専用、base workbook は最終出力の土台とする。

### 3. shared formula参照元をbase workbookへ変更
対象: `rust_core/src/core2/apply_workbook_writer.rs`

Apply時の shared formula 親セル検出と XML patch 参照元を base workbook に変更した。

### 4. Web Apply出力を明示固定
対象: `web_app/app.py`

Apply出力先を `output/<job_id>_<server_original_stem>_apply.xlsx` に固定し、SameFileErrorを避ける構造にした。

### 5. Render build固定
対象: `render_build.sh`

Python依存関係インストール後、Rust toolchain確認、`cargo build --release --bin core1_etb` を実行する構造にした。

### 6. Release check強化
対象: `RELEASE_CHECK_20260524.ps1`

以下を検査する。

- root `src/` 不存在
- `rust_core/src` 存在
- `generate-select` CLI存在
- 旧P1固定読取・定数の不存在
- Generate本体sheet書込ロジックの不存在
- Apply writerがbase workbook土台であること
- `cargo build --release --bin core1_etb`
- `python -m py_compile web_app/app.py`
- 任意のxlsx指定時は mock translator で Generate E2E smoke

## 未実行事項
この実行環境には cargo が無いため、こちらでは Rust実ビルドは未実行。WindowsまたはRenderで `RELEASE_CHECK_20260524.ps1` / `bash render_build.sh` を実行して確認する。
