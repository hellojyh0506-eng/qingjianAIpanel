use super::cloud_word::CloudWord;
use super::kind::PredictionKind;
use super::lane::PredictionLane;
use super::resolve::Resolved;

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

    /// 这是哪类请求的结果。由 Engine 按序号回填，Predictor 实现不用管；壳据此分派，不要再看 `last_prediction_kind`。
    pub kind: PredictionKind,

    /// 这段拼音可能对应的词（已按拼音校验）。
    pub words: Vec<CloudWord>,

    /// 组句中的整句补全，替换整段拼音。
    pub sentence: Option<String>,

    /// 针对当前候选的自然表达建议与细览（翻译模式下产出）。
    pub alternatives: Vec<PredictionAlternative>,

    /// 深度口语学习：音标
    pub phonetic: Option<String>,

    /// 深度口语学习：释义与地道用法说明
    pub explanation: Option<String>,

    /// 深度口语学习：高频搭配
    pub collocations: Vec<String>,

    /// 深度口语学习：例句
    pub examples: Vec<String>,

    /// 深度口语学习：雅思/正式替换
    pub ielts_friendly: Option<String>,

    /// 深度口语学习：语气色彩
    pub tone: Option<String>,

    /// 深度口语学习：核心适用场景
    pub scenario: Option<String>,

    /// 深度口语学习：核心语感精析
    pub nuance: Option<String>,

    /// 真实情境短对话：(speaker, text, zh)
    pub dialogue: Vec<(String, String, String)>,

    /// Resolve 结果：经过 Core 本地校验、可以放进候选窗口的那一条；没通过校验或模型没把握时为 `None`。
    pub resolved: Option<Resolved>,
}

impl Prediction {
    pub fn lane(&self) -> PredictionLane {
        self.kind.lane()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
            && self.sentence.is_none()
            && self.alternatives.is_empty()
            && self.explanation.is_none()
    }
}
