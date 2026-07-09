param(
    [string]$InputXlsx = ""
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $Root

Write-Host "[CHECK] root = $Root"

if (Test-Path "$Root\src") {
    throw "NG: root src/ exists. Source-of-truth must be rust_core/src only."
}
if (!(Test-Path "$Root\rust_core\src")) {
    throw "NG: rust_core/src missing."
}

$main = Get-Content "$Root\rust_core\src\main.rs" -Raw -Encoding UTF8
if ($main -notmatch '"generate-select"') {
    throw "NG: Rust CLI generate-select mode missing."
}

$p1Hits = Select-String -Path "$Root\rust_core\src\*.rs","$Root\rust_core\src\**\*.rs" -Pattern 'worksheet_range\("P1"\)|MAIN_SHEET_NAME\s*=\s*"P1"' -ErrorAction SilentlyContinue
if ($p1Hits) {
    $p1Hits | ForEach-Object { Write-Host $_ }
    throw "NG: legacy P1 fixed reader/protection constant detected."
}

$genWriter = Get-Content "$Root\rust_core\src\core2\generate_workbook_writer.rs" -Raw -Encoding UTF8
if ($genWriter -match 'set_value_string\(selected_text\)|should_write_in_generate|resolve_initial_text') {
    throw "NG: Generate still appears to write translated values into source sheets."
}

$applyWriter = Get-Content "$Root\rust_core\src\core2\apply_workbook_writer.rs" -Raw -Encoding UTF8
if ($applyWriter -notmatch 'base_workbook_path' -or $applyWriter -notmatch 'base workbook read failed') {
    throw "NG: Apply writer does not clearly use base workbook as writeback base."
}

Write-Host "[CHECK] cargo build --release --bin core1_etb"
Set-Location "$Root\rust_core"
cargo build --release --bin core1_etb

$exe1 = "$Root\rust_core\target\release\core1_etb.exe"
$exe2 = "$Root\rust_core\target\release\core1_etb"
if (!(Test-Path $exe1) -and !(Test-Path $exe2)) {
    throw "NG: Rust release binary was not created."
}

Set-Location $Root
Write-Host "[CHECK] Python syntax"
python -m py_compile "$Root\web_app\app.py"

if ($InputXlsx -ne "") {
    if (!(Test-Path $InputXlsx)) { throw "NG: InputXlsx not found: $InputXlsx" }
    $env:ETB_FORCE_MOCK_TRANSLATORS = "1"
    $env:ETB_SELECTED_SHEETS = "1"
    $env:ETB_UI_OUTPUT = "$Root\output\release_check_ui.xlsx"
    if (Test-Path $env:ETB_UI_OUTPUT) { Remove-Item $env:ETB_UI_OUTPUT -Force }

    $bin = if (Test-Path $exe1) { $exe1 } else { $exe2 }
    Write-Host "[CHECK] Generate E2E smoke using mock translators"
    & $bin generate-select $InputXlsx
    if ($LASTEXITCODE -ne 0) { throw "NG: generate-select failed." }
    if (!(Test-Path $env:ETB_UI_OUTPUT)) { throw "NG: UI output not created." }
    Write-Host "[CHECK] Generate E2E smoke output = $env:ETB_UI_OUTPUT"
}

Write-Host "[RESULT] PASS: release structural checks completed."
