# -*- coding: utf-8 -*-
"""
ユーザー向けエラーメッセージの多言語辞書ライブラリ。

【使い方】
    from error_messages import t_error
    raise ValueError(t_error("XLSX_ONLY", lang))
    raise ValueError(t_error("SHEET_INDEX_OUT_OF_RANGE", lang, idx=idx))

【言語を追加する場合】
    1. SUPPORTED_LANGS に言語コードを追加する
    2. MESSAGES 内の「全てのキー」に、その言語のメッセージを1行ずつ追加する
       （既存のキーを書き換える必要は無い。追加するだけでよい）
    3. これ以外の変更は不要（app.py側は一切変更しなくてよい）

このファイル自体に技術的な詳細（ファイルパス・内部ID・スタックトレース等）を
書かないこと。技術的な詳細は呼び出し側で print() してRenderログにだけ残し、
ここに書くのは「ユーザーが読んで意味が分かる文章」だけにする。
"""

SUPPORTED_LANGS = ("ja", "zh", "vi", "en")
DEFAULT_LANG = "ja"


MESSAGES: dict[str, dict[str, str]] = {

    "XLSX_ONLY": {
        "ja": ".xlsxファイルのみアップロードできます。ファイルの種類をご確認ください。",
        "zh": "仅支持上传.xlsx文件，请确认文件类型。",
        "vi": "Chỉ có thể tải lên file .xlsx. Vui lòng kiểm tra loại file.",
        "en": "Only .xlsx files can be uploaded. Please check the file type.",
    },

    "FILE_EMPTY": {
        "ja": "ファイルの中身が空でした。別のファイルでお試しください。",
        "zh": "文件内容为空，请尝试其他文件。",
        "vi": "Nội dung file trống. Vui lòng thử file khác.",
        "en": "The file is empty. Please try a different file.",
    },

    "XLSX_CORRUPTED": {
        "ja": "このExcelファイルは壊れているため開けませんでした。Excelで一度開いて保存し直してから、もう一度アップロードしてください。",
        "zh": "该Excel文件已损坏，无法打开。请用Excel重新打开并保存后再次上传。",
        "vi": "File Excel này bị hỏng nên không thể mở được. Vui lòng mở lại bằng Excel, lưu lại rồi tải lên lần nữa.",
        "en": "This Excel file appears to be corrupted and could not be opened. Please open it in Excel, save it again, and re-upload.",
    },

    "XLSX_INVALID_FORMAT": {
        "ja": "このファイルは正しいExcel形式として認識できませんでした。.xlsx形式で保存し直してから、もう一度お試しください。",
        "zh": "无法将该文件识别为有效的Excel格式，请另存为.xlsx格式后重试。",
        "vi": "Không thể nhận dạng file này là định dạng Excel hợp lệ. Vui lòng lưu lại theo định dạng .xlsx rồi thử lại.",
        "en": "This file could not be recognized as a valid Excel format. Please re-save it as .xlsx and try again.",
    },

    "EXTERNAL_LINK_BLOCKED": {
        "ja": "外部リンク付きxlsxは安全上ブロックします。【対処法】Excelでファイルを開き、「データ」タブ→「リンクの編集」→該当リンクを選択して「リンクの解除」を行い、保存し直してから再度アップロードしてください。",
        "zh": "出于安全考虑，含有外部链接的xlsx文件将被阻止。【解决方法】用Excel打开文件，进入「数据」标签→「编辑链接」→选择相应链接并「断开链接」，保存后重新上传。",
        "vi": "Vì lý do an toàn, file xlsx có chứa liên kết ngoài sẽ bị chặn. 【Cách xử lý】Mở file bằng Excel, vào tab 'Dữ liệu' → 'Chỉnh sửa liên kết' → chọn liên kết tương ứng và 'Ngắt liên kết', sau đó lưu lại và tải lên lần nữa.",
        "en": "For security reasons, xlsx files containing external links are blocked. [How to fix] Open the file in Excel, go to the 'Data' tab → 'Edit Links' → select the link and 'Break Link', then save and re-upload.",
    },

    "MACRO_BLOCKED": {
        "ja": "マクロ（VBA）を含むExcelファイルは、安全のためアップロードできません。マクロを削除してから、もう一度お試しください。",
        "zh": "出于安全考虑，无法上传包含宏（VBA）的Excel文件，请删除宏后重试。",
        "vi": "Vì lý do an toàn, không thể tải lên file Excel có chứa macro (VBA). Vui lòng xóa macro rồi thử lại.",
        "en": "For security reasons, Excel files containing macros (VBA) cannot be uploaded. Please remove the macros and try again.",
    },

    "XLSX_OPEN_FAILED": {
        "ja": "このファイルをExcelファイルとして開けませんでした。ファイルが破損していないかご確認ください。",
        "zh": "无法将该文件作为Excel文件打开，请确认文件是否损坏。",
        "vi": "Không thể mở file này như một file Excel. Vui lòng kiểm tra xem file có bị hỏng không.",
        "en": "This file could not be opened as an Excel file. Please check whether the file is damaged.",
    },

    "FILE_NOT_SELECTED": {
        "ja": "ファイルが選択されていません。ファイルを選んでから、もう一度お試しください。",
        "zh": "尚未选择文件，请选择文件后重试。",
        "vi": "Chưa chọn file. Vui lòng chọn file rồi thử lại.",
        "en": "No file has been selected. Please choose a file and try again.",
    },

    "FILE_TOO_LARGE": {
        "ja": "ファイルが大きすぎます。上限は{limit_mb:.0f}MBですが、このファイルは約{size_mb:.1f}MBあります。不要なシートや画像を減らして{limit_mb:.0f}MB以下にしてから、もう一度お試しください。",
        "zh": "文件过大。上限为{limit_mb:.0f}MB，当前文件约{size_mb:.1f}MB。请删除不需要的工作表或图片，缩小至{limit_mb:.0f}MB以下后重试。",
        "vi": "File quá lớn. Giới hạn là {limit_mb:.0f}MB, nhưng file này khoảng {size_mb:.1f}MB. Vui lòng xóa bớt sheet hoặc hình ảnh không cần thiết để giảm xuống dưới {limit_mb:.0f}MB rồi thử lại.",
        "en": "The file is too large. The limit is {limit_mb:.0f}MB, but this file is about {size_mb:.1f}MB. Please remove unnecessary sheets or images to bring it under {limit_mb:.0f}MB and try again.",
    },

    "NO_SHEETS_FOUND": {
        "ja": "このExcelファイルにはシートが含まれていないようです。別のファイルでお試しください。",
        "zh": "该Excel文件中未找到工作表，请尝试其他文件。",
        "vi": "File Excel này dường như không chứa sheet nào. Vui lòng thử file khác.",
        "en": "This Excel file does not appear to contain any sheets. Please try a different file.",
    },

    "SHEET_SELECTION_EMPTY": {
        "ja": "翻訳対象シートが入力されていません。",
        "zh": "未输入翻译对象工作表。",
        "vi": "Chưa nhập sheet cần dịch.",
        "en": "No target sheet has been entered.",
    },

    "EXPERIENCE_ONE_SHEET_ONLY": {
        "ja": "体験コースでは、シートを1つだけ指定してください。",
        "zh": "体验课程仅可指定1个工作表。",
        "vi": "Gói dùng thử chỉ được chọn 1 sheet.",
        "en": "The trial course allows only one sheet to be specified.",
    },

    "SHEET_TOKEN_BLANK": {
        "ja": "シート指定に空欄があります。カンマの前後を確認してください。",
        "zh": "工作表指定中存在空白，请检查逗号前后。",
        "vi": "Có mục trống trong danh sách sheet. Vui lòng kiểm tra trước/sau dấu phẩy.",
        "en": "There is a blank entry in the sheet list. Please check around the commas.",
    },

    "SHEET_INDEX_OUT_OF_RANGE": {
        "ja": "シート番号「{idx}」は存在しません。シート一覧の番号をご確認ください。",
        "zh": "工作表编号「{idx}」不存在，请确认工作表列表中的编号。",
        "vi": "Không có sheet số {idx}. Vui lòng kiểm tra lại số thứ tự trong danh sách sheet.",
        "en": "Sheet number {idx} does not exist. Please check the sheet list numbers.",
    },

    "SHEET_NAME_NOT_FOUND": {
        "ja": "シート名「{token}」が見つかりません。シート名の入力が正しいかご確認ください。",
        "zh": "未找到工作表名称「{token}」，请确认输入是否正确。",
        "vi": "Không tìm thấy sheet có tên '{token}'. Vui lòng kiểm tra lại tên sheet đã nhập.",
        "en": "Sheet name '{token}' was not found. Please check that the sheet name was entered correctly.",
    },

    "RUST_PROCESSING_FAILED": {
        "ja": "翻訳処理中に問題が発生し、処理を完了できませんでした。お手数ですが、最初からもう一度お試しください。改善しない場合は、ファイル内の数式や図形が原因の可能性があります。",
        "zh": "翻译处理过程中出现问题，未能完成处理。请重新从头开始尝试。如果问题持续存在，可能是文件中的公式或图形导致的。",
        "vi": "Đã xảy ra sự cố trong quá trình dịch nên không thể hoàn tất xử lý. Vui lòng thử lại từ đầu. Nếu vẫn không cải thiện, nguyên nhân có thể do công thức hoặc hình ảnh trong file.",
        "en": "A problem occurred during translation and processing could not be completed. Please try again from the beginning. If the problem persists, formulas or shapes in the file may be the cause.",
    },

    "GENERATE_OUTPUT_MISSING": {
        "ja": "翻訳結果ファイルの作成に失敗しました。お手数ですが、最初からもう一度お試しください。",
        "zh": "翻译结果文件生成失败，请重新从头开始尝试。",
        "vi": "Không thể tạo file kết quả dịch. Vui lòng thử lại từ đầu.",
        "en": "Failed to create the translation result file. Please try again from the beginning.",
    },

    "INVALID_OPERATION_INFO": {
        "ja": "操作情報を正しく読み取れませんでした。お手数ですが、最初からもう一度お試しください。",
        "zh": "无法正确读取操作信息，请重新从头开始尝试。",
        "vi": "Không thể đọc đúng thông tin thao tác. Vui lòng thử lại từ đầu.",
        "en": "Could not correctly read the operation information. Please try again from the beginning.",
    },

    "JOB_ID_NOT_FOUND": {
        "ja": "このファイルが、どの翻訳（Generate）結果に対応するものか分かりませんでした。ファイル名が大きく変更されている可能性があります（末尾に(1)や(2)が付くのは問題ありません）。ダウンロード時のファイル名に戻すか、最初からGenerateをやり直してください。",
        "zh": "无法确定该文件对应哪一次翻译（Generate）结果，文件名可能被大幅修改。请恢复为下载时的文件名，或从头重新执行Generate。",
        "vi": "Không thể xác định file này tương ứng với kết quả dịch (Generate) nào. Tên file có thể đã bị thay đổi nhiều (có thêm '(1)' hoặc '(2)' ở cuối thì không sao). Vui lòng đổi lại tên file như lúc tải xuống, hoặc thực hiện lại Generate từ đầu.",
        "en": "Could not determine which translation (Generate) result this file corresponds to. The file name may have been changed significantly (having '(1)' or '(2)' added at the end is fine). Please restore the original downloaded file name, or run Generate again from the beginning.",
    },

    "SERVER_ORIGINAL_MISSING": {
        "ja": "翻訳（Generate）時の元ファイルが見つかりませんでした。サーバー側の一時保存が失われた可能性があります。最初からGenerateをやり直してください。Apply前ですので、やり直しに追加費用はかかりません。",
        "zh": "未找到翻译（Generate）时的原始文件，服务器端的临时保存可能已丢失。请重新从头执行Generate。由于尚未进行Apply，重新操作不会产生额外费用。",
        "vi": "Không tìm thấy file gốc tại thời điểm dịch (Generate). Bản lưu tạm phía máy chủ có thể đã bị mất. Vui lòng thực hiện lại Generate từ đầu. Vì chưa thực hiện Apply nên việc làm lại không phát sinh thêm chi phí.",
        "en": "The original file from the Generate step could not be found. The temporary copy on the server may have been lost. Please run Generate again from the beginning. Since Apply has not been performed yet, redoing this will not incur any extra cost.",
    },

    "UPLOAD_FILE_INFO_UNREADABLE": {
        "ja": "アップロードされたファイルの情報を読み取れませんでした。お手数ですが、最初からもう一度お試しください。",
        "zh": "无法读取上传文件的信息，请重新从头开始尝试。",
        "vi": "Không thể đọc thông tin file đã tải lên. Vui lòng thử lại từ đầu.",
        "en": "Could not read the information for the uploaded file. Please try again from the beginning.",
    },

    "DIRECTION_INVALID": {
        "ja": "翻訳方向の設定を正しく読み取れませんでした。お手数ですが、ホームからやり直してください。",
        "zh": "无法正确读取翻译方向设置，请从首页重新开始。",
        "vi": "Không thể đọc đúng cài đặt hướng dịch. Vui lòng quay lại trang chủ và thử lại.",
        "en": "Could not correctly read the translation direction setting. Please start again from the home page.",
    },

    "APPLY_PREPARE_ERROR": {
        "ja": "反映（Apply）処理の準備でエラーが発生しました。お手数ですが、最初からもう一度お試しください。",
        "zh": "准备执行反映（Apply）处理时发生错误，请重新从头开始尝试。",
        "vi": "Đã xảy ra lỗi khi chuẩn bị xử lý Áp dụng (Apply). Vui lòng thử lại từ đầu.",
        "en": "An error occurred while preparing the Apply process. Please try again from the beginning.",
    },

    "APPLY_NOT_COMPLETED": {
        "ja": "反映（Apply）処理が完了しませんでした。お手数ですが、最初からもう一度お試しください。",
        "zh": "反映（Apply）处理未能完成，请重新从头开始尝试。",
        "vi": "Xử lý Áp dụng (Apply) chưa hoàn tất. Vui lòng thử lại từ đầu.",
        "en": "The Apply process did not complete. Please try again from the beginning.",
    },

    "UNEXPECTED_ERROR": {
        "ja": "予期しないエラーが発生しました。お手数ですが、最初からもう一度お試しください。改善しない場合はお問い合わせください。",
        "zh": "发生了意外错误，请重新从头开始尝试。如果问题持续存在，请联系我们。",
        "vi": "Đã xảy ra lỗi ngoài dự kiến. Vui lòng thử lại từ đầu. Nếu vẫn còn lỗi, vui lòng liên hệ với chúng tôi.",
        "en": "An unexpected error occurred. Please try again from the beginning. If the problem continues, please contact us.",
    },

    "WORKBOOK_PARSE_FAILED": {
        "ja": "翻訳処理を完了できませんでした。\n\n【対処方法】\n対象のファイルをExcelで開き、改めて.xlsx形式で「名前を付けて保存」してから再度アップロードしてください。\n\n※上記を行っても改善しない場合、ファイルの構造上の原因により対応していない可能性がございます。恐れ入りますが、あらかじめご了承ください。",
        "zh": "翻译处理未能完成。\n\n【解决方法】\n请在Excel中打开目标文件，重新通过\"另存为\"保存为.xlsx格式后，再次上传。\n\n※如果完成上述操作后仍无法改善，可能是文件结构方面的原因导致暂不支持，敬请谅解。",
        "vi": "Không thể hoàn tất xử lý dịch.\n\n【Cách xử lý】\nVui lòng mở file bằng Excel, lưu lại theo định dạng .xlsx bằng chức năng \"Save As\", sau đó tải lên lần nữa.\n\n※ Nếu vẫn không cải thiện sau khi thực hiện thao tác trên, có thể do cấu trúc file nên hiện chưa được hỗ trợ. Mong quý khách thông cảm.",
        "en": "The translation process could not be completed.\n\n[How to fix]\nPlease open the file in Excel, save it again as .xlsx using \"Save As\", and re-upload it.\n\n* If this does not resolve the issue, the file's internal structure may not be supported. We apologize for the inconvenience.",
    },

    "INVALID_CANDIDATE_COMBO": {
        "ja": "選択された翻訳エンジンと翻訳方式の組合せに対応していません。\n\nDeepLを使用する場合は「文脈翻訳」を選択してください。",
        "zh": "所选择的翻译引擎与翻译方式的组合暂不支持。\n\n使用DeepL时，请选择\"上下文翻译\"。",
        "vi": "Không hỗ trợ tổ hợp giữa công cụ dịch và phương thức dịch đã chọn.\n\nNếu sử dụng DeepL, vui lòng chọn \"Dịch theo ngữ cảnh\".",
        "en": "The selected combination of translation engine and translation method is not supported.\n\nIf using DeepL, please select \"Context translation\".",
    },

}


def t_error(key: str, lang: str = DEFAULT_LANG, **kwargs) -> str:
    """
    キー名と言語コードから、ユーザー向けエラーメッセージを1件だけ組み立てて返す。

    - 未対応の言語コードが来た場合は DEFAULT_LANG (ja) にフォールバックする。
    - 未定義のキーが来た場合は、キー名をそのまま返す（フェイルセーフ。
      本番でここに落ちた場合は MESSAGES への追加漏れなので、Renderログの
      print() 側の詳細と合わせて調査すること）。
    - kwargs はメッセージ内の {idx} や {token} のようなプレースホルダーに
      そのまま渡される（str.format() と同じ書式）。
    """
    entry = MESSAGES.get(key)
    if entry is None:
        return key
    effective_lang = lang if lang in entry else DEFAULT_LANG
    template = entry.get(effective_lang) or entry.get(DEFAULT_LANG, key)
    try:
        return template.format(**kwargs)
    except (KeyError, IndexError):
        return template
