# APPLY_OUTPUT_CONTRACT_FIX_20260524

## 位置付け

このZIPはフォルダー単位で全置き換え用。

## 修正対象

web_app/app.py の Apply 出力契約。

## 修正内容

- Apply時に UI ファイル名から `job_id` を抽出する。
- `server_originals/{job_id}_original.xlsx` を既存ロジックで照合する。
- Apply出力先を Flask 側で明示的に決定する。
- 出力先を `output/{job_id}_apply.xlsx` に固定する。
- Rust実行時に `ETB_APPLY_OUTPUT=output/{job_id}_apply.xlsx` を渡す。
- Flaskは `ETB_APPLY_OUTPUT` で指定したファイルだけを確認し、ダウンロード対象にする。
- `newest_created_xlsx()` fallback による別xlsx探索を Apply 経路から除去する。
- `server_originals/` をApply出力探索対象にしない。

## Apply契約

```text
UI upload
↓
job_id抽出
↓
server_originals/{job_id}_original.xlsx 照合
↓
ETB_APPLY_OUTPUT=output/{job_id}_apply.xlsx 指定
↓
Rust apply <ui.xlsx> <server_original.xlsx>
↓
Flaskは output/{job_id}_apply.xlsx のみ確認
↓
download
```

## Render設定

変更が必要なのは Build Command のみ。

```text
Build Command
bash render_build.sh
```

現状維持：

```text
Root Directory
空欄
```

```text
Start Command
gunicorn --timeout 900 web_app.app:app
```
