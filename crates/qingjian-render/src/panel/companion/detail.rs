//! 「搭配与语法细节」里的一项：小标题 + 一两句说明。

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Detail {
    pub title: String,

    pub body: String,
}
