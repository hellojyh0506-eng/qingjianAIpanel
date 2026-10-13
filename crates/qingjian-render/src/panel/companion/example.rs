//! 一条例句：英文、中文翻译、英文里要加粗的目标短语。

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Example {
    pub en: String,

    pub zh: String,

    /// 英文里要加粗的短语；空或不在句中则不加粗。
    pub hl: String,
}
