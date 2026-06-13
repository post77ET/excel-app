# excel-app

Excel翻訳作業支援ツール。

## 固定契約

- Generate: 確認用 UI workbook を生成する。元sheetへ翻訳結果を書き込まない。
- Apply: サーバ保存原本 base workbook を土台に、UI workbook の選択結果だけを反映する。

## Local build

```powershell
cd C:\Users\USER\Desktop\excel-app\rust_core
cargo build --release --bin core1_etb
```

## Local run

```powershell
cd C:\Users\USER\Desktop\excel-app
$env:ETB_BIN_PATH="C:\Users\USER\Desktop\excel-app\rust_core\target\release\core1_etb.exe"
cd web_app
python app.py
```

## Release check

```powershell
cd C:\Users\USER\Desktop\excel-app
.\RELEASE_CHECK_20260524.ps1
```

With Generate smoke test:

```powershell
cd C:\Users\USER\Desktop\excel-app
.\RELEASE_CHECK_20260524.ps1 -InputXlsx "C:\Users\USER\Desktop\短縮版xlsx.xlsx"
```

## Render settings

- Root Directory: blank
- Build Command: `bash render_build.sh`
- Start Command: `gunicorn --timeout 900 web_app.app:app`
- Auto-Deploy: Off
