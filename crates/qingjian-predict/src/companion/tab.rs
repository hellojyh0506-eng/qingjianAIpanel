//! 学习助手的三个标签。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tab {
    /// 语感解析：意思、语气、使用场景、一条自然例句、搭配与语法细节。
    Nuance,

    /// 更自然地说：3 个更地道的替换表达。
    Alternatives,

    /// 真实对话：按场景生成的 3 句对话。
    Dialogue,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Nuance, Tab::Alternatives, Tab::Dialogue];

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    pub fn index(self) -> usize {
        match self {
            Tab::Nuance => 0,
            Tab::Alternatives => 1,
            Tab::Dialogue => 2,
        }
    }

    /// 请求的 `max_tokens`：按该标签输出的 JSONL 行数留余量。
    pub fn max_tokens(self) -> u32 {
        match self {
            Tab::Nuance => 600,
            Tab::Alternatives => 700,
            Tab::Dialogue => 500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_round_trips() {
        for tab in Tab::ALL {
            assert_eq!(Tab::from_index(tab.index()), Some(tab));
        }
        assert_eq!(Tab::from_index(3), None);
    }
}
