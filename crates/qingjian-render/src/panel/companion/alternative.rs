//! 「更自然地说」里的一个替换表达。

use super::example::Example;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Alternative {
    pub expression: String,

    pub zh: String,

    /// 右上角的小标签（如「日常口语」），可空。
    pub tag: String,

    /// 「语气：」后面的内容。
    pub tone: String,

    /// 「区别：」后面的内容。
    pub diff: String,

    pub example: Example,
}
