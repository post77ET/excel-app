

from __future__ import annotations

import hashlib
import html
import json
import os
import re
import shutil
import subprocess
import uuid
import threading
import time
import zipfile
import xml.etree.ElementTree as ET
from datetime import datetime
from pathlib import Path
from typing import Iterable

from flask import Flask, abort, request, send_file, render_template, url_for
from werkzeug.utils import secure_filename

# ============================================================
# Excel Translation Web Frontend for Render
#
# Purpose:
#   Web flow must be:
#     upload -> security check -> course/sheet selection -> generate
#   Rust CLI still owns the real pipeline. Web side passes the selected
#   sheets via ETB_SELECTED_SHEETS so Rust never waits for stdin on Render.
# ============================================================

APP_ROOT = Path(__file__).resolve().parent
PROJECT_ROOT = APP_ROOT.parent
UPLOAD_DIR = PROJECT_ROOT / "uploads"
OUTPUT_DIR = PROJECT_ROOT / "output"
WORKING_DIR = PROJECT_ROOT / "working"
SERVER_ORIGINAL_DIR = PROJECT_ROOT / "server_originals"
WORK_DIR = PROJECT_ROOT

UPLOAD_DIR.mkdir(parents=True, exist_ok=True)
OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
WORKING_DIR.mkdir(parents=True, exist_ok=True)
SERVER_ORIGINAL_DIR.mkdir(parents=True, exist_ok=True)

app = Flask(__name__)
app.config["MAX_CONTENT_LENGTH"] = int(os.environ.get("MAX_UPLOAD_MB", "30")) * 1024 * 1024

# 1ファイルあたりの上限。Rustコア(core1_etb)の size_check と一致させること(10MB)。
MAX_WORKBOOK_BYTES = 10 * 1024 * 1024

# ============================================================
# CL-10: 長時間(5〜7分)のGenerate処理中にブラウザが切れても、
#   JOB_ID で結果を再取得できるようにするためのジョブ管理。
#   - Generateはバックグラウンドスレッドで実行し、即座にJOB_IDを返す
#   - /job/<job_id>          : 結果ページ(処理中はポーリング、完了でダウンロード)
#   - /job/<job_id>/status   : 状態JSON
#   - /job/<job_id>/download : JOB_IDでUI出力を直接ダウンロード
# サーバ再起動でメモリ上のジョブ情報が消えても、出力ファイルが残っていれば
# ファイル存在チェックにフォールバックして再取得できる。
# ============================================================
JOB_ID_PATTERN = r"job\d{14}_[0-9a-fA-F]{8}"
GENERATE_JOBS: dict[str, dict] = {}
GENERATE_JOBS_LOCK = threading.Lock()

# 順番待ち（セマフォ直列化）。同時に走るRust処理を GENERATE_WORKERS 件に制限する。
# 各Generateはリクエスト毎にスレッドを起こし（実績ある方式）、関所(セマフォ)を
# 取得できるまで待つ。待っている間が「順番待ち」。常駐ワーカー方式と違い、
# 待機ジョブには必ず生きたスレッドが付くので「消化されず詰まる」問題が起きない。
GENERATE_WORKERS = 1
GENERATE_SEMAPHORE = threading.BoundedSemaphore(GENERATE_WORKERS)
GENERATE_QUEUE_ORDER: list[str] = []  # 待機中 job_id の順序（待ち人数の表示用）


def _run_generate_serialized(job_id: str, original_path: str, output_path: Path, extra_env: dict) -> None:
    """セマフォで直列化したうえでGenerateを実行する（リクエスト毎スレッドで起動）。"""
    try:
        GENERATE_SEMAPHORE.acquire()  # 取得できるまで待機＝順番待ち
        try:
            with GENERATE_JOBS_LOCK:
                if job_id in GENERATE_QUEUE_ORDER:
                    GENERATE_QUEUE_ORDER.remove(job_id)
                job = GENERATE_JOBS.get(job_id, {})
                job["status"] = "running"
                GENERATE_JOBS[job_id] = job
            _generate_worker(job_id, original_path, output_path, extra_env)
        finally:
            GENERATE_SEMAPHORE.release()
    except Exception as exc:  # スレッドは絶対に落とさない
        print("[GENERATE SERIALIZED ERROR]", repr(exc), flush=True)
        with GENERATE_JOBS_LOCK:
            if job_id in GENERATE_QUEUE_ORDER:
                GENERATE_QUEUE_ORDER.remove(job_id)
            job = GENERATE_JOBS.get(job_id, {})
            job.update({"status": "error", "message": str(exc)})
            GENERATE_JOBS[job_id] = job


def is_valid_job_id(job_id: str) -> bool:
    return bool(re.fullmatch(JOB_ID_PATTERN, secure_filename(job_id)))


def find_ui_output_by_job_id(job_id: str) -> Path | None:
    """JOB_IDに対応するGenerate UI出力(*_ui.xlsx)を探す。なければNone。"""
    safe = secure_filename(job_id)
    if not is_valid_job_id(safe):
        return None
    matches = sorted(
        OUTPUT_DIR.glob(f"{safe}_*_ui.xlsx"),
        key=lambda p: p.stat().st_mtime if p.exists() else 0.0,
        reverse=True,
    )
    for path in matches:
        if path.exists() and path.stat().st_size > 0:
            return path
    return None







def timestamp() -> str:
    return datetime.now().strftime("%Y%m%d_%H%M%S")


def request_id() -> str:
    return f"job{datetime.now().strftime('%Y%m%d%H%M%S')}_{uuid.uuid4().hex[:8]}"


def file_sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def log_file_check(label: str, path: Path) -> str:
    try:
        resolved = path.resolve()
        exists = resolved.exists()
        size = resolved.stat().st_size if exists else -1
        digest = file_sha256(resolved) if exists else "MISSING"
        print(f"[FILE CHECK] {label} path={resolved} exists={exists} size={size} sha256={digest}", flush=True)
        return digest
    except Exception as exc:
        print(f"[FILE CHECK] {label} ERROR path={path} error={repr(exc)}", flush=True)
        return "ERROR"


def validate_xlsx(path: Path) -> None:
    if path.suffix.lower() != ".xlsx":
        raise ValueError(".xlsx ファイルのみ対応です。")
    if not path.exists() or path.stat().st_size == 0:
        raise ValueError("ファイルが空、または存在しません。")
    try:
        with zipfile.ZipFile(path, "r") as zf:
            bad = zf.testzip()
            if bad is not None:
                raise ValueError(f"xlsx ZIP破損: {bad}")
            names = set(zf.namelist())
            required = {"[Content_Types].xml", "xl/workbook.xml", "_rels/.rels"}
            missing = sorted(required - names)
            if missing:
                raise ValueError("xlsx必須構成が不足: " + ", ".join(missing))
            if any(name.startswith("xl/externalLinks/") for name in names):
                raise ValueError("外部リンク付きxlsxは安全上ブロックします。")
            if any(name.endswith("vbaProject.bin") for name in names):
                raise ValueError("マクロ付きファイルは安全上ブロックします。")
    except zipfile.BadZipFile as exc:
        raise ValueError("xlsxとして開けません。") from exc


def save_uploaded_file(field_name: str, prefix: str) -> Path:
    file_storage = request.files.get(field_name)
    if file_storage is None or file_storage.filename == "":
        raise ValueError(f"アップロードファイルがありません: {field_name}")
    # 拡張子は「元の」ファイル名で判定する。secure_filename() は日本語・中国語など
    # 非ASCII文字を除去するため、「売上表.xlsx」のような全角名だと拡張子が消え、
    # 正常な.xlsxでも誤って弾かれていた（全角名ユーザーを直撃）。
    if Path(file_storage.filename).suffix.lower() != ".xlsx":
        raise ValueError(".xlsx ファイルのみ対応です。")
    # アップロード直後にサイズを測り、上限超過なら保存・Rust処理の前に即エラー。
    stream = file_storage.stream
    stream.seek(0, os.SEEK_END)
    size_bytes = stream.tell()
    stream.seek(0)
    if size_bytes > MAX_WORKBOOK_BYTES:
        size_mb = size_bytes / (1024 * 1024)
        limit_mb = MAX_WORKBOOK_BYTES / (1024 * 1024)
        raise ValueError(
            f"ファイルが大きすぎます。上限は {limit_mb:.0f}MB ですが、"
            f"このファイルは約 {size_mb:.1f}MB あります。"
            f"不要なシートや画像を減らして {limit_mb:.0f}MB 以下にしてから、もう一度お試しください。 / "
            f"文件过大。上限为 {limit_mb:.0f}MB，当前文件约 {size_mb:.1f}MB。"
            f"请删除不需要的工作表或图片，缩小至 {limit_mb:.0f}MB 以下后重试。"
        )
    # 保存用の安全名。本体が非ASCIIで消えても拡張子は必ず付与する。
    original_name = secure_filename(file_storage.filename)
    if Path(original_name).suffix.lower() != ".xlsx":
        original_name = f"{prefix}.xlsx"
    safe_name = f"{timestamp()}_{uuid.uuid4().hex[:8]}_{prefix}_{original_name}"
    save_path = UPLOAD_DIR / safe_name
    file_storage.save(save_path)
    validate_xlsx(save_path)
    return save_path


def workbook_sheet_names(path: Path) -> list[str]:
    validate_xlsx(path)
    with zipfile.ZipFile(path, "r") as zf:
        workbook_xml = zf.read("xl/workbook.xml")
    root = ET.fromstring(workbook_xml)
    ns = {"main": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
    names = []
    for sheet in root.findall(".//main:sheets/main:sheet", ns):
        name = sheet.attrib.get("name")
        if name:
            names.append(name)
    if not names:
        raise ValueError("シートが見つかりません。")
    return names


def normalize_mode(value: str | None) -> str:
    return "paid" if value == "paid" else "experience"


def parse_selected_sheets(selection: str, sheet_names: list[str], mode: str) -> tuple[str, str]:
    text = (selection or "").strip()
    if not text:
        raise ValueError("翻訳対象シートが未入力です。")
    if mode == "experience":
        if text.lower() == "all" or "," in text:
            raise ValueError("体験コースは1シートのみ指定してください。")
    if text.lower() == "all":
        return "all", ", ".join(sheet_names)
    selected_names: list[str] = []
    normalized_tokens: list[str] = []
    for raw in text.split(','):
        token = raw.strip()
        if not token:
            raise ValueError("シート指定に空欄があります。")
        if token.isdigit():
            idx = int(token)
            if idx < 1 or idx > len(sheet_names):
                raise ValueError(f"シート番号が範囲外です: {idx}")
            selected_names.append(sheet_names[idx - 1])
            normalized_tokens.append(str(idx))
        else:
            if token not in sheet_names:
                raise ValueError(f"存在しないシート名です: {token}")
            selected_names.append(token)
            normalized_tokens.append(token)
    return ",".join(normalized_tokens), ", ".join(selected_names)


def rust_binary_path() -> str:
    env_path = os.environ.get("ETB_BIN_PATH")
    if env_path:
        path = Path(env_path)
        if not path.exists():
            raise FileNotFoundError(f"ETB_BIN_PATH points to a missing Rust binary: {env_path}")
        return str(path)
    candidates = [
        PROJECT_ROOT / "rust_core" / "target" / "release" / "core1_etb.exe",
        PROJECT_ROOT / "rust_core" / "target" / "release" / "core1_etb",
        PROJECT_ROOT / "rust_core" / "target" / "debug" / "core1_etb.exe",
        PROJECT_ROOT / "rust_core" / "target" / "debug" / "core1_etb",
    ]
    for candidate in candidates:
        if candidate.exists():
            return str(candidate)
    raise FileNotFoundError(
        "Rust binary not found. Build first: cd rust_core; cargo build --release"
    )


def build_rust_env(extra_env: dict[str, str] | None = None) -> dict[str, str]:
    rust_env = os.environ.copy()
    if rust_env.get("DEEPL_API_KEY") and not rust_env.get("DEEPL_KEY"):
        rust_env["DEEPL_KEY"] = rust_env["DEEPL_API_KEY"]
    if rust_env.get("DEEPL_KEY") and not rust_env.get("DEEPL_API_KEY"):
        rust_env["DEEPL_API_KEY"] = rust_env["DEEPL_KEY"]
    rust_env.setdefault("AWS_REGION", "ap-northeast-1")
    rust_env.setdefault("AWS_DEFAULT_REGION", rust_env.get("AWS_REGION", "ap-northeast-1"))
    config_path = PROJECT_ROOT / "config" / "translator_config.json"
    if config_path.exists() and not rust_env.get("ETB_TRANSLATOR_CONFIG"):
        rust_env["ETB_TRANSLATOR_CONFIG"] = str(config_path)
    rust_env.setdefault("RUST_BACKTRACE", "1")
    if extra_env:
        rust_env.update({k: str(v) for k, v in extra_env.items()})
    return rust_env


def print_env_check(rust_env: dict[str, str]) -> None:
    print("[ENV CHECK] DEEPL_API_KEY =", "OK" if rust_env.get("DEEPL_API_KEY") else "MISSING", flush=True)
    print("[ENV CHECK] DEEPL_KEY =", "OK" if rust_env.get("DEEPL_KEY") else "MISSING", flush=True)
    print("[ENV CHECK] AWS_ACCESS_KEY_ID =", "OK" if rust_env.get("AWS_ACCESS_KEY_ID") else "MISSING", flush=True)
    print("[ENV CHECK] AWS_SECRET_ACCESS_KEY =", "OK" if rust_env.get("AWS_SECRET_ACCESS_KEY") else "MISSING", flush=True)
    print("[ENV CHECK] AWS_REGION =", rust_env.get("AWS_REGION", "MISSING"), flush=True)
    print("[ENV CHECK] AWS_DEFAULT_REGION =", rust_env.get("AWS_DEFAULT_REGION", "MISSING"), flush=True)
    print("[ENV CHECK] ETB_TRANSLATOR_CONFIG =", rust_env.get("ETB_TRANSLATOR_CONFIG", "MISSING"), flush=True)
    print("[ENV CHECK] ETB_JOB_PLAN_CONFIG =", rust_env.get("ETB_JOB_PLAN_CONFIG", "MISSING"), flush=True)
    print("[ENV CHECK] ETB_SELECTED_SHEETS =", rust_env.get("ETB_SELECTED_SHEETS", "MISSING"), flush=True)
    print("[ENV CHECK] ETB_BIN_PATH =", os.environ.get("ETB_BIN_PATH", "MISSING"), flush=True)


def run_rust(args: list[str], extra_env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    rust_env = build_rust_env(extra_env)
    print_env_check(rust_env)
    cmd = [rust_binary_path(), *args]
    print("[RUST CMD]", " ".join(str(x) for x in cmd), flush=True)
    print("[RUST CWD]", str(WORK_DIR), flush=True)
    result = subprocess.run(
        cmd,
        cwd=str(WORK_DIR),
        env=rust_env,

        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=int(os.environ.get("RUST_TIMEOUT_SEC", "900")),
    )
    print("[RUST RETURN_CODE]", result.returncode, flush=True)
    if result.stdout:
        print("[RUST STDOUT]\n" + result.stdout, flush=True)
    if result.stderr:
        print("[RUST STDERR]\n" + result.stderr, flush=True)
    if result.returncode != 0:
        raise RuntimeError("Rust処理に失敗しました。Renderログの [RUST STDERR] を確認してください。")
    return result


def write_job_plan_config(
    job_id: str,
    mode: str,
    course: str = "full",
    c1_provider: str | None = None,
    c2_provider: str | None = None,
    c3_provider: str | None = None,
) -> Path:
    force_mock = os.environ.get("ETB_FORCE_MOCK_TRANSLATORS", "").strip().lower() in {"1", "true", "yes", "y"}
    valid_providers = {"google", "amazon", "deepl", "mock"}

    if force_mock:
        candidate1_provider = candidate2_provider = candidate3_provider = "mock"
    else:
        candidate1_provider = c1_provider if c1_provider in valid_providers else os.environ.get("ETB_CANDIDATE1_PROVIDER", "google")
        candidate2_provider = c2_provider if c2_provider in valid_providers else os.environ.get("ETB_CANDIDATE2_PROVIDER", "amazon")
        candidate3_provider = c3_provider if c3_provider in valid_providers else os.environ.get("ETB_CANDIDATE3_PROVIDER", "deepl")

    # コースに応じてenabled_candidatesを決定
    if course == "c1only":
        enabled_candidates = [1]
    elif course == "c1c3":
        enabled_candidates = [1, 3]
    else:
        enabled_candidates = [1, 2, 3]

    plan = {
        "mode": mode,
        "plan_name": "WEB_EXPERIENCE" if mode == "experience" else "WEB_PAID",
        "enabled_candidates": enabled_candidates,
        "candidate1_provider": candidate1_provider,
        "candidate2_provider": candidate2_provider,
        "candidate3_provider": candidate3_provider,
        "default_candidate_priority": [1, 2, 3],
        "job_accept_threshold": 0.8,
        "experience_range": "A1:D5",
    }
    path = WORKING_DIR / f"{job_id}_job_plan_settings.json"
    path.write_text(json.dumps(plan, ensure_ascii=False, indent=2), encoding="utf-8")
    return path


def iter_xlsx_files() -> Iterable[Path]:
    for directory in [OUTPUT_DIR, PROJECT_ROOT / "outputs", UPLOAD_DIR, WORKING_DIR]:
        if directory.exists():
            yield from directory.rglob("*.xlsx")


def newest_created_xlsx(exclude: set[Path]) -> Path:
    candidates = []
    for path in iter_xlsx_files():
        resolved = path.resolve()
        if resolved in exclude:
            continue
        if path.exists() and path.stat().st_size > 0:
            candidates.append(path)
    if not candidates:
        raise RuntimeError("Rust処理後の出力xlsxが見つかりません。")
    return max(candidates, key=lambda p: p.stat().st_mtime)


def server_original_path(job_id: str) -> Path:
    safe_job_id = secure_filename(job_id)
    if not re.fullmatch(r"job\d{14}_[0-9a-fA-F]{8}", safe_job_id):
        raise ValueError(f"不正なJOB_IDです: {job_id}")
    return SERVER_ORIGINAL_DIR / f"{safe_job_id}_original.xlsx"


def extract_job_id_from_ui_filename(filename: str) -> str:
    match = re.search(r"(job\d{14}_[0-9a-fA-F]{8})", filename)
    if not match:
        raise ValueError("UIファイル名からJOB_IDを取得できません。UIファイル名を変更している可能性があります。ダウンロード時の元のファイル名に戻してください。元の名前が分からない場合は、最初からGenerateをやり直してください。")
    return match.group(1)


def save_server_original_clone(job_id: str, original_path: Path) -> Path:
    clone_path = server_original_path(job_id)
    print("[SERVER ORIGINAL] clone start job_id=", job_id, flush=True)
    src_hash = log_file_check("GENERATE_ORIGINAL_BEFORE_CLONE", original_path)
    shutil.copy2(original_path, clone_path)
    validate_xlsx(clone_path)
    clone_hash = log_file_check("SERVER_ORIGINAL_AFTER_CLONE", clone_path)
    print("[SERVER ORIGINAL] saved =", str(clone_path), flush=True)
    print("[SERVER ORIGINAL] clone_hash_match =", "OK" if src_hash == clone_hash else "MISMATCH", flush=True)
    return clone_path


def load_server_original_for_ui(ui_path: Path) -> Path:
    print("[SERVER ORIGINAL] apply lookup ui_filename=", ui_path.name, flush=True)
    job_id = extract_job_id_from_ui_filename(ui_path.name)
    print("[SERVER ORIGINAL] apply lookup job_id=", job_id, flush=True)
    log_file_check("APPLY_UI_UPLOAD", ui_path)
    original_path = server_original_path(job_id)
    if not original_path.exists():
        raise ValueError(f"対応するサーバ保存原本が見つかりません: {original_path.name}。UIファイルが迷子、またはサーバ側の一時保存が失われています。最初からGenerateをやり直してください。Apply前なので無料でやり直しできます。")
    validate_xlsx(original_path)
    print("[SERVER ORIGINAL] loaded =", str(original_path), flush=True)
    log_file_check("APPLY_SERVER_ORIGINAL", original_path)
    return original_path


@app.get("/")
def index():
    return render_template("landing.html", lang=request.args.get("lang", "ja"))


@app.get("/googlea52e60130d420841.html")
def google_site_verification():
    # Google Search Console 所有権確認用。確認状態維持のため削除しないこと。
    return ("google-site-verification: googlea52e60130d420841.html", 200,
            {"Content-Type": "text/html; charset=utf-8"})


@app.get("/engine")
def engine():
    return render_template("engine.html", lang=request.args.get("lang", "ja"))

@app.get("/health")
def health():
    rust_env = build_rust_env()
    lines = [
        "OK",
        f"PROJECT_ROOT={PROJECT_ROOT}",
        f"RUST_BIN={rust_binary_path()}",
        f"DEEPL_API_KEY={'OK' if rust_env.get('DEEPL_API_KEY') else 'MISSING'}",
        f"DEEPL_KEY={'OK' if rust_env.get('DEEPL_KEY') else 'MISSING'}",
        f"AWS_ACCESS_KEY_ID={'OK' if rust_env.get('AWS_ACCESS_KEY_ID') else 'MISSING'}",
        f"AWS_SECRET_ACCESS_KEY={'OK' if rust_env.get('AWS_SECRET_ACCESS_KEY') else 'MISSING'}",
        f"AWS_REGION={rust_env.get('AWS_REGION', 'MISSING')}",
        f"AWS_DEFAULT_REGION={rust_env.get('AWS_DEFAULT_REGION', 'MISSING')}",
        f"ETB_TRANSLATOR_CONFIG={rust_env.get('ETB_TRANSLATOR_CONFIG', 'MISSING')}",
    ]
    return "\n".join(lines), 200, {"Content-Type": "text/plain; charset=utf-8"}




@app.errorhandler(500)
def internal_error(error):
    message = getattr(error, "description", str(error))
    return render_template("error.html", message=message), 500


@app.errorhandler(413)
def too_large(error):
    limit_mb = MAX_WORKBOOK_BYTES / (1024 * 1024)
    message = (
        f"ファイルが大きすぎます。上限は {limit_mb:.0f}MB です。 / "
        f"文件过大。上限为 {limit_mb:.0f}MB，请缩小后重试。"
    )
    return render_template("error.html", message=message), 413


@app.post("/upload")
def upload():
    try:
        mode = normalize_mode(request.form.get("mode"))
        lang = request.form.get("lang", "ja")
        original_path = save_uploaded_file("file", "original")
        sheet_names = workbook_sheet_names(original_path)
        return render_template(
            "select.html",
            filename=original_path.name,
            mode=mode,
            sheet_names=sheet_names,
            lang=lang,
        )
    except Exception as exc:
        print("[UPLOAD ERROR]", repr(exc), flush=True)
        abort(500, description=str(exc))


@app.post("/generate")
def generate():
    try:
        filename = secure_filename(request.form.get("filename", ""))
        if not filename:
            raise ValueError("filename がありません。")
        original_path = (UPLOAD_DIR / filename).resolve()
        if UPLOAD_DIR.resolve() not in original_path.parents:
            raise ValueError("不正なfilenameです。")
        validate_xlsx(original_path)

        mode = normalize_mode(request.form.get("mode"))
        lang = request.form.get("lang", "ja")
        sheet_names = workbook_sheet_names(original_path)
        selected_token, selected_label = parse_selected_sheets(request.form.get("sheets", ""), sheet_names, mode)

        job_id = request_id()
        output_path = OUTPUT_DIR / f"{job_id}_{original_path.stem}_ui.xlsx"
        print("[GENERATE FILE CHECK] job_id =", job_id, flush=True)
        print("[GENERATE FILE CHECK] original_filename =", original_path.name, flush=True)
        print("[GENERATE FILE CHECK] output_ui_filename =", output_path.name, flush=True)
        print("[GENERATE FILE CHECK] output_ui_path =", str(output_path), flush=True)
        save_server_original_clone(job_id, original_path)
        course = request.form.get("course", "full")
        c1_provider = request.form.get("c1_provider", None)
        c2_provider = request.form.get("c2_provider", None)
        c3_provider = request.form.get("c3_provider", None)
        plan_path = write_job_plan_config(job_id, mode, course, c1_provider, c2_provider, c3_provider)
        extra_env = {
            "ETB_REQUEST_ID": job_id,
            "ETB_SELECTED_SHEETS": selected_token,
            "ETB_JOB_PLAN_CONFIG": str(plan_path),
            "ETB_UI_OUTPUT": str(output_path),
            "ETB_OUTPUT_DIR": str(OUTPUT_DIR),
        }

        # 順番待ち: queuedで登録し、リクエスト毎スレッドを起こしてセマフォで直列化。
        # ブラウザが切れてもJOB_IDで結果を再取得できる。
        with GENERATE_JOBS_LOCK:
            GENERATE_JOBS[job_id] = {
                "status": "queued",
                "filename": output_path.name,
                "label": selected_label,
                "mode": mode,
                "lang": lang,
                "message": "",
            }
            GENERATE_QUEUE_ORDER.append(job_id)

        worker = threading.Thread(
            target=_run_generate_serialized,
            args=(job_id, str(original_path), output_path, extra_env),
            daemon=True,
        )
        worker.start()

        # 処理中ページを即返す（自動ポーリングで完了後ダウンロードへ遷移）
        return render_template(
            "processing.html",
            job_id=job_id,
            lang=lang,
            result_url=url_for("job_result", job_id=job_id, lang=lang),
            status_url=url_for("job_status", job_id=job_id),
        )
    except Exception as exc:
        print("[GENERATE ERROR]", repr(exc), flush=True)
        abort(500, description=str(exc))


def _generate_worker(job_id: str, original_path: str, output_path: Path, extra_env: dict) -> None:
    """CL-10: Generateの重い処理をバックグラウンドで実行し、結果を記録する。"""
    try:
        run_rust(["generate-select", original_path], extra_env=extra_env)
        if not output_path.exists() or output_path.stat().st_size <= 0:
            raise RuntimeError(
                f"Generate出力ファイルが見つかりません: {output_path.name}\n"
                "Rustログの [RUST STDERR] を確認してください。"
            )
        with GENERATE_JOBS_LOCK:
            job = GENERATE_JOBS.get(job_id, {})
            job.update({"status": "done", "filename": output_path.name})
            GENERATE_JOBS[job_id] = job
        print("[GENERATE WORKER] done job_id =", job_id, flush=True)
    except Exception as exc:
        print("[GENERATE WORKER ERROR]", repr(exc), flush=True)
        with GENERATE_JOBS_LOCK:
            job = GENERATE_JOBS.get(job_id, {})
            job.update({"status": "error", "message": str(exc)})
            GENERATE_JOBS[job_id] = job


@app.get("/download/<path:filename>")
def download_output(filename: str):
    safe = secure_filename(filename)
    path = (OUTPUT_DIR / safe).resolve()
    if OUTPUT_DIR.resolve() not in path.parents or not path.exists():
        abort(404)
    return send_file(path, as_attachment=True, download_name=path.name)


@app.get("/job/<job_id>/status")
def job_status(job_id: str):
    """CL-10: ジョブ状態をJSONで返す。画面ポーリング用。"""
    if not is_valid_job_id(job_id):
        return {"status": "invalid"}, 400
    safe = secure_filename(job_id)

    with GENERATE_JOBS_LOCK:
        job = dict(GENERATE_JOBS.get(safe, {}))

    # メモリに無い（サーバ再起動など）場合は出力ファイル存在で判断する。
    if not job:
        if find_ui_output_by_job_id(safe) is not None:
            return {"status": "done", "download_url": url_for("download_job", job_id=safe)}
        return {"status": "unknown"}

    status = job.get("status", "running")
    resp = {"status": status}
    if status == "done":
        resp["download_url"] = url_for("download_job", job_id=safe)
    elif status == "error":
        resp["message"] = job.get("message", "")
    elif status == "queued":
        with GENERATE_JOBS_LOCK:
            order = list(GENERATE_QUEUE_ORDER)
            live = [j for j in order if GENERATE_JOBS.get(j, {}).get("status") == "queued"]
        resp["queue_position"] = (live.index(safe) + 1) if safe in live else 1
        resp["queue_total"] = len(live)
    return resp


@app.get("/job/<job_id>")
def job_result(job_id: str):
    """CL-10: 結果ページ。処理中はポーリング、完了でダウンロードページを表示。"""
    if not is_valid_job_id(job_id):
        abort(404)
    safe = secure_filename(job_id)

    with GENERATE_JOBS_LOCK:
        job = dict(GENERATE_JOBS.get(safe, {}))

    lang = request.args.get("lang", job.get("lang", "ja"))
    status = job.get("status")
    output = find_ui_output_by_job_id(safe)

    if status == "done" or output is not None:
        if output is None:
            abort(404)
        return render_template(
            "done.html",
            mode=job.get("mode", ""),
            selected_sheets=job.get("label", ""),
            download_url=url_for("download_job", job_id=safe),
            lang=lang,
        )

    if status == "error":
        return (
            render_template(
                "error.html",
                message=job.get("message", "生成処理中にエラーが発生しました。"),
            ),
            500,
        )

    # 処理中（またはメモリに無いがファイル未生成）→ ポーリングページ
    return render_template(
        "processing.html",
        job_id=safe,
        lang=lang,
        result_url=url_for("job_result", job_id=safe, lang=lang),
        status_url=url_for("job_status", job_id=safe),
    )


@app.get("/job/<job_id>/download")
def download_job(job_id: str):
    """CL-10: JOB_IDでGenerate UI出力を直接ダウンロードする。"""
    if not is_valid_job_id(job_id):
        abort(404)
    output = find_ui_output_by_job_id(job_id)
    if output is None:
        abort(404)
    return send_file(output, as_attachment=True, download_name=output.name)


@app.post("/apply")
def apply():
    try:
        lang = request.form.get("lang", "ja")
        ui_path = save_uploaded_file("ui_file", "ui")
        job_id = extract_job_id_from_ui_filename(ui_path.name)
        original_path = load_server_original_for_ui(ui_path)
        # 元ファイル名を含めて識別しやすくする
        apply_output_path = (OUTPUT_DIR / f"{job_id}_{original_path.stem}_apply.xlsx").resolve()

        print("[APPLY FILE CHECK] uploaded_ui_filename =", ui_path.name, flush=True)
        print("[APPLY FILE CHECK] uploaded_ui_path =", str(ui_path), flush=True)
        print("[APPLY FILE CHECK] matched_job_id =", job_id, flush=True)
        print("[APPLY FILE CHECK] matched_original_filename =", original_path.name, flush=True)
        print("[APPLY FILE CHECK] matched_original_path =", str(original_path), flush=True)
        print("[APPLY FILE CHECK] apply_output_path =", str(apply_output_path), flush=True)

        if OUTPUT_DIR.resolve() not in apply_output_path.parents:
            raise ValueError("不正なApply出力先です。")

        if apply_output_path.exists():
            apply_output_path.unlink()

        run_rust(
            ["apply", str(ui_path), str(original_path)],
            extra_env={
                "ETB_APPLY_OUTPUT": str(apply_output_path),
                "ETB_OUTPUT_DIR": str(OUTPUT_DIR),
            },
        )

        if not apply_output_path.exists() or apply_output_path.stat().st_size <= 0:
            raise RuntimeError(f"Apply指定出力ファイルが生成されていません: {apply_output_path}")

        validate_xlsx(apply_output_path)
        download_name = apply_output_path.name

        return render_template(
            "apply_done.html",
            download_url=url_for("download_output", filename=download_name),
            lang=lang if lang in ["ja", "zh"] else "ja",
        )
    except Exception as exc:
        print("[APPLY ERROR]", repr(exc), flush=True)
        return render_template("apply_error.html", message=str(exc), lang=request.form.get("lang", "ja")), 400


def self_ping():
    time.sleep(30)  # 起動直後は少し待つ
    while True:
        try:
            import urllib.request
            port = int(os.environ.get("PORT", "5000"))
            urllib.request.urlopen(f"http://localhost:{port}/health", timeout=5)
        except Exception:
            pass
        time.sleep(50)  # 50秒ごとにping（1分以内）

_ping_thread = threading.Thread(target=self_ping, daemon=True)
_ping_thread.start()


if __name__ == "__main__":
    port = int(os.environ.get("PORT", "5000"))
    app.run(host="0.0.0.0", port=port, debug=False)
