# APPLY FILE MATCH REPAIR 2026-05-24

## 位置付け

このZIPはフォルダー単位で全置き換え用です。

## 修正対象

Apply処理のうち、ファイル照合・引数構成・出力成果物決定を旧安定方式と同等に戻しました。

## 戻した契約

旧安定方式:

```text
ui_path = save_uploaded_file("ui_file", "ui")
original_path = load_server_original_for_ui(ui_path)
before = 現在存在するxlsx集合
run_rust(["apply", str(ui_path), str(original_path)])
Rust実行後に新規生成xlsxを探索して output_path 決定
```

今回、この方式に戻しました。

## 変更しないことを明確化した部分

以下は旧方式を維持し、構造変更対象にしません。

```text
extract_job_id_from_ui_filename()
server_original_path()
load_server_original_for_ui()
APPLY_UI_UPLOAD ログ
APPLY_SERVER_ORIGINAL ログ
run_rust(["apply", ui_path, original_path]) の2引数契約
```

## 除去した破壊要因

```text
ETB_APPLY_OUTPUT を Flask側で事前指定する方式
job_id から apply_output_path を先決めする方式
TRANSLATION_UI sheet が最終Apply出力に必ず存在する前提
```

## Render設定

変更が必要なのは Build Command のみです。

```text
Build Command
bash render_build.sh
```

以下は現状維持です。

```text
Root Directory
空欄

Start Command
gunicorn --timeout 900 web_app.app:app
```

## 検証限界

この環境には cargo が無いため、こちらでは cargo build 実行は未実施です。
ただし、今回修正は既にユーザー側でコンパイル成功した buildfix 版を土台にし、Applyファイル照合部分と保護対象の存在判定のみ修正しています。
