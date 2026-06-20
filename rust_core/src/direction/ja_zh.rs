// JA→ZH 方向プロファイル。
// Phase 1 では現行ハードコード (Lang::Ja, Lang::Zh) と完全に同一。

use crate::adapters::types::Lang;
use crate::direction::DirectionProfile;

pub struct JaZhProfile;

impl DirectionProfile for JaZhProfile {
    fn id(&self) -> &'static str {
        "ja2zh"
    }

    fn lang_pair(&self) -> (Lang, Lang) {
        (Lang::Ja, Lang::Zh)
    }
}
