use super::cloud_word::CloudWord;

/// 翻译给出的单条表达候选与学习细览。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PredictionAlternative {
    pub text: String,
    pub tag: String,
    pub meaning: String,
    pub tone: String,
    pub scenario: String,
    pub commonality: String,
    pub why_natural: String,
}

/// 联想结果。只是候选之外的补充展示，**不重排本地候选**：云端词补进第一页末尾几格。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prediction {
    /// 对应的请求序号。
    pub sequence: u64,

    /// 这段拼音可能对应的词（已按拼音校验）。
    pub words: Vec<CloudWord>,

    /// 组句中的整句补全，替换整段拼音。
    pub sentence: Option<String>,

    /// 针对当前候选的自然表达建议与细览（翻译模式下产出）。
    pub alternatives: Vec<PredictionAlternative>,
}

impl Prediction {
    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.sentence.is_none() && self.alternatives.is_empty()
    }
}
