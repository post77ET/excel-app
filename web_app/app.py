

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

try:
    # Render本番: `gunicorn web_app.app:app` のようにパッケージ経由で起動される場合
    from web_app.error_messages import t_error
except ImportError:
    # ローカル開発: `web_app/` ディレクトリに cd して `python app.py` のように
    # 直接実行する場合（この場合 sys.path には web_app/ 自体が入るため、
    # パッケージ接頭辞なしの絶対インポートで解決できる）
    from error_messages import t_error

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


def _run_generate_serialized(job_id: str, original_path: str, output_path: Path, extra_env: dict, lang: str = "ja") -> None:
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
            _generate_worker(job_id, original_path, output_path, extra_env, lang)
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


def validate_xlsx(path: Path, lang: str = "ja") -> None:
    if path.suffix.lower() != ".xlsx":
        raise ValueError(t_error("XLSX_ONLY", lang))
    if not path.exists() or path.stat().st_size == 0:
        raise ValueError(t_error("FILE_EMPTY", lang))
    try:
        with zipfile.ZipFile(path, "r") as zf:
            bad = zf.testzip()
            if bad is not None:
                print(f"[VALIDATE_XLSX][ZIP_CORRUPT] path={path} bad_entry={bad}", flush=True)
                raise ValueError(t_error("XLSX_CORRUPTED", lang))
            names = set(zf.namelist())
            required = {"[Content_Types].xml", "xl/workbook.xml", "_rels/.rels"}
            missing = sorted(required - names)
            if missing:
                print(f"[VALIDATE_XLSX][MISSING_PARTS] path={path} missing={missing}", flush=True)
                raise ValueError(t_error("XLSX_INVALID_FORMAT", lang))
            if any(name.startswith("xl/externalLinks/") for name in names):
                raise ValueError(t_error("EXTERNAL_LINK_BLOCKED", lang))
            if any(name.endswith("vbaProject.bin") for name in names):
                raise ValueError(t_error("MACRO_BLOCKED", lang))
    except zipfile.BadZipFile as exc:
        print(f"[VALIDATE_XLSX][BAD_ZIP] path={path} error={repr(exc)}", flush=True)
        raise ValueError(t_error("XLSX_OPEN_FAILED", lang)) from exc


def save_uploaded_file(field_name: str, prefix: str, lang: str = "ja") -> Path:
    file_storage = request.files.get(field_name)
    if file_storage is None or file_storage.filename == "":
        print(f"[SAVE_UPLOADED_FILE][MISSING_FIELD] field_name={field_name}", flush=True)
        raise ValueError(t_error("FILE_NOT_SELECTED", lang))
    # 拡張子は「元の」ファイル名で判定する。secure_filename() は日本語・中国語など
    # 非ASCII文字を除去するため、「売上表.xlsx」のような全角名だと拡張子が消え、
    # 正常な.xlsxでも誤って弾かれていた（全角名ユーザーを直撃）。
    if Path(file_storage.filename).suffix.lower() != ".xlsx":
        raise ValueError(t_error("XLSX_ONLY", lang))
    # アップロード直後にサイズを測り、上限超過なら保存・Rust処理の前に即エラー。
    stream = file_storage.stream
    stream.seek(0, os.SEEK_END)
    size_bytes = stream.tell()
    stream.seek(0)
    if size_bytes > MAX_WORKBOOK_BYTES:
        size_mb = size_bytes / (1024 * 1024)
        limit_mb = MAX_WORKBOOK_BYTES / (1024 * 1024)
        raise ValueError(t_error("FILE_TOO_LARGE", lang, limit_mb=limit_mb, size_mb=size_mb))
    # 保存用の安全名。本体が非ASCIIで消えても拡張子は必ず付与する。
    original_name = secure_filename(file_storage.filename)
    if Path(original_name).suffix.lower() != ".xlsx":
        original_name = f"{prefix}.xlsx"
    safe_name = f"{timestamp()}_{uuid.uuid4().hex[:8]}_{prefix}_{original_name}"
    save_path = UPLOAD_DIR / safe_name
    file_storage.save(save_path)
    validate_xlsx(save_path, lang)
    return save_path


def workbook_sheet_names(path: Path, lang: str = "ja") -> list[str]:
    validate_xlsx(path, lang)
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
        print(f"[WORKBOOK_SHEET_NAMES][NO_SHEETS] path={path}", flush=True)
        raise ValueError(t_error("NO_SHEETS_FOUND", lang))
    return names


def normalize_mode(value: str | None) -> str:
    return "paid" if value == "paid" else "experience"


def parse_selected_sheets(selection: str, sheet_names: list[str], mode: str, lang: str = "ja") -> tuple[str, str]:
    text = (selection or "").strip()
    if not text:
        raise ValueError(t_error("SHEET_SELECTION_EMPTY", lang))
    if mode == "experience":
        if text.lower() == "all" or "," in text:
            raise ValueError(t_error("EXPERIENCE_ONE_SHEET_ONLY", lang))
    if text.lower() == "all":
        return "all", ", ".join(sheet_names)
    selected_names: list[str] = []
    normalized_tokens: list[str] = []
    for raw in text.split(','):
        token = raw.strip()
        if not token:
            raise ValueError(t_error("SHEET_TOKEN_BLANK", lang))
        if token.isdigit():
            idx = int(token)
            if idx < 1 or idx > len(sheet_names):
                raise ValueError(t_error("SHEET_INDEX_OUT_OF_RANGE", lang, idx=idx))
            selected_names.append(sheet_names[idx - 1])
            normalized_tokens.append(str(idx))
        else:
            if token not in sheet_names:
                raise ValueError(t_error("SHEET_NAME_NOT_FOUND", lang, token=token))
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


def print_env_check(rust_env: dict[str, str], command: str = "") -> None:
    print("[ENV CHECK] DEEPL_API_KEY =", "OK" if rust_env.get("DEEPL_API_KEY") else "MISSING", flush=True)
    print("[ENV CHECK] DEEPL_KEY =", "OK" if rust_env.get("DEEPL_KEY") else "MISSING", flush=True)
    print("[ENV CHECK] AWS_ACCESS_KEY_ID =", "OK" if rust_env.get("AWS_ACCESS_KEY_ID") else "MISSING", flush=True)
    print("[ENV CHECK] AWS_SECRET_ACCESS_KEY =", "OK" if rust_env.get("AWS_SECRET_ACCESS_KEY") else "MISSING", flush=True)
    print("[ENV CHECK] AWS_REGION =", rust_env.get("AWS_REGION", "MISSING"), flush=True)
    print("[ENV CHECK] AWS_DEFAULT_REGION =", rust_env.get("AWS_DEFAULT_REGION", "MISSING"), flush=True)
    print("[ENV CHECK] ETB_TRANSLATOR_CONFIG =", rust_env.get("ETB_TRANSLATOR_CONFIG", "MISSING"), flush=True)
    print("[ENV CHECK] ETB_JOB_PLAN_CONFIG =", rust_env.get("ETB_JOB_PLAN_CONFIG", "MISSING"), flush=True)
    print("[ENV CHECK] ETB_SELECTED_SHEETS =", rust_env.get("ETB_SELECTED_SHEETS", "MISSING"), flush=True)

    # ETB_DIRECTION_ID / ETB_BILLING_MODE は generate / estimate でのみ必要。
    # apply など他コマンドでは不要なので MISSING ではなく NOT_REQUIRED_FOR_APPLY を表示する。
    requires_direction = command in ("generate-select", "estimate-select")
    if requires_direction:
        did = rust_env.get("ETB_DIRECTION_ID")
        bmode = rust_env.get("ETB_BILLING_MODE")
        print("[ENV CHECK] ETB_DIRECTION_ID =", ("OK " + did) if did else "MISSING", flush=True)
        print("[ENV CHECK] ETB_BILLING_MODE =", ("OK " + bmode) if bmode else "MISSING", flush=True)
    else:
        print("[ENV CHECK] ETB_DIRECTION_ID = NOT_REQUIRED_FOR_APPLY", flush=True)
        print("[ENV CHECK] ETB_BILLING_MODE = NOT_REQUIRED_FOR_APPLY", flush=True)

    print("[ENV CHECK] ETB_BIN_PATH =", os.environ.get("ETB_BIN_PATH", "MISSING"), flush=True)


class WorkbookParseError(Exception):
    """umya がファイルを読めず内部 panic（WORKBOOK_PARSE_FAILED）した場合の専用例外。"""
    pass


class InvalidCandidateComboError(Exception):
    """provider×method の不正組合せ（DeepL×split 等）を Rust が拒否した場合の専用例外。"""
    pass


# Rust 側が返す安定キー。これを ja/zh の客向け文言に変換する（表示は Web の責務）。
WORKBOOK_PARSE_FAILED_KEY = "WORKBOOK_PARSE_FAILED"
# C-5: provider×method の不正組合せ（DeepL×split）の安定キー。
# 客向け文言は error_messages.py の INVALID_CANDIDATE_COMBO で一元管理。
INVALID_CANDIDATE_COMBO_KEY = "INVALID_CANDIDATE_METHOD_PROVIDER"


def run_rust(args: list[str], extra_env: dict[str, str] | None = None, lang: str = "ja") -> subprocess.CompletedProcess[str]:
    rust_env = build_rust_env(extra_env)
    print_env_check(rust_env, args[0] if args else "")
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
        if result.stderr and WORKBOOK_PARSE_FAILED_KEY in result.stderr:
            raise WorkbookParseError()
        if result.stderr and INVALID_CANDIDATE_COMBO_KEY in result.stderr:
            raise InvalidCandidateComboError()
        raise RuntimeError(t_error("RUST_PROCESSING_FAILED", lang))
    return result


def write_job_plan_config(
    job_id: str,
    mode: str,
    course: str = "full",
    c1_provider: str | None = None,
    c2_provider: str | None = None,
    c3_provider: str | None = None,
    c1_method: str | None = None,
    c2_method: str | None = None,
    c3_method: str | None = None,
    direction: str = "ja2zh",
) -> Path:
    force_mock = os.environ.get("ETB_FORCE_MOCK_TRANSLATORS", "").strip().lower() in {"1", "true", "yes", "y"}
    valid_providers = {"google", "amazon", "deepl", "mock"}
    valid_methods = {"split", "whole"}

    if force_mock:
        candidate1_provider = candidate2_provider = candidate3_provider = "mock"
    else:
        candidate1_provider = c1_provider if c1_provider in valid_providers else os.environ.get("ETB_CANDIDATE1_PROVIDER", "google")
        candidate2_provider = c2_provider if c2_provider in valid_providers else os.environ.get("ETB_CANDIDATE2_PROVIDER", "amazon")
        candidate3_provider = c3_provider if c3_provider in valid_providers else os.environ.get("ETB_CANDIDATE3_PROVIDER", "deepl")

    # C-6 / QA-017要求1: 候補ごとの翻訳方式。未指定/不正は既定補完。
    # 中→日(zh2ja)は分割で誤訳が多いため、未指定時の既定を whole とする。
    # 日→中(ja2zh)の既定は従来どおり 1=split,2=split,3=whole。
    # ja2vi/vi2ja: split/whole の品質差が未検証のため、zh2ja に倣い既定を whole とする
    # （QAで実測後に見直す。無言 fallback ではなく明示的な既定値として扱う）。
    if direction in ("zh2ja", "ja2vi", "vi2ja"):
        default_methods = {"c1": "whole", "c2": "whole", "c3": "whole"}
    else:
        default_methods = {"c1": "split", "c2": "split", "c3": "whole"}
    candidate1_method = c1_method if c1_method in valid_methods else default_methods["c1"]
    candidate2_method = c2_method if c2_method in valid_methods else default_methods["c2"]
    candidate3_method = c3_method if c3_method in valid_methods else default_methods["c3"]

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
        "candidate1_method": candidate1_method,
        "candidate2_method": candidate2_method,
        "candidate3_method": candidate3_method,
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


def newest_created_xlsx(exclude: set[Path], lang: str = "ja") -> Path:
    candidates = []
    for path in iter_xlsx_files():
        resolved = path.resolve()
        if resolved in exclude:
            continue
        if path.exists() and path.stat().st_size > 0:
            candidates.append(path)
    if not candidates:
        print(f"[FIND_OUTPUT][NOT_FOUND] exclude={exclude}", flush=True)
        raise RuntimeError(t_error("GENERATE_OUTPUT_MISSING", lang))
    return max(candidates, key=lambda p: p.stat().st_mtime)


def server_original_path(job_id: str, lang: str = "ja") -> Path:
    safe_job_id = secure_filename(job_id)
    if not re.fullmatch(r"job\d{14}_[0-9a-fA-F]{8}", safe_job_id):
        print(f"[SERVER_ORIGINAL_PATH][INVALID_JOB_ID] job_id={job_id}", flush=True)
        raise ValueError(t_error("INVALID_OPERATION_INFO", lang))
    return SERVER_ORIGINAL_DIR / f"{safe_job_id}_original.xlsx"


def extract_job_id_from_ui_filename(filename: str, lang: str = "ja") -> str:
    # Windowsが付ける重複連番 "(1)" "(2)" や前後の空白を除いてから探す（保険）。
    # 本来 job_id はファイル名の途中に埋め込まれているため re.search で足りるが、
    # 将来の命名変更にも耐えられるよう、明示的に連番・空白を除去しておく。
    normalized = re.sub(r"\s*\(\d+\)(?=\.[^.]+$|$)", "", filename)
    match = re.search(r"(job\d{14}_[0-9a-fA-F]{8})", normalized)
    if not match:
        print(f"[EXTRACT_JOB_ID][NOT_FOUND] filename={filename}", flush=True)
        raise ValueError(t_error("JOB_ID_NOT_FOUND", lang))
    return match.group(1)


def save_server_original_clone(job_id: str, original_path: Path, lang: str = "ja") -> Path:
    clone_path = server_original_path(job_id, lang)
    print("[SERVER ORIGINAL] clone start job_id=", job_id, flush=True)
    src_hash = log_file_check("GENERATE_ORIGINAL_BEFORE_CLONE", original_path)
    shutil.copy2(original_path, clone_path)
    validate_xlsx(clone_path, lang)
    clone_hash = log_file_check("SERVER_ORIGINAL_AFTER_CLONE", clone_path)
    print("[SERVER ORIGINAL] saved =", str(clone_path), flush=True)
    print("[SERVER ORIGINAL] clone_hash_match =", "OK" if src_hash == clone_hash else "MISMATCH", flush=True)
    return clone_path


def load_server_original_for_ui(ui_path: Path, lang: str = "ja") -> Path:
    print("[SERVER ORIGINAL] apply lookup ui_filename=", ui_path.name, flush=True)
    job_id = extract_job_id_from_ui_filename(ui_path.name, lang)
    print("[SERVER ORIGINAL] apply lookup job_id=", job_id, flush=True)
    log_file_check("APPLY_UI_UPLOAD", ui_path)
    original_path = server_original_path(job_id, lang)
    if not original_path.exists():
        print(f"[LOAD_SERVER_ORIGINAL][NOT_FOUND] original_path={original_path}", flush=True)
        raise ValueError(t_error("SERVER_ORIGINAL_MISSING", lang))
    validate_xlsx(original_path, lang)
    print("[SERVER ORIGINAL] loaded =", str(original_path), flush=True)
    log_file_check("APPLY_SERVER_ORIGINAL", original_path)
    return original_path


@app.get("/")
def index():
    return render_template("landing.html", lang=request.args.get("lang", "ja"))


# src(出発言語)から実際に翻訳可能な行き先一覧。rust_core/src/direction/mod.rs の
# 対応方向(ja2zh, zh2ja, ja2vi, vi2ja)と完全一致させること。
SRC_TO_TARGETS = {
    "ja": [("zh", "ja2zh"), ("vi", "ja2vi")],
    "zh": [("ja", "zh2ja")],
    "vi": [("ja", "vi2ja")],
    "en": [],  # 現時点で英語を出発点とする翻訳方向は未対応
}


@app.get("/start")
def start():
    src = request.args.get("src", "ja")
    if src not in SRC_TO_TARGETS:
        src = "ja"
    page_lang = request.args.get("lang", src if src in ("ja", "zh") else "ja")
    return render_template(
        "target_select.html",
        src=src,
        targets=SRC_TO_TARGETS[src],
        lang=page_lang,
    )


@app.get("/googlea52e60130d420841.html")
def google_site_verification():
    # Google Search Console 所有権確認用。確認状態維持のため削除しないこと。
    return ("google-site-verification: googlea52e60130d420841.html", 200,
            {"Content-Type": "text/html; charset=utf-8"})


@app.get("/robots.txt")
def robots_txt():
    body = (
        "User-agent: *\n"
        "Allow: /\n"
        "Sitemap: https://excel-app-t3dn.onrender.com/sitemap.xml\n"
    )
    return (body, 200, {"Content-Type": "text/plain; charset=utf-8"})


@app.get("/sitemap.xml")
def sitemap_xml():
    base = "https://excel-app-t3dn.onrender.com"
    langs = ["ja", "zh", "vi", "en"]
    alt_links = "".join(
        f'      <xhtml:link rel="alternate" hreflang="{l if l != "zh" else "zh-CN"}" href="{base}/?lang={l}"/>\n'
        for l in langs
    )
    lang_urls = "".join(
        f'  <url>\n    <loc>{base}/?lang={l}</loc>\n{alt_links}'
        f'    <changefreq>weekly</changefreq><priority>1.0</priority>\n  </url>\n'
        for l in langs
    )
    body = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" '
        'xmlns:xhtml="http://www.w3.org/1999/xhtml">\n'
        f'{lang_urls}'
        f'  <url><loc>{base}/engine</loc><changefreq>weekly</changefreq><priority>0.8</priority></url>\n'
        f'  <url><loc>{base}/guide/excel-ja-zh-translate</loc><changefreq>monthly</changefreq><priority>0.9</priority></url>\n'
        f'  <url><loc>{base}/guide/manufacturing-excel-translation</loc><changefreq>monthly</changefreq><priority>0.9</priority></url>\n'
        '</urlset>\n'
    )
    return (body, 200, {"Content-Type": "application/xml; charset=utf-8"})


@app.get("/guide/excel-ja-zh-translate")
def guide_excel_ja_zh():
    return render_template("guide_excel_ja_zh.html")


@app.get("/guide/manufacturing-excel-translation")
def guide_manufacturing_spec():
    return render_template("guide_manufacturing_spec.html")


@app.get("/engine")
def engine():
    return render_template(
        "engine.html",
        lang=request.args.get("lang", "ja"),
        direction=request.args.get("direction", "ja2zh"),
    )

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
        direction = request.form.get("direction", "ja2zh")
        original_path = save_uploaded_file("file", "original", lang)
        sheet_names = workbook_sheet_names(original_path, lang)
        return render_template(
            "select.html",
            filename=original_path.name,
            mode=mode,
            sheet_names=sheet_names,
            lang=lang,
            direction=direction,
        )
    except (ValueError, RuntimeError) as exc:
        print("[UPLOAD ERROR]", repr(exc), flush=True)
        abort(500, description=str(exc))
    except Exception as exc:
        print("[UPLOAD ERROR][UNEXPECTED]", repr(exc), flush=True)
        safe_lang = request.form.get("lang", "ja")
        abort(500, description=t_error("UNEXPECTED_ERROR", safe_lang))


@app.post("/generate")
def generate():
    try:
        lang = request.form.get("lang", "ja")
        filename = secure_filename(request.form.get("filename", ""))
        if not filename:
            print("[GENERATE][MISSING_FILENAME]", flush=True)
            raise ValueError(t_error("UPLOAD_FILE_INFO_UNREADABLE", lang))
        original_path = (UPLOAD_DIR / filename).resolve()
        if UPLOAD_DIR.resolve() not in original_path.parents:
            print(f"[GENERATE][INVALID_FILENAME] filename={filename}", flush=True)
            raise ValueError(t_error("UPLOAD_FILE_INFO_UNREADABLE", lang))
        validate_xlsx(original_path, lang)

        mode = normalize_mode(request.form.get("mode"))
        # Phase 4B: 翻訳方向（表示言語 lang とは別物）。許可値以外は黙ってja2zhに落とさずエラー。
        direction = request.form.get("direction", "ja2zh")
        if direction not in ("ja2zh", "zh2ja", "ja2vi", "vi2ja"):
            print(f"[GENERATE][INVALID_DIRECTION] direction={direction!r}", flush=True)
            raise ValueError(t_error("DIRECTION_INVALID", lang))
        sheet_names = workbook_sheet_names(original_path, lang)
        selected_token, selected_label = parse_selected_sheets(request.form.get("sheets", ""), sheet_names, mode, lang)

        job_id = request_id()
        output_path = OUTPUT_DIR / f"{job_id}_{original_path.stem}_ui.xlsx"
        print("[GENERATE FILE CHECK] job_id =", job_id, flush=True)
        print("[GENERATE FILE CHECK] original_filename =", original_path.name, flush=True)
        print("[GENERATE FILE CHECK] output_ui_filename =", output_path.name, flush=True)
        print("[GENERATE FILE CHECK] output_ui_path =", str(output_path), flush=True)
        save_server_original_clone(job_id, original_path, lang)
        course = request.form.get("course", "full")
        c1_provider = request.form.get("c1_provider", None)
        c2_provider = request.form.get("c2_provider", None)
        c3_provider = request.form.get("c3_provider", None)
        c1_method = request.form.get("c1_method", None)
        c2_method = request.form.get("c2_method", None)
        c3_method = request.form.get("c3_method", None)
        plan_path = write_job_plan_config(
            job_id, mode, course,
            c1_provider, c2_provider, c3_provider,
            c1_method, c2_method, c3_method,
            direction,
        )
        # Phase 1: ExecutionPlan の実行条件を Rust へ明示的に渡す。
        # direction_id は Web の方向選択（ja2zh / zh2ja）から受け取り検証済みの値を渡す。billing_mode は既存 mode から導出（experience->free / paid->paid_standard）。
        # これらは Rust 側 ExecutionPlan::from_runtime が env として受け取り resolve に流す。
        billing_mode = "free" if mode == "experience" else "paid_standard"
        extra_env = {
            "ETB_REQUEST_ID": job_id,
            "ETB_SELECTED_SHEETS": selected_token,
            "ETB_JOB_PLAN_CONFIG": str(plan_path),
            "ETB_UI_OUTPUT": str(output_path),
            "ETB_OUTPUT_DIR": str(OUTPUT_DIR),
            "ETB_DIRECTION_ID": direction,
            "ETB_BILLING_MODE": billing_mode,
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
            args=(job_id, str(original_path), output_path, extra_env, lang),
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
    except (ValueError, RuntimeError) as exc:
        print("[GENERATE ERROR]", repr(exc), flush=True)
        abort(500, description=str(exc))
    except Exception as exc:
        print("[GENERATE ERROR][UNEXPECTED]", repr(exc), flush=True)
        safe_lang = request.form.get("lang", "ja")
        abort(500, description=t_error("UNEXPECTED_ERROR", safe_lang))


def _generate_worker(job_id: str, original_path: str, output_path: Path, extra_env: dict, lang: str = "ja") -> None:
    """CL-10: Generateの重い処理をバックグラウンドで実行し、結果を記録する。"""
    try:
        run_rust(["generate-select", original_path], extra_env=extra_env, lang=lang)
        if not output_path.exists() or output_path.stat().st_size <= 0:
            print(f"[GENERATE][OUTPUT_MISSING] output_path={output_path}", flush=True)
            raise RuntimeError(t_error("GENERATE_OUTPUT_MISSING", lang))
        with GENERATE_JOBS_LOCK:
            job = GENERATE_JOBS.get(job_id, {})
            job.update({"status": "done", "filename": output_path.name})
            GENERATE_JOBS[job_id] = job
        print("[GENERATE WORKER] done job_id =", job_id, flush=True)
    except Exception as exc:
        print("[GENERATE WORKER ERROR]", repr(exc), flush=True)
        with GENERATE_JOBS_LOCK:
            job = GENERATE_JOBS.get(job_id, {})
            if isinstance(exc, WorkbookParseError):
                msg = t_error("WORKBOOK_PARSE_FAILED", job.get("lang", "ja"))
            elif isinstance(exc, InvalidCandidateComboError):
                msg = t_error("INVALID_CANDIDATE_COMBO", job.get("lang", "ja"))
            else:
                msg = str(exc)
            job.update({"status": "error", "message": msg})
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
        ui_path = save_uploaded_file("ui_file", "ui", lang)
        job_id = extract_job_id_from_ui_filename(ui_path.name, lang)
        original_path = load_server_original_for_ui(ui_path, lang)
        # 元ファイル名を含めて識別しやすくする
        apply_output_path = (OUTPUT_DIR / f"{job_id}_{original_path.stem}_apply.xlsx").resolve()

        print("[APPLY FILE CHECK] uploaded_ui_filename =", ui_path.name, flush=True)
        print("[APPLY FILE CHECK] uploaded_ui_path =", str(ui_path), flush=True)
        print("[APPLY FILE CHECK] matched_job_id =", job_id, flush=True)
        print("[APPLY FILE CHECK] matched_original_filename =", original_path.name, flush=True)
        print("[APPLY FILE CHECK] matched_original_path =", str(original_path), flush=True)
        print("[APPLY FILE CHECK] apply_output_path =", str(apply_output_path), flush=True)

        if OUTPUT_DIR.resolve() not in apply_output_path.parents:
            print(f"[APPLY][INVALID_OUTPUT_PATH] apply_output_path={apply_output_path}", flush=True)
            raise ValueError(t_error("APPLY_PREPARE_ERROR", lang))

        if apply_output_path.exists():
            apply_output_path.unlink()

        run_rust(
            ["apply", str(ui_path), str(original_path)],
            extra_env={
                "ETB_APPLY_OUTPUT": str(apply_output_path),
                "ETB_OUTPUT_DIR": str(OUTPUT_DIR),
            },
            lang=lang,
        )

        if not apply_output_path.exists() or apply_output_path.stat().st_size <= 0:
            print(f"[APPLY][OUTPUT_MISSING] apply_output_path={apply_output_path}", flush=True)
            raise RuntimeError(t_error("APPLY_NOT_COMPLETED", lang))

        validate_xlsx(apply_output_path, lang)
        download_name = apply_output_path.name

        return render_template(
            "apply_done.html",
            download_url=url_for("download_output", filename=download_name),
            lang=lang if lang in ["ja", "zh"] else "ja",
        )
    except (ValueError, RuntimeError) as exc:
        print("[APPLY ERROR]", repr(exc), flush=True)
        return render_template("apply_error.html", message=str(exc), lang=request.form.get("lang", "ja")), 400
    except Exception as exc:
        print("[APPLY ERROR][UNEXPECTED]", repr(exc), flush=True)
        safe_lang = request.form.get("lang", "ja")
        fallback_msg = t_error("UNEXPECTED_ERROR", safe_lang)
        return render_template("apply_error.html", message=fallback_msg, lang=safe_lang), 400


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
