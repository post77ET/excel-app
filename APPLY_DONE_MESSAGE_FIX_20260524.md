# APPLY_DONE_MESSAGE_FIX_20260524

位置付け: フォルダー単位で全置き換え。

修正対象: web_app/templates/apply_done.html のみ。

修正内容:
- Apply完了後画面から、Windowsがファイル名末尾に「(1)」「(2)」を付ける注意文を削除。
- Apply完了後も必要な「反映後ファイルはシート保護されています。編集する場合は保護解除してください。」の注意文は維持。

非変更対象:
- Rustコード
- app.py
- Generate / Apply契約
- ファイル照合
- ETB_APPLY_OUTPUT
- Render設定
