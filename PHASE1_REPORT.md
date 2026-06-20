# Phase 1 実施報告 & 完了確認

対象: `rust_core`（Phase 1: 実行条件 ExecutionPlan の導入と direction/plan resolve の接続）
方針: 恒等マッピング（現行 ja2zh + 現行モードと完全に同一挙動）。証跡として resolve 通過ログを残す。
注記: 本作業環境には Rust ツールチェーンが無いため、**ビルド・実行・ゴールデン比較は実環境で行う**。本書はそのための手順と、結果を記録する雛形を含む。

---

## 0. 改訂履歴（レビュー反映）

GPT審査の4点を検討し、以下のとおり反映した。

| 指摘 | 対応 | 内容 |
|---|---|---|
| 1. Web側が未変更 | **対応** | `web_app/app.py` の generate で `ETB_DIRECTION_ID="ja2zh"` と `ETB_BILLING_MODE`（mode から導出）を Rust へ渡すように変更。env チェックログにも追加。 |
| 2. cell_scope の値を未使用 | **対応** | `matches!` 判定をやめ、`cell_scope.contains(col, row)` で `Range{max_row,max_col}` の値を使って範囲判定するように変更（generate / estimate 両方）。 |
| 3. 未知 id をフォールバック | **対応** | `direction::resolve` / `plan::resolve` を `Result` 化し、未知の明示値は **エラー（fail-fast）**。既定値の適用は ExecutionPlan 側（未指定時のみ ja2zh / mode 由来）に限定し、現行挙動は不変。 |
| 4. 実環境で確認 | **据え置き** | 本環境では cargo 不在のため未実施。お手元での build / resolve ログ / golden PASS を完了条件として維持。 |

いずれも正常系（ja2zh・experience/paid・env 未設定時の既定）は現行挙動と同一。変更は「不正な明示値の拒否」と「値の実使用」に限られる。

---

## 1. 変更内容

### 新規ファイル（追加のみ）
```
rust_core/src/planning/mod.rs        ExecutionPlan（direction_id / billing_mode を保持・確定）
rust_core/src/direction/mod.rs       trait DirectionProfile + resolve()（証跡ログ付き）
rust_core/src/direction/ja_zh.rs     JaZhProfile -> (Lang::Ja, Lang::Zh)
rust_core/src/plan/mod.rs            trait PlanPolicy + CellScope + resolve()（証跡ログ付き）
rust_core/src/plan/free_plan.rs      FreePlan -> CellScope::Range(A1:D5)
rust_core/src/plan/paid_standard.rs  PaidStandardPlan -> CellScope::Full（billing は Phase 5）
tools/golden_compare.py              ゴールデン第一判定（セル内容ベース）スクリプト
```

### 既存ファイルの最小編集（挙動不変）
- `rust_core/src/lib.rs` … `direction` / `plan` / `planning` を登録。
- `rust_core/src/core1/analyzer.rs` … 翻訳リクエストの `from_lang/to_lang` を**ハードコード `Lang::Ja/Zh` から引数経由**に変更（呼び出し元から渡る値が (Ja,Zh) なので挙動同一）。
- `rust_core/src/entry/generate_entry_pipeline.rs` … ExecutionPlan を確定→ direction/plan を resolve → **言語ペアを翻訳経路へ、範囲制約を体験版フィルタへ**接続。`if job_plan.is_experience()` を `if let CellScope::Range { .. } = cell_scope`＋`cell_scope.contains(col, row)` による範囲判定に置換（同値）。未知の direction_id / billing_mode は `Result` で fail-fast。
- `rust_core/src/entry/estimate_entry_pipeline.rs` … 同様に resolve を接続（範囲フィルタを cell_scope.contains 経由に）。
- `web_app/app.py` … generate で `ETB_DIRECTION_ID` / `ETB_BILLING_MODE` を Rust へ受け渡し（Web→ExecutionPlan→Rust の接続）。

> 既存ロジック本体（日本語判定の閾値・A1:D5 の値）は**移設していない**。それは Phase 2 / 3 の作業。Phase 1 は「経路の接続」のみ。

---

## 2. 「resolve を本当に通っている証跡」（最終条件への対応）

実行時に標準出力へ以下が必ず出力される。これが要求された証跡に対応する。

| 要求された証跡 | 出力されるログ |
|---|---|
| direction_id=ja2zh を受け取り resolve が呼ばれた | `[EXECUTION_PLAN] direction_id=ja2zh (source=...) ...`<br>`[DIRECTION][resolve] direction_id="ja2zh" -> profile=ja2zh` |
| billing_mode=free を受け取り resolve が呼ばれた | `[EXECUTION_PLAN] ... billing_mode=free (source=...)`<br>`[PLAN][resolve] billing_mode="free" -> policy=free` |
| その結果で既存処理が実行された | `[EXECUTION_PLAN][RESOLVED] direction=ja2zh lang_pair=Ja->Zh plan=free cell_scope=Range { max_row: 5, max_col: 4 }`<br>（この lang_pair が翻訳リクエストへ、cell_scope が範囲フィルタへ流れる） |
| ゴールデン第一判定 差分ゼロ | `tools/golden_compare.py` が `RESULT = PASS (差分ゼロ)` |

注: Phase 1 では `direction_id` / `billing_mode` の既定は「ja2zh」と「現行 job_plan.mode から導出」。環境変数 `ETB_DIRECTION_ID` / `ETB_BILLING_MODE` を設定すればその値が resolve へ流れる（Web からの受け渡し確認に使用可）。**未設定時は現行挙動と完全一致**。

---

## 3. 実環境での手順（ビルド → ゴールデン取得 → 検証）

### 3-1. 先にゴールデンを取得（移行前コードで）
移行前のバイナリで、代表的な入力 xlsx に対し generate を実行し、出力UIワークブックを保存する。
```powershell
# 例（PowerShell）
cargo build --release --bin core1_etb
$env:ETB_BIN_PATH="...\rust_core\target\release\core1_etb.exe"
# 現行どおり Web もしくは CLI で generate を実行し、出力を golden.xlsx として退避
```
estimate も使う場合は estimate 出力も同様に退避する。

### 3-2. Phase 1 コードでビルド・再実行
```powershell
cargo build --release --bin core1_etb   # コンパイルが通ること（Phase 1 の最初の関門）
# 同一入力・同一設定（環境変数 ETB_DIRECTION_ID/ETB_BILLING_MODE は未設定）で generate を実行
# 出力を current.xlsx として保存
```
標準出力に §2 の `[DIRECTION][resolve]` / `[PLAN][resolve]` / `[EXECUTION_PLAN][RESOLVED]` が出ることを確認（証跡）。

### 3-3. ゴールデン第一判定
```bash
python3 tools/golden_compare.py golden.xlsx current.xlsx
# RESULT = PASS (差分ゼロ) を確認
```

---

## 4. Phase 1 完了チェックリスト（記入式）

- [ ] `cargo build --release --bin core1_etb` が成功する
- [ ] generate 実行時に `[DIRECTION][resolve] ... -> profile=ja2zh` が出力された
- [ ] generate 実行時に `[PLAN][resolve] ... -> policy=free`（または paid_standard）が出力された
- [ ] `[EXECUTION_PLAN][RESOLVED] ... lang_pair=Ja->Zh ... cell_scope=...` が出力された
- [ ] resolve の結果（lang_pair）が翻訳リクエストに渡っている（コード経路 = analyzer の from_lang/to_lang）
- [ ] resolve の結果（cell_scope）が範囲フィルタを駆動している（コード経路 = `cell_scope.contains(col, row)`）
- [ ] generate: `golden_compare.py` が PASS（差分ゼロ）
- [ ] estimate: 課金見積り出力が現行と一致（数値ベースで確認）
- [ ]（任意）`ETB_DIRECTION_ID=ja2zh` `ETB_BILLING_MODE=free` を明示設定しても PASS

### 記入欄
```
build         : [ OK / NG ]   日時:           実施者:
trace evidence: [ 確認 / 未 ]  ログ保存先:
golden(gen)   : [ PASS / FAIL ] 入力ファイル:           diff件数:
golden(est)   : [ PASS / FAIL ]
判定          : Phase 1 完了 [ 可 / 不可 ]
次フェーズ移行 : [ 承認 / 保留 ]
```

---

## 5. 注意・既知事項

- 本 Phase の `direction::resolve` / `plan::resolve` は**恒等委譲**だが、未知の id は **エラー（fail-fast）** で拒否する（誤った方向を黙って既定へ戻さない）。既定値（ja2zh / mode 由来）の適用は ExecutionPlan::from_runtime 側で、id 未指定時のみ行う。Phase 1 の通常経路では未知 id は発生しない。
- `is_in_experience_range` と `EXPERIENCE_MAX_ROW/COL` は**そのまま残置**（Phase 3 で `free_plan.cell_scope()` に実移設予定）。現時点では free_plan が同じ定数を参照しており値は一致。
- Web 側（app.py）は generate で `ETB_DIRECTION_ID="ja2zh"` と `ETB_BILLING_MODE`（mode から導出: experience→free / paid→paid_standard）を Rust へ渡すよう変更済み。これにより Web→ExecutionPlan→Rust の接続が成立する。env チェックログにも両項目を追加した。なお JSON 契約への一本化（env 撤去）は Phase 6 の JobContext 化でまとめて行う。
- ビルド検証は実環境で必須。本作業環境では cargo 不在のため未実施。
