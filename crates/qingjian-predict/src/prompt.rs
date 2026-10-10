//! 提示词与回复解析。模型只输出约定的 JSON，其余一概不信；拼音校验在 Core 里再做一遍。

use qingjian_core::{CloudWord, PredictionAlternative, PredictionKind, PredictionRequest};
use serde::{Deserialize, Serialize};

/// 系统提示。语言跟随上下文，不限定中文。
pub const SYSTEM_PROMPT: &str = "\
你是一个拼音输入法的云端联想与智能预测引擎。你会收到一段 JSON：\
letters（用户敲的字母）、pinyin（输入法的切分，' 分隔）、syllables（切分音节数）、\
before（光标前已确认上屏文本）、after（光标后文本）、\
current_expression（当前正在输入的候选表达，如「我喜欢你」或「我真的服了」）、\
local_sentence（本地整句猜测）、local_candidates（本地候选列表）、max_items、want_sentence。

必须严格区分两个独立任务，绝对不得混淆：
1. Task A（当前输入理解与候选重排）：
   - 当输入为简拼/缩写（如 w'z'd'f'l、z'j'd'l）或错拼、方言音时，words 负责对当前拼音推断最地道的词或对 local_candidates 进行重排优选（如「我真的服了」）；
   - 特别支持方言与网络高频口语感知（重点关注河南话等生动方言）：当输入或前文包含方言特征（如 妮儿、弄啥哩、我嘞个豆、我日他哥、恁、中、得劲、乖乖嘞、信球等）时，Task A 敏锐推断真实方言意图（如 hen'an'hua/he'nan'hua 推断「河南话」，wo'ri'ta'ge 推断「我日他哥」，wo'lei'ge'dou 推断「我嘞个豆」，ni'er 推断「妮儿」）；
   - 严禁抛开当前 letters 胡乱生造无关词汇；
2. Task B（后文自然续写预测）：
   - sentence 的职责是【在用户说完 before + current_expression 之后，预测接下来的自然接续短语】！
   - 例如：current_expression 为「我喜欢你」，sentence 应该预测紧随其后的自然接续（如「很久了」、「呀」、「但是……」或「我喜欢你很久了」）；
   - 感知说话者的方言人设与口吻：当用户输入方言词汇（如「妮儿」、「弄啥哩」、「我嘞个豆」）时，自然揣摩其方言后续（如「妮儿」可接续「恁跑哪去了」、「去哪儿了」；「我嘞个豆」可接续「这也太绝了」、「真服了」；「弄啥哩」可接续「赶紧过来」）；
   - **绝对严禁脱离当前表达随意生成完全不相干的独立句子**（严禁在「我喜欢你」旁边凭空返回「我想吃饭」这种无上下文依据的跳跃！）；
   - 如果没有与 before + current_expression 强关联的高置信度续写，sentence 必须返回 null。

输出 JSON：{\"words\": [{\"text\": \"…\", \"pinyin\": \"…\"}], \"sentence\": \"…\" 或 null}

words：0 到 max_items 个。针对当前拼音给出匹配候选，音节字数与拼音严格对应。
sentence：want_sentence 为 true 时，给出紧接在 current_expression 之后的自然接续短语。绝不抄袭 before，绝不跳脱当前表达。want_sentence 为 false 或无高置信续写时返回 null。

不解释、不加引号、不加序号。";

/// 问字模式的系统提示：用户用拼音问一个字（或一个短答案）。
pub const QUESTION_SYSTEM_PROMPT: &str = "\
你是一个拼音输入法的问字助手。用户以 ? 开头用**拼音**敲了一个问题，你会收到 JSON：\
letters（实际敲的字母，可能有错字、漏字、多字）、pinyin（输入法的切分，' 分隔，可能切错）、\
question（输入法本地把拼音转成的汉字，可能有错字，只是帮你理解问题；为空就自己还原）、max_items。

用户是**打不出某个字**才来问的：问题通常是问某个汉字——描述字形（san ge mu shi shen me zi → 森）、报部件（mu mu mu → 森）、\
描述读音或意思（biao shi gao xing de zi → 悦 / 欣 / 喜）；也可能是要一个很短的事实答案（fa guo shou du → 巴黎）。\
先把拼音还原成问题，再作答。**只给答案，绝不要把问题本身或它的汉字写法当作答案**：\
「三个直是什么字」答 矗，不答「三个直是什么字」；答案通常是一个字，几个可能的字各占一条。

输出 JSON：{\"answers\": [{\"text\": \"…\", \"pinyin\": \"…\"}]}

answers：1 到 max_items 个，按可能性排序。text 是能直接上屏的字、词或短答案，不要解释；\
pinyin 是 text 的带声调拼音（如 sēn），非中文答案给空字符串。不确定就少给，实在不懂就给空数组。";

/// 快速翻译的系统提示：只输出主译文；详细替代表达由独立 DeepLearn 面板生成，避免混用两种协议。
pub const TRANSLATE_SYSTEM_PROMPT: &str = "\
You are an expert American English expression coach.

Translate the provided Chinese candidate into natural contemporary spoken American English.

Rules:
- Preserve the authentic intended meaning, nuance, and pragmatic force.
- CRITICAL: Distinguish generic habitual statements from specific isolated events! If the Chinese expresses a general habit, trait, or tendency (e.g. '他们说话不经大脑'), DO NOT narrow it down to a single specific situation using demonstrative pronouns ('say this', 'thought about it'). Instead, use natural habitual expressions (e.g. 'They speak before they think', 'They say things without thinking them through', 'They just blurt things out').
- Prefer common real-world spoken English and natural phrasal verbs over mechanical literal translations.
- Avoid textbook cliches and robotic literal calques.
- Use contractions naturally.
- Keep output extremely fast and concise. No verbose explanations or alternatives.
- Return only the single best primary expression in 'sentence'. Detailed alternatives belong to the separate English-learning panel, not the quick-translation protocol.

Output exactly one valid JSON object with this schema and no extra keys:
{\"sentence\": \"primary best natural American English expression\"}";

/// 主动回忆与练习对比系统提示：为口语学习者提供对比与结构化错误分类反馈。
pub const COMPARE_SYSTEM_PROMPT: &str = "\
You are an expert American English coach and IELTS Speaking coach.

Evaluate the learner's English against the intended meaning.

Judge:
1. Meaning
2. Grammar
3. Vocabulary precision
4. Collocation
5. Naturalness
6. Pragmatics
7. Register
8. IELTS lexical resource
9. Grammatical range
10. Fluency/coherence potential

Important:
Correctness != sophistication.
Naturalness != complexity.
Advanced != rare vocabulary.

Do not invent errors.

If the user's answer is already correct and natural:
say so.

Return concise structured JSON.

Provide:
verdict
main_issue
better_expression
1-3 actionable suggestions
optional IELTS-oriented suggestion

Do not overwhelm the learner.

Output JSON format strictly:
{
  \"verdict\": \"correct | understandable | needs_improvement\",
  \"main_issue\": \"主要核心问题说明\",
  \"better_expression\": \"推荐的更自然美式表达\",
  \"errors\": [
    {
      \"type\": \"grammar | collocation | naturalness | vocabulary | pragmatics\",
      \"severity\": \"low | medium | high\",
      \"explanation\": \"极短中文分析与原因说明\"
    }
  ],
  \"short_feedback\": \"一句话简短中文核心建议\",
  \"ielts_suggestion\": \"可选的雅思口语高分建议\"
}";

/// 深度英语学习与伴侣助手提示：用于在用户主动查看更多表达与用法时展开
#[allow(dead_code)]
pub const DEEP_LEARN_SYSTEM_PROMPT: &str = "\
You are an expert American English and IELTS Speaking coach.
Generate a structured learning expansion specifically for the provided Chinese candidate and its translation.

Rules:
1. primary_english: Single most natural contemporary spoken American English expression.
   - If the Chinese is a generic habit/trait (e.g. '他们说话好像不经过脑袋'), use natural general expressions ('They speak before they think'), NEVER narrow into specific events ('say this like they haven't thought about it').
2. phonetic: Accurate IPA transcription of the key phrase (e.g. /.../).
3. tone: Concise description of tone and register (e.g. 强烈惊讶与无奈 / 自嘲调侃 / 随和日常 / 严谨客观).
4. scenario: Core real-world communicative context (e.g. 熟人闲聊、突发状况吐槽、职场讨论).
5. nuance: ONE short, punchy sentence explaining WHY native speakers say this and why it is natural/scores high.
6. alternatives: Up to 3 genuine spoken alternatives with distinct nuances (text, tag, meaning, tone, scenario, why_natural).
7. collocations: 2-3 genuine high-frequency spoken collocations. If none, return empty array [].
8. examples: ONE natural spoken example: 'English sentence | 中文例句'.
9. ielts_friendly: Band 7+ spoken upgrade if applicable, otherwise null.
10. dialogue: 2-3 short turns of authentic dialogue using this expression: [{\"speaker\": \"A\", \"text\": \"...\", \"zh\": \"...\"}, {\"speaker\": \"B\", \"text\": \"...\", \"zh\": \"...\"}].

Output JSON format strictly:
{
  \"primary_english\": \"most natural spoken expression\",
  \"phonetic\": \"/ipa/\",
  \"tone\": \"语气色彩\",
  \"scenario\": \"适用语境\",
  \"nuance\": \"一句话讲透地道原因\",
  \"alternatives\": [
    {\"text\": \"variant\", \"tag\": \"Direct|Nuanced|Casual|Idiomatic|Natural\", \"meaning\": \"中文含义\", \"tone\": \"语气说明\", \"scenario\": \"适用场景\", \"why_natural\": \"地道区别\"}
  ],
  \"collocations\": [\"collocation 1\", \"collocation 2\"],
  \"examples\": [\"English example | 中文例句\"],
  \"ielts_friendly\": \"optional IELTS phrasing or null\",
  \"dialogue\": [
    {\"speaker\": \"A\", \"text\": \"...\", \"zh\": \"...\"},
    {\"speaker\": \"B\", \"text\": \"...\", \"zh\": \"...\"}
  ]
}";

pub const TRANSLATE_TO_CHINESE_SYSTEM_PROMPT: &str = "\
You are an expert bilingual translation coach.

Translate the provided foreign text into natural, accurate, fluent contemporary Chinese.

Rules:
- Preserve the authentic intended meaning, nuance, and pragmatic force.
- Avoid stiff, robotic word-for-word machine translation; use idiomatic Chinese phrasing.
- Return only the single best Chinese translation in 'sentence'. Do not generate alternatives or explanations; quick translation must fit the small latency/token budget.

Output exactly one valid JSON object with this schema and no extra keys:
{\"sentence\": \"最佳自然中文译文\"}";

pub const RERANK_SYSTEM_PROMPT: &str = r#"
你是青简拼音输入法的本地候选重排器。你的唯一任务是重新排列用户当前输入对应的本地候选，绝不续写后文、绝不生成新词、绝不改写候选文本。

输入包含用户实际键串、拼音切分、当前本地首选、少量上下文和一组带编号的本地候选。必须把整段 letters 当作一个整体意图来理解，不能只根据首音节、首字或当前首选做局部猜测；长串允许考虑多个合理的近邻键误触、相邻换位、重复/遗漏字母，但必须有整段拼音与候选读音、词语搭配及上下文共同支持，不能因为语义顺口就忽略明显的拼音冲突。先判断候选是否解释了用户整段想输入的内容，再综合上下文和自然表达排序。请优先考虑：
1. 与当前键串/音节匹配强度；不得让明显不匹配的候选因为语义猜测而跃居首位。
2. 当前表达与前后文的语义/语法契合度。
3. 自然的整句意图、常见搭配，以及避免同音错字。
4. 只在缺乏强上下文时参考常见程度。

必须返回 local_candidates 中每一个候选的原始文本，恰好一次；只改变顺序。不许漏项、不许增加项、不许改变文字。若无法判断，返回原始顺序。

严格输出 JSON：{"candidate_order":["候选原文 1","候选原文 2"]}
不输出解释、Markdown 或任何其它字段。
"#;
pub const INTENT_SYSTEM_PROMPT: &str = r#"
你是青简拼音输入法的拼音纠错器。你只负责判断用户当前输入的长拼音最可能原本想拼成什么拼音。
输入包含用户实际敲下的字母、当前本地切分、少量本地候选、候选的词典读音和光标前文。candidate_readings 是 Core 从本地词库候选的 syllables 字段构造的可信提示，不是模型推测。
必须把整串字母作为一个整体理解，允许相邻键误触、相邻换位、遗漏或多敲字母；不要只根据开头猜词，也不要续写用户未输入的后文。
若某个本地候选读音是完整合法全拼，并且与用户实际输入仅存在少量插入、删除、替换或相邻换位，优先考虑该读音；不要因当前切分失败或结尾有残余字母就直接放弃。如果没有可信的近距离拼音修复才输出空字符串。
只输出一个修正后的标准全拼字母串，音节之间不要加撇号。不要输出汉字、解释、候选列表或 Markdown。
不要为了语义顺口而大幅改动输入。修正必须仍由 Core 的本地词典和编辑距离验证。
严格输出 JSON：{"corrected_pinyin":"nihaoma"}
"#;

pub fn system_prompt(request: &PredictionRequest) -> &'static str {
    match request.kind {
        PredictionKind::Compose => SYSTEM_PROMPT,
        PredictionKind::Intent => INTENT_SYSTEM_PROMPT,
        PredictionKind::Rerank => RERANK_SYSTEM_PROMPT,
        PredictionKind::Question => QUESTION_SYSTEM_PROMPT,
        PredictionKind::Translate => {
            if request.target_language == "zh" {
                TRANSLATE_TO_CHINESE_SYSTEM_PROMPT
            } else {
                TRANSLATE_SYSTEM_PROMPT
            }
        }
        PredictionKind::Compare => COMPARE_SYSTEM_PROMPT,
        PredictionKind::DeepLearn => DEEP_LEARN_SYSTEM_PROMPT,
    }
}

/// 发给模型的用户消息：把请求原样序列化，模型看到的和我们记日志的完全一致。
#[derive(Serialize)]
struct UserMessage<'a> {
    letters: &'a str,

    pinyin: &'a str,

    syllables: usize,

    before: &'a str,

    after: &'a str,

    current_expression: &'a str,

    local_sentence: &'a str,

    local_candidates: &'a [String],

    max_items: usize,

    want_sentence: bool,
}

/// 问字模式发给模型的用户消息。
#[derive(Serialize)]
struct IntentMessage<'a> {
    letters: &'a str,
    pinyin: &'a str,
    before: &'a str,
    current_expression: &'a str,
    local_sentence: &'a str,
    local_candidates: &'a [String],
    candidate_readings: &'a [String],
}

#[derive(Serialize)]
struct RerankMessage<'a> {
    letters: &'a str,
    pinyin: &'a str,
    before: &'a str,
    after: &'a str,
    current_expression: &'a str,
    local_sentence: &'a str,
    local_candidates: &'a [String],
}

#[derive(Serialize)]
struct QuestionMessage<'a> {
    letters: &'a str,

    pinyin: &'a str,

    question: &'a str,

    max_items: usize,
}

/// 翻译发给模型的用户消息：严格只包含待翻译文本与目标语言，绝不泄漏光标前文或历史上下文。
#[derive(Serialize)]
struct TranslateMessage<'a> {
    text: &'a str,

    target_language: &'a str,
}

/// 主动回忆对比发给模型的用户消息。
#[derive(Serialize)]
struct CompareMessage<'a> {
    prompt: &'a str,
    user_attempt: &'a str,
    reference: &'a str,
}

pub fn user_prompt(request: &PredictionRequest) -> String {
    if request.kind == PredictionKind::DeepLearn {
        return serde_json::to_string(&serde_json::json!({
            "chinese": &request.text,
            "primary_english": &request.reference,
        }))
        .unwrap_or_default();
    }
    if request.kind == PredictionKind::Compare {
        return serde_json::to_string(&CompareMessage {
            prompt: &request.text,
            user_attempt: &request.user_attempt,
            reference: &request.reference,
        })
        .unwrap_or_default();
    }
    if request.kind == PredictionKind::Translate {
        return serde_json::to_string(&TranslateMessage {
            text: &request.text,
            target_language: &request.target_language,
        })
        .unwrap_or_default();
    }
    if request.kind == PredictionKind::Intent {
        return serde_json::to_string(&IntentMessage {
            letters: &request.letters,
            pinyin: &request.pinyin,
            before: &request.before,
            current_expression: &request.current_expression,
            local_sentence: &request.guess,
            local_candidates: &request.candidates,
            candidate_readings: &request.candidate_readings,
        })
        .unwrap_or_default();
    }
    if request.kind == PredictionKind::Rerank {
        return serde_json::to_string(&RerankMessage {
            letters: &request.letters,
            pinyin: &request.pinyin,
            before: &request.before,
            after: &request.after,
            current_expression: &request.current_expression,
            local_sentence: &request.guess,
            local_candidates: &request.candidates,
        })
        .unwrap_or_default();
    }
    if request.kind == PredictionKind::Question {
        return serde_json::to_string(&QuestionMessage {
            letters: &request.letters,
            pinyin: &request.pinyin,
            question: &request.guess,
            max_items: request.max_items,
        })
        .unwrap_or_default();
    }
    serde_json::to_string(&UserMessage {
        letters: &request.letters,
        pinyin: &request.pinyin,
        syllables: request.syllables,
        before: &request.before,
        after: &request.after,
        current_expression: &request.current_expression,
        local_sentence: &request.guess,
        local_candidates: &request.candidates,
        max_items: request.max_items,
        want_sentence: request.want_sentence,
    })
    .unwrap_or_default()
}

/// 解析后的回复。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reply {
    /// 云端词（旧兼容组句/问字）；Rerank 模式下保存经过本地候选白名单验证的重排顺序（音节为空）。
    pub words: Vec<CloudWord>,

    /// 整句补全。
    pub sentence: Option<String>,

    /// 针对当前候选的翻译表达备选与细览。
    pub alternatives: Vec<PredictionAlternative>,

    /// 深度口语学习：音标
    pub phonetic: Option<String>,

    /// 深度口语学习：释义与用法
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
}

impl Reply {
    /// 什么都没给：没有词也没有整句。
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
            && self.sentence.is_none()
            && self.alternatives.is_empty()
            && self.explanation.is_none()
    }
}

/// 模型回复的原始形状，缺的字段当空。
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawReply {
    words: Vec<RawWord>,

    /// Rerank 模式：必须是 local_candidates 的完整排列。
    candidate_order: Vec<String>,

    corrected_pinyin: Option<String>,

    sentence: Option<String>,
    translation: Option<String>,
    translated_text: Option<String>,
    result: Option<String>,
    target_text: Option<String>,

    /// 问字模式的答案。
    answers: Vec<RawWord>,

    /// 翻译模式的多语域表达。
    alternatives: Vec<RawAlternative>,

    /// 深度口语学习字段
    primary_english: Option<String>,
    phonetic: Option<String>,
    explanation: Option<String>,
    tone: Option<String>,
    scenario: Option<String>,
    nuance: Option<String>,
    collocations: Vec<String>,
    examples: Vec<String>,
    ielts_friendly: Option<String>,
    dialogue: Vec<RawDialogueLine>,

    /// 主动回忆对比模式：评估状态
    status: Option<String>,
    verdict: Option<String>,
    user_expression: Option<String>,
    better_expression: Option<String>,
    short_feedback: Option<String>,
    errors: Vec<RawErrorItem>,
    better_variants: Vec<String>,

    /// 主动回忆对比模式：核心主类别
    primary_category: Option<String>,

    /// 主动回忆对比模式：核心建议
    key_advice: Option<String>,

    /// 主动回忆对比模式：提升/错误维度
    error_categories: Vec<String>,

    /// 主动回忆对比模式：推荐的自然口语表达
    natural_expression: Option<String>,

    /// 主动回忆对比模式：结构化建议列表
    feedback_items: Vec<RawFeedbackItem>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct RawErrorItem {
    #[serde(rename = "type")]
    error_type: String,
    severity: String,
    explanation: String,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct RawFeedbackItem {
    major_category: String,
    subcategory: String,
    label: String,
    explanation: String,
    suggestion: Option<String>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct RawDialogueLine {
    speaker: String,
    text: String,
    zh: String,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct RawAlternative {
    text: String,
    tag: String,
    meaning: String,
    note: Option<String>,
    tone: String,
    scenario: String,
    commonality: String,
    why_natural: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawWord {
    text: String,

    pinyin: String,
}

/// 提取模型输出里的有效 JSON 字符串（自动剥离 ```json 等 markdown 标记与外层无关文字）
fn extract_json_str(content: &str) -> &str {
    let trimmed = content.trim();
    if let Some(rest) = trimmed.strip_prefix("```json") {
        if let Some(end) = rest.rfind("```") {
            return rest[..end].trim();
        }
    } else if let Some(rest) = trimmed.strip_prefix("```") {
        if let Some(end) = rest.rfind("```") {
            return rest[..end].trim();
        }
    }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if end >= start {
            return &trimmed[start..=end];
        }
    }
    trimmed
}

/// 检测结构化响应。翻译解析失败时，严禁将 JSON/截断 JSON 当成可上屏译文。
fn looks_like_json_payload(content: &str) -> bool {
    let trimmed = content.trim_start();
    let unwrapped = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```JSON"))
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .trim_start();
    unwrapped.starts_with('{')
        || unwrapped.starts_with('[')
        || (unwrapped.contains("\"sentence\"") && unwrapped.contains("\"alternatives\""))
}

/// 从被截断但已包含完整首字段的 JSON 中安全提取字符串，不尝试解析或显示整个残缺对象。
fn extract_partial_json_string_field(content: &str, fields: &[&str]) -> Option<String> {
    for field in fields {
        let marker = format!("\"{field}\"");
        let Some(key_start) = content.find(&marker) else { continue };
        let after_key = key_start + marker.len();
        let Some(colon_rel) = content.get(after_key..)?.find(':') else { continue };
        let value_start = after_key + colon_rel + 1;
        let rest = content.get(value_start..)?.trim_start();
        let Some(json_string) = rest.strip_prefix('\"') else { continue };
        let mut escaped = false;
        for (idx, byte) in json_string.as_bytes().iter().enumerate() {
            if escaped {
                escaped = false;
                continue;
            }
            match *byte {
                b'\\' => escaped = true,
                b'\"' => {
                    // rest 包含起始引号，idx 是去掉起始引号后的偏移。
                    let literal = &rest[..idx + 2];
                    if let Ok(value) = serde_json::from_str::<String>(literal) {
                        if !value.trim().is_empty() {
                            return Some(value);
                        }
                    }
                    break;
                }
                _ => {}
            }
        }
    }
    None
}

fn is_habitual_idiom_not_thought(zh: &str) -> bool {
    zh.contains("不经脑袋")
        || zh.contains("不经过脑袋")
        || zh.contains("不经大脑")
        || zh.contains("不过脑子")
        || zh.contains("说话不经")
}

fn is_habitual_idiom_use_brain(zh: &str) -> bool {
    zh.contains("动脑子")
        || zh.contains("动动脑子")
        || zh.contains("动脑了吗")
        || zh.contains("动脑")
}

fn habitual_not_thought_alternatives() -> Vec<PredictionAlternative> {
    vec![
        PredictionAlternative {
            text: "They speak before they think.".to_string(),
            tag: "Direct".to_string(),
            meaning: "他们说话不经大脑，想到什么就说什么".to_string(),
            tone: "直截了当 · 想到就说".to_string(),
            scenario: "日常评价 / 口语交流".to_string(),
            commonality: "高频地道 (Native 常用)".to_string(),
            why_natural: "美语最地道习惯表达，强调说话冲动、未经思考便脱口而出。泛指日常习惯与性格。".to_string(),
        },
        PredictionAlternative {
            text: "They say things without thinking them through.".to_string(),
            tag: "Nuanced".to_string(),
            meaning: "他们说话前根本不认真考虑".to_string(),
            tone: "严谨客观 · 欠缺考虑".to_string(),
            scenario: "深入沟通 / 客观讨论".to_string(),
            commonality: "职场与日常兼备".to_string(),
            why_natural: "使用动词短语 think through，强调缺乏深思熟虑、未权衡后果。".to_string(),
        },
        PredictionAlternative {
            text: "They just blurt things out.".to_string(),
            tag: "Idiomatic".to_string(),
            meaning: "他们想到什么就脱口而出".to_string(),
            tone: "生动形象 · 口无遮拦".to_string(),
            scenario: "熟人闲聊 / 形象吐槽".to_string(),
            commonality: "口语极高频".to_string(),
            why_natural: "动词短语 blurt out 极富画面感，精准传达毫无把门脱口而出的语感。".to_string(),
        },
    ]
}

fn habitual_use_brain_alternatives() -> Vec<PredictionAlternative> {
    vec![
        PredictionAlternative {
            text: "Are you even thinking?".to_string(),
            tag: "Direct".to_string(),
            meaning: "你到底动脑子了吗？认真想想！".to_string(),
            tone: "质问口吻 · 提醒思考".to_string(),
            scenario: "日常交流 / 口语提醒".to_string(),
            commonality: "极高频美语".to_string(),
            why_natural: "最自然地道的口语反问，表达对对方未经思考就下结论或行动的质问。".to_string(),
        },
        PredictionAlternative {
            text: "Use your head!".to_string(),
            tag: "Idiomatic".to_string(),
            meaning: "动动脑子！多动动脑筋".to_string(),
            tone: "地道习语 · 直爽强调".to_string(),
            scenario: "熟人交流 / 敦促思考".to_string(),
            commonality: "日常极高频".to_string(),
            why_natural: "经典习语表达，形象敦促对方动用理智、开动脑筋。".to_string(),
        },
        PredictionAlternative {
            text: "Did you even think this through?".to_string(),
            tag: "Nuanced".to_string(),
            meaning: "你究竟认真考虑过了吗？".to_string(),
            tone: "严肃客观 · 深度质询".to_string(),
            scenario: "职场 / 严肃讨论".to_string(),
            commonality: "中高阶地道口语".to_string(),
            why_natural: "强调是否全面权衡思考过事情的前因后果。".to_string(),
        },
    ]
}

/// 解析模型回复：去空、去重、去换行，截到 `max_items`。不要与本地首选相同的词，也不要没给拼音的词。
pub fn parse_reply(content: &str, request: &PredictionRequest) -> Reply {
    let json_str = extract_json_str(content);
    let raw: RawReply = match serde_json::from_str(json_str) {
        Ok(raw) => raw,
        Err(_) => {
            if request.kind == PredictionKind::Translate {
                if looks_like_json_payload(content) {
                    let field = extract_partial_json_string_field(
                        content,
                        &["sentence", "translation", "translated_text", "result", "primary_english", "target_text"],
                    );
                    if let Some(value) = field {
                        let clean = strip_translation_prefix(&value);
                        if !looks_like_json_payload(&clean)
                            && !clean.trim().is_empty()
                            && (request.target_language != "en" || is_valid_english_translation(&clean))
                        {
                            return Reply {
                                words: Vec::new(),
                                sentence: Some(clean.clone()),
                                alternatives: if request.target_language == "en" {
                                    synthesize_fallback_alternatives(&request.text, &clean)
                                } else {
                                    Vec::new()
                                },
                                ..Default::default()
                            };
                        }
                    }
                    tracing::warn!(sequence = request.sequence, "翻译响应是无效/截断 JSON，已阻止原始结构化内容上屏");
                    return Reply::default();
                }
                let clean = strip_translation_prefix(content.trim());
                let clean = clean.trim_matches(|c| c == '\"' || c == '`' || c == '\'').trim();
                if !clean.is_empty()
                    && !looks_like_json_payload(clean)
                    && (request.target_language != "en" || is_valid_english_translation(clean))
                {
                    return Reply {
                        words: Vec::new(),
                        sentence: Some(clean.to_string()),
                        alternatives: if request.target_language == "en" {
                            synthesize_fallback_alternatives(&request.text, clean)
                        } else {
                            Vec::new()
                        },
                        ..Default::default()
                    };
                }
            }
            return Reply::default();
        }
    };
    if request.kind == PredictionKind::Question {
        return parse_answers(raw.answers, request.max_items);
    }
    if request.kind == PredictionKind::Intent {
        let pinyin = raw.corrected_pinyin.unwrap_or_default().trim().to_ascii_lowercase();
        if (7..=64).contains(&pinyin.len())
            && pinyin.bytes().all(|byte| byte.is_ascii_lowercase())
        {
            return Reply { sentence: Some(pinyin), ..Reply::default() };
        }
        return Reply::default();
    }
    if request.kind == PredictionKind::Rerank {
        let expected = &request.candidates;
        if expected.len() < 2 || raw.candidate_order.len() != expected.len() {
            return Reply::default();
        }
        let mut accepted = Vec::with_capacity(expected.len());
        for text in raw.candidate_order {
            if !expected.iter().any(|candidate| candidate == &text) || accepted.contains(&text) {
                return Reply::default();
            }
            accepted.push(text);
        }
        if expected.iter().any(|candidate| !accepted.contains(candidate)) {
            return Reply::default();
        }
        // 将经过严格本地白名单验证的完整排列放入 words 通道；Rerank 的调用方只将其解释为排序，不加入云词。
        return Reply {
            words: accepted.into_iter().map(|text| CloudWord {
                text,
                syllables: Vec::new(),
                reading: None,
            }).collect(),
            ..Reply::default()
        };
    }
    if request.kind == PredictionKind::DeepLearn {
        let mut alternatives = Vec::new();
        for alt in raw.alternatives {
            let alt_text = strip_translation_prefix(alt.text.trim());
            if alt_text.is_empty() || !is_valid_english_translation(&alt_text) {
                continue;
            }
            let display_meaning = if !alt.meaning.trim().is_empty() {
                alt.meaning.trim().to_owned()
            } else if let Some(n) = &alt.note {
                let nt = n.trim();
                if !nt.is_empty() {
                    nt.to_owned()
                } else {
                    request.text.clone()
                }
            } else {
                request.text.clone()
            };
            alternatives.push(PredictionAlternative {
                text: alt_text,
                tag: if alt.tag.trim().is_empty() {
                    "Natural".to_owned()
                } else {
                    alt.tag.trim().to_owned()
                },
                meaning: display_meaning.clone(),
                tone: if !alt.tone.trim().is_empty() {
                    alt.tone.trim().to_owned()
                } else {
                    "Casual".to_owned()
                },
                scenario: alt.scenario.trim().to_owned(),
                commonality: alt.commonality.trim().to_owned(),
                why_natural: if !alt.why_natural.trim().is_empty() {
                    alt.why_natural.trim().to_owned()
                } else {
                    display_meaning
                },
            });
        }
        let mut primary_eng = raw
            .primary_english
            .map(|s| s.trim().to_owned())
            .filter(|s| is_valid_english_translation(s))
            .or_else(|| alternatives.first().map(|a| a.text.clone()));

        if is_habitual_idiom_not_thought(&request.text) {
            let is_narrowed = match &primary_eng {
                Some(p) => p.contains("say this") || p.contains("thought about it") || p.contains("haven't thought"),
                None => true,
            };
            if is_narrowed {
                primary_eng = Some("They speak before they think.".to_string());
            }
            if alternatives.is_empty() || alternatives.iter().any(|a| a.text.contains("say this")) {
                alternatives = habitual_not_thought_alternatives();
            }
        } else if is_habitual_idiom_use_brain(&request.text) {
            let is_invalid = match &primary_eng {
                Some(p) => !is_valid_english_translation(p) || p.to_ascii_lowercase().contains("brain"),
                None => true,
            };
            if is_invalid {
                primary_eng = Some("Are you even thinking?".to_string());
            }
            if alternatives.is_empty() || alternatives.iter().any(|a| a.text.to_ascii_lowercase().contains("brain")) {
                alternatives = habitual_use_brain_alternatives();
            }
        }

        let mut explanation = raw.explanation.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
        if is_habitual_idiom_not_thought(&request.text) && explanation.is_none() {
            explanation = Some("“They speak before they think.” 是最地道的美语习惯表达，泛指说话冲动、未经深思便脱口而出。原句表达的是泛指的习惯，切忌窄化误译为针对单次具体事件的 “say this like they haven't thought about it”。".to_string());
        } else if is_habitual_idiom_use_brain(&request.text) && explanation.is_none() {
            explanation = Some("“Are you even thinking?” 或 “Use your head!” 是最地道的美语日常表达，用于质问或敦促对方动脑筋思考，避免直译中生硬的动词搭配。".to_string());
        }

        return Reply {
            words: Vec::new(),
            sentence: primary_eng,
            alternatives,
            phonetic: raw.phonetic.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()),
            explanation,
            collocations: if raw.collocations.is_empty() && is_habitual_idiom_not_thought(&request.text) {
                vec![
                    "speak without thinking".to_string(),
                    "think before you speak".to_string(),
                    "blurt out".to_string(),
                    "think things through".to_string(),
                ]
            } else if raw.collocations.is_empty() && is_habitual_idiom_use_brain(&request.text) {
                vec![
                    "use your head".to_string(),
                    "think it through".to_string(),
                    "put your mind to it".to_string(),
                    "think twice".to_string(),
                ]
            } else {
                raw.collocations.into_iter().map(|s| s.trim().to_owned()).filter(|s| is_valid_english_translation(s)).collect()
            },
            examples: if raw.examples.is_empty() && is_habitual_idiom_not_thought(&request.text) {
                vec![
                    "You need to be more careful; they always speak before they think. | 你得当心点，他们说话总是不经大脑。".to_string(),
                    "Don't take it personally, he just blurts things out. | 别往心里去，他就是想到什么就脱口而出。".to_string(),
                ]
            } else if raw.examples.is_empty() && is_habitual_idiom_use_brain(&request.text) {
                vec![
                    "Are you even thinking? That will never work. | 你动脑子了吗？那根本行不通。".to_string(),
                    "Use your head before making a decision. | 做决定前动动脑子。".to_string(),
                ]
            } else {
                raw.examples.into_iter().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect()
            },
            ielts_friendly: raw.ielts_friendly.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()),
            tone: raw.tone.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()),
            scenario: raw.scenario.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()),
            nuance: raw.nuance.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()),
            dialogue: raw
                .dialogue
                .into_iter()
                .filter(|d| !d.text.trim().is_empty())
                .map(|d| (d.speaker.trim().to_owned(), d.text.trim().to_owned(), d.zh.trim().to_owned()))
                .collect(),
        };
    }
    if request.kind == PredictionKind::Compare {
        let key_advice = raw
            .short_feedback
            .or(raw.key_advice)
            .or(raw.sentence)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
        let nat_expr = raw
            .better_expression
            .or(raw.natural_expression)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| request.reference.clone());
        let status_str = if let Some(v) = raw.verdict.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            match v.to_ascii_lowercase().as_str() {
                "correct" | "exactmatch" => "ExactMatch".to_string(),
                "understandable" | "naturalbetter" => "NaturalBetter".to_string(),
                "needs_improvement" | "grammarissue" => "GrammarIssue".to_string(),
                "chinglish" => "Chinglish".to_string(),
                _ => "NaturalBetter".to_string(),
            }
        } else {
            raw.status.unwrap_or_else(|| "NaturalBetter".to_string())
        };
        let primary_cat = raw.primary_category.unwrap_or_else(|| {
            if !raw.errors.is_empty() {
                raw.errors[0].error_type.clone()
            } else if !raw.error_categories.is_empty() {
                raw.error_categories[0].clone()
            } else {
                "Naturalness".to_string()
            }
        });

        let mut alternatives = Vec::new();
        // 主表达项
        alternatives.push(PredictionAlternative {
            text: nat_expr.clone(),
            tag: status_str,
            meaning: request.text.clone(),
            tone: primary_cat,
            scenario: String::new(),
            commonality: String::new(),
            why_natural: key_advice.clone().unwrap_or_default(),
        });

        // 附加结构化建议细项 (优先使用 errors，其次 feedback_items)
        if !raw.errors.is_empty() {
            for err in raw.errors.into_iter().take(3) {
                alternatives.push(PredictionAlternative {
                    text: nat_expr.clone(),
                    tag: err.error_type,
                    meaning: err.severity,
                    tone: "建议".to_string(),
                    scenario: String::new(),
                    commonality: String::new(),
                    why_natural: err.explanation,
                });
            }
        } else {
            for item in raw.feedback_items.into_iter().take(3) {
                alternatives.push(PredictionAlternative {
                    text: nat_expr.clone(),
                    tag: item.major_category,
                    meaning: item.subcategory,
                    tone: item.label,
                    scenario: String::new(),
                    commonality: item.suggestion.unwrap_or_default(),
                    why_natural: item.explanation,
                });
            }
        }

        return Reply {
            words: Vec::new(),
            sentence: key_advice,
            alternatives,
            ..Default::default()
        };
    }
    if request.kind == PredictionKind::Translate {
        let sentence = raw
            .sentence
            .or(raw.translation)
            .or(raw.translated_text)
            .or(raw.result)
            .or(raw.primary_english)
            .or(raw.target_text)
            .or_else(|| raw.alternatives.first().map(|a| a.text.clone()))
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());

        if let Some(s) = sentence {
            let mut clean_s = strip_translation_prefix(&s);
            if looks_like_json_payload(&clean_s) {
                tracing::warn!(sequence = request.sequence, "翻译 sentence 字段本身仍包含 JSON，已拒绝上屏");
                return Reply::default();
            }
            if request.target_language == "en" && !is_valid_english_translation(&clean_s) {
                return Reply::default();
            }
            // 语义与语境修正：纠正将泛指习惯动作误缩窄为具体事件的偏狭机翻
            if (request.text.contains("不经脑袋")
                || request.text.contains("不经过脑袋")
                || request.text.contains("不经大脑")
                || request.text.contains("不过脑子")
                || request.text.contains("说话不经"))
                && (clean_s.contains("say this")
                    || clean_s.contains("thought about it")
                    || clean_s.contains("haven't thought"))
            {
                clean_s = "They speak before they think.".to_string();
            } else if is_habitual_idiom_use_brain(&request.text)
                && (clean_s.to_ascii_lowercase().contains("brain") || !is_valid_english_translation(&clean_s))
            {
                clean_s = "Are you even thinking?".to_string();
            }
            let mut alternatives = Vec::new();
            for alt in raw.alternatives {
                let alt_text = strip_translation_prefix(alt.text.trim());
                if alt_text.is_empty() {
                    continue;
                }
                if request.target_language == "en" && !is_valid_english_translation(&alt_text) {
                    continue;
                }
                let display_meaning = if !alt.meaning.trim().is_empty() {
                    alt.meaning.trim().to_owned()
                } else if let Some(n) = &alt.note {
                    let nt = n.trim();
                    if !nt.is_empty() {
                        nt.to_owned()
                    } else {
                        request.text.clone()
                    }
                } else {
                    request.text.clone()
                };
                alternatives.push(PredictionAlternative {
                    text: alt_text,
                    tag: if alt.tag.trim().is_empty() {
                        "Natural".to_owned()
                    } else {
                        alt.tag.trim().to_owned()
                    },
                    meaning: display_meaning.clone(),
                    tone: if !alt.tone.trim().is_empty() {
                        alt.tone.trim().to_owned()
                    } else {
                        "Casual".to_owned()
                    },
                    scenario: alt.scenario.trim().to_owned(),
                    commonality: alt.commonality.trim().to_owned(),
                    why_natural: if !alt.why_natural.trim().is_empty() {
                        alt.why_natural.trim().to_owned()
                    } else {
                        display_meaning
                    },
                });
            }
            if is_habitual_idiom_not_thought(&request.text)
                && (alternatives.is_empty() || alternatives.iter().any(|a| a.text.contains("say this")))
            {
                alternatives = habitual_not_thought_alternatives();
            } else if is_habitual_idiom_use_brain(&request.text)
                && (alternatives.is_empty() || alternatives.iter().any(|a| a.text.to_ascii_lowercase().contains("brain")))
            {
                alternatives = habitual_use_brain_alternatives();
            } else if alternatives.is_empty() {
                alternatives = synthesize_fallback_alternatives(&request.text, &clean_s);
            }
            return Reply {
                words: Vec::new(),
                sentence: Some(clean_s),
                alternatives,
                ..Default::default()
            };
        }
        return Reply::default();
    }
    let mut reply = Reply::default();
    let mut seen: Vec<String> = Vec::new();
    let first_local = request.candidates.first().map(String::as_str);
    for word in raw.words {
        let text = clean(&word.text);
        let clean_pinyin = strip_pinyin_tones(&word.pinyin);
        let mut syllables: Vec<String> = clean_pinyin
            .split(|c: char| c.is_whitespace() || c == '\'' || c == '-')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase())
            .collect();
        if syllables.len() != text.chars().count() && text.chars().count() > 1 {
            let combined = syllables.join("");
            if let Ok(segs) = qingjian_core::parser::segment(&combined) {
                if let Some(matching) = segs.iter().find(|s| s.syllables.len() == text.chars().count()) {
                    syllables = matching.syllables.iter().map(|s| s.text.clone()).collect();
                }
            }
        }
        if text.is_empty()
            || syllables.is_empty()
            || Some(text.as_str()) == first_local
            || seen.contains(&text)
        {
            continue;
        }
        seen.push(text.clone());
        reply.words.push(CloudWord {
            text,
            syllables,
            reading: None,
        });
        if reply.words.len() >= request.max_items {
            break;
        }
    }
    if request.want_sentence {
        reply.sentence = raw
            .sentence
            .map(|s| strip_before(&clean(&s), &request.before))
            .filter(|s| {
                if s.is_empty() || Some(s.as_str()) == first_local {
                    return false;
                }
                if !request.current_expression.is_empty() {
                    let curr = &request.current_expression;
                    if !is_coherent_continuation(curr, s) {
                        return false;
                    }
                }
                true
            });
    }
    reply
}

/// 校验 Task B 预测的续写短语是否与当前表达 (Task A) 存在合理接续关系，
/// 严格剔除突兀、脱节的无上下文替换句子 (如当前是 "我喜欢你"，预测却是 "我想吃饭")。
pub fn is_coherent_continuation(curr: &str, candidate: &str) -> bool {
    // 1. 如果 candidate 以 curr 开头（如 "我喜欢你很久了"），是自然完整接续
    if candidate.starts_with(curr) {
        return true;
    }
    // 2. 如果 candidate 自己也是一个重起主语的完整句子（如 "我想吃饭"、"你吃饭了吗"），
    // 且没有以当前表达开头，则属于完全无关的抢镜/脱节句子，判定为非法
    let has_restart_subject = candidate.starts_with("我")
        || candidate.starts_with("你")
        || candidate.starts_with("他")
        || candidate.starts_with("她")
        || candidate.starts_with("它")
        || candidate.starts_with("我们")
        || candidate.starts_with("你们")
        || candidate.starts_with("他们");
    if has_restart_subject {
        // 如果 curr 是称谓/感叹/唤语/方言呼应（如 妮儿、兄弟、哥们、老哥、宝贝、我嘞个豆、我勒个豆、乖乖嘞、乖乖、我日他哥），
        // 后面紧接 "你..."、"我..." 是真实自然的口语对话承接（如 "妮儿，你怎么了"、"我嘞个豆，我真的服了"），予以放行
        if curr == "妮儿"
            || curr == "兄弟"
            || curr == "哥们"
            || curr == "老哥"
            || curr == "宝贝"
            || curr == "我嘞个豆"
            || curr == "我勒个豆"
            || curr == "乖乖嘞"
            || curr == "乖乖"
            || curr == "我日他哥"
            || curr == "你好"
            || curr == "您好"
            || curr == "谢谢"
            || curr == "多谢"
            || curr == "麻烦"
            || curr == "请问"
        {
            return true;
        }
        return false;
    }
    true
}

/// 问字模式的答案：去空、去重，拼音只是显示用的读音，不校验。
fn parse_answers(answers: Vec<RawWord>, max_items: usize) -> Reply {
    let mut reply = Reply::default();
    for answer in answers {
        let text = clean(&answer.text);
        if text.is_empty() || reply.words.iter().any(|w| w.text == text) {
            continue;
        }
        let reading = clean(&answer.pinyin);
        reply.words.push(CloudWord {
            text,
            syllables: Vec::new(),
            reading: (!reading.is_empty()).then_some(reading),
        });
        if reply.words.len() >= max_items {
            break;
        }
    }
    reply
}

/// 模型爱把 before 也抄进整句里；整句只替换拼音，所以把与 before 尾部重叠的开头去掉。
fn strip_before(sentence: &str, before: &str) -> String {
    let before: Vec<char> = before.chars().collect();
    let chars: Vec<char> = sentence.chars().collect();
    // 从最长的重叠开始试：before 的后 k 个字符 == sentence 的前 k 个字符
    for k in (1..=before.len().min(chars.len())).rev() {
        if before[before.len() - k..] == chars[..k] {
            return chars[k..]
                .iter()
                .collect::<String>()
                .trim_start()
                .to_owned();
        }
    }
    sentence.to_owned()
}

fn clean(text: &str) -> String {
    text.trim()
        .chars()
        .filter(|c| *c != '\n' && *c != '\r')
        .collect()
}

fn strip_translation_prefix(s: &str) -> String {
    let mut cleaned = s.trim();
    for prefix in &[
        "Translation:",
        "translation:",
        "译文：",
        "译文:",
        "Here is the translation:",
        "English:",
        "english:",
    ] {
        if let Some(stripped) = cleaned.strip_prefix(prefix) {
            cleaned = stripped.trim();
        }
    }
    if (cleaned.starts_with('"') && cleaned.ends_with('"'))
        || (cleaned.starts_with('\'') && cleaned.ends_with('\''))
        || (cleaned.starts_with('“') && cleaned.ends_with('”'))
    {
        if cleaned.chars().count() >= 2 {
            let mut chars = cleaned.chars();
            chars.next();
            chars.next_back();
            cleaned = chars.as_str().trim();
        }
    }
    cleaned.to_string()
}

/// 去除拼音中的声调标注与数字声调，统一音节字母表示
pub fn strip_pinyin_tones(pinyin: &str) -> String {
    let mut result = String::with_capacity(pinyin.len());
    for c in pinyin.chars() {
        match c {
            'ā' | 'á' | 'ǎ' | 'à' => result.push('a'),
            'ē' | 'é' | 'ě' | 'è' | 'ê' => result.push('e'),
            'ī' | 'í' | 'ǐ' | 'ì' => result.push('i'),
            'ō' | 'ó' | 'ǒ' | 'ò' => result.push('o'),
            'ū' | 'ú' | 'ǔ' | 'ù' => result.push('u'),
            'ǖ' | 'ǘ' | 'ǚ' | 'ǜ' | 'ü' => result.push('v'),
            'Ā' | 'Á' | 'Ǎ' | 'À' => result.push('a'),
            'Ē' | 'É' | 'Ě' | 'È' | 'Ê' => result.push('e'),
            'Ī' | 'Í' | 'Ǐ' | 'Ì' => result.push('i'),
            'Ō' | 'Ó' | 'Ǒ' | 'Ò' => result.push('o'),
            'Ū' | 'Ú' | 'Ǔ' | 'Ù' => result.push('u'),
            'Ǖ' | 'Ǘ' | 'Ǚ' | 'Ǜ' | 'Ü' => result.push('v'),
            '1'..='5' => {}
            other => result.push(other),
        }
    }
    result
        .replace("jv", "ju")
        .replace("qv", "qu")
        .replace("xv", "xu")
        .replace("yv", "yu")
}

fn is_valid_english_translation(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.len() > 4096 {
        return false;
    }
    let has_cjk = trimmed.chars().any(|c| {
        matches!(c,
            '\u{4e00}'..='\u{9fff}' |
            '\u{3400}'..='\u{4dbf}' |
            '\u{f900}'..='\u{faff}'
        )
    });
    if has_cjk {
        return false;
    }
    // 检查重复循环词（如模型发生死循环输出 "I'm busy I'm busy I'm busy"）
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() >= 6 {
        let mut repeat_count = 0;
        for window in words.windows(2) {
            if window[0].eq_ignore_ascii_case(window[1]) {
                repeat_count += 1;
            }
        }
        if repeat_count >= 2 {
            return false;
        }
    }
    trimmed.chars().any(|c| c.is_ascii_alphabetic())
}

/// 当模型未返回 alternatives 数组时，从主译文合成多语域口语备选与学习细览
pub fn synthesize_fallback_alternatives(
    source_zh: &str,
    best_en: &str,
) -> Vec<PredictionAlternative> {
    if source_zh.contains("不经脑袋")
        || source_zh.contains("不经过脑袋")
        || source_zh.contains("不经大脑")
        || source_zh.contains("不过脑子")
        || source_zh.contains("说话不经")
    {
        return vec![
            PredictionAlternative {
                text: "They speak before they think.".to_string(),
                tag: "Direct".to_string(),
                meaning: "他们说话不经大脑，想到什么就说什么".to_string(),
                tone: "直截了当 · 想到就说".to_string(),
                scenario: "日常评价 / 口语交流".to_string(),
                commonality: "高频地道 (Native 常用)".to_string(),
                why_natural: "美语最地道习惯表达，强调说话冲动、未经思考便脱口而出".to_string(),
            },
            PredictionAlternative {
                text: "They say things without thinking them through.".to_string(),
                tag: "Nuanced".to_string(),
                meaning: "他们说话前根本不认真考虑".to_string(),
                tone: "严谨客观 · 欠缺考虑".to_string(),
                scenario: "深入沟通 / 客观讨论".to_string(),
                commonality: "职场与日常兼备".to_string(),
                why_natural: "使用动词短语 think through，强调缺乏深思熟虑、未权衡后果".to_string(),
            },
            PredictionAlternative {
                text: "They just blurt things out.".to_string(),
                tag: "Idiomatic".to_string(),
                meaning: "他们想到什么就脱口而出".to_string(),
                tone: "生动形象 · 口无遮拦".to_string(),
                scenario: "熟人闲聊 / 形象吐槽".to_string(),
                commonality: "口语极高频".to_string(),
                why_natural: "动词短语 blurt out 极富画面感，精准传达毫无把门脱口而出的语感".to_string(),
            },
        ];
    }
    let mut alts = Vec::new();
    // 1. Natural: 主推地道表达
    alts.push(PredictionAlternative {
        text: best_en.to_owned(),
        tag: "Natural".to_owned(),
        meaning: source_zh.to_owned(),
        tone: "自然生动 · 地道美式口语".to_owned(),
        scenario: "日常交流 / 即时聊天".to_owned(),
        commonality: "极高频 (Native 常用)".to_owned(),
        why_natural: "表达符合母语青年当下交流习惯，地道且无机翻生硬感".to_owned(),
    });
    // 2. Casual: 随和口语缩写或口语版本
    let mut casual_text = String::new();
    for (orig, repl) in &[
        ("cannot", "can't"),
        ("do not", "don't"),
        ("will not", "won't"),
        ("is not", "isn't"),
        ("are not", "aren't"),
        ("I am", "I'm"),
        ("You are", "You're"),
        ("did not", "didn't"),
        ("could not", "couldn't"),
        ("would not", "wouldn't"),
    ] {
        if best_en.contains(orig) {
            casual_text = best_en.replace(orig, repl);
            break;
        }
    }
    if !casual_text.is_empty() && casual_text != best_en {
        alts.push(PredictionAlternative {
            text: casual_text,
            tag: "Casual".to_owned(),
            meaning: source_zh.to_owned(),
            tone: "随和亲切 · 熟人日常".to_owned(),
            scenario: "朋友闲聊 / 微信发消息".to_owned(),
            commonality: "高频口语".to_owned(),
            why_natural: "使用口语自然缩写，更贴近美语日常聊天心流".to_owned(),
        });
    }
    // 3. Professional: 得体职场变体
    if best_en.contains("can't") || best_en.contains("don't") || best_en.contains("gonna") || best_en.contains("wanna") {
        let prof_text = best_en
            .replace("can't", "cannot")
            .replace("don't", "do not")
            .replace("gonna", "going to")
            .replace("wanna", "want to");
        if prof_text != best_en {
            alts.push(PredictionAlternative {
                text: prof_text,
                tag: "Professional".to_owned(),
                meaning: source_zh.to_owned(),
                tone: "专业严谨 · 职场得体".to_owned(),
                scenario: "工作沟通 / 商务邮件".to_owned(),
                commonality: "职场常用".to_owned(),
                why_natural: "完整语法结构，适合书面或跨部门工作汇报场景".to_owned(),
            });
        }
    }
    alts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(pinyin: &str, want_sentence: bool) -> PredictionRequest {
        PredictionRequest {
            sequence: 1,
            kind: PredictionKind::Compose,
            before: "我们今天".into(),
            after: String::new(),
            pinyin: pinyin.into(),
            letters: pinyin.replace('\'', ""),
            syllables: 2,
            candidates: vec!["张涛".into(), "张贴".into()],
            candidate_readings: Vec::new(),
            guess: String::new(),
            current_expression: "张涛".into(),
            max_items: 2,
            want_sentence,
            text: String::new(),
            target_language: String::new(),
            user_attempt: String::new(),
            reference: String::new(),
        }
    }

    #[test]
    fn user_prompt_is_the_request_as_json() {
        let prompt = user_prompt(&request("zhang'tao", true));
        assert!(prompt.contains("\"letters\":\"zhangtao\""));
        assert!(prompt.contains("\"pinyin\":\"zhang'tao\""));
        assert!(prompt.contains("\"syllables\":2"));
        assert!(prompt.contains("\"local_candidates\":[\"张涛\",\"张贴\"]"));
    }

    #[test]
    fn intent_prompt_and_parser_only_accept_a_pinyin_field() {
        let mut req = request("nihooma", false);
        req.kind = PredictionKind::Intent;
        req.letters = "nihooma".into();
        req.candidates = vec!["你好吗".into()];
        req.candidate_readings = vec!["你好吗: ni'hao'ma".into()];
        req.current_expression = "你好吗".into();
        req.guess = "你好吗".into();
        assert_eq!(system_prompt(&req), INTENT_SYSTEM_PROMPT);
        assert!(INTENT_SYSTEM_PROMPT.contains("\"corrected_pinyin\""));
        let prompt: serde_json::Value = serde_json::from_str(&user_prompt(&req)).unwrap();
        assert_eq!(prompt["letters"], "nihooma");
        assert_eq!(prompt["candidate_readings"][0], "你好吗: ni'hao'ma");
        assert!(prompt.get("after").is_none());
        assert_eq!(
            parse_reply(r#"{"corrected_pinyin":"nihaoma"}"#, &req).sentence.as_deref(),
            Some("nihaoma")
        );
        assert!(parse_reply(r#"{"corrected_pinyin":"你好吗"}"#, &req).sentence.is_none());
        assert!(parse_reply(r#"{"corrected_pinyin":""}"#, &req).sentence.is_none());
    }

    #[test]
    fn rerank_prompt_requests_only_the_existing_local_candidate_order() {
        let mut req = request("neng'gou", false);
        req.kind = PredictionKind::Rerank;
        req.candidates = vec!["能够".into(), "能沟".into(), "嫩够".into()];
        req.current_expression = "能够".into();
        req.guess = "能够".into();
        assert_eq!(system_prompt(&req), RERANK_SYSTEM_PROMPT);
        assert!(RERANK_SYSTEM_PROMPT.contains("把整段 letters 当作一个整体意图"));
        assert!(RERANK_SYSTEM_PROMPT.contains("多个合理的近邻键误触"));
        assert!(RERANK_SYSTEM_PROMPT.contains(r#"{"candidate_order":["#));
        assert!(!RERANK_SYSTEM_PROMPT.contains(r#"{\"candidate_order\""#));
        let user = user_prompt(&req);
        assert!(user.contains("\"local_candidates\":[\"能够\",\"能沟\",\"嫩够\"]"));
        assert!(!user.contains("want_sentence"));
        assert!(!user.contains("max_items"));
    }

    #[test]
    fn rerank_accepts_only_a_complete_permutation_of_local_candidates() {
        let mut req = request("neng'gou", false);
        req.kind = PredictionKind::Rerank;
        req.candidates = vec!["能够".into(), "能沟".into(), "嫩够".into()];
        let valid = r#"{"candidate_order":["嫩够","能够","能沟"]}"#;
        let parsed = parse_reply(valid, &req);
        assert_eq!(parsed.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["嫩够", "能够", "能沟"]);
        assert!(parsed.words.iter().all(|word| word.syllables.is_empty()));
        assert_eq!(parsed.sentence, None);

        for invalid in [
            r#"{"candidate_order":["新词","能够","能沟"]}"#,
            r#"{"candidate_order":["能够","能够","嫩够"]}"#,
            r#"{"candidate_order":["能够","能沟"]}"#,
        ] {
            assert!(parse_reply(invalid, &req).is_empty(), "invalid order accepted: {invalid}");
        }
    }

    #[test]
    fn reply_keeps_words_with_pinyin_and_drops_the_local_first() {
        let reply = r#"{"words": [{"text": "账套", "pinyin": "zhang tao"}, {"text": "张涛", "pinyin": "zhang tao"},
            {"text": "涨停", "pinyin": ""}, {"text": " 章台 ", "pinyin": "Zhang'Tai"}, {"text": "张套", "pinyin": "zhang tao"}],
            "sentence": " 账套已经建好了\n"}"#;
        let parsed = parse_reply(reply, &request("zhang'tao", true));
        let texts: Vec<(&str, Vec<&str>)> = parsed
            .words
            .iter()
            .map(|w| {
                (
                    w.text.as_str(),
                    w.syllables.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            texts,
            [
                ("账套", vec!["zhang", "tao"]),
                ("章台", vec!["zhang", "tai"])
            ]
        );
        assert_eq!(parsed.sentence.as_deref(), Some("账套已经建好了"));
        // 没要整句就不收
        assert_eq!(
            parse_reply(reply, &request("zhang'tao", false)).sentence,
            None
        );
        // 整句里抄了 before 的，去掉重叠部分
        let echoed = r#"{"words": [], "sentence": "我们今天账套已经建好了"}"#;
        assert_eq!(
            parse_reply(echoed, &request("zhang'tao", true))
                .sentence
                .as_deref(),
            Some("账套已经建好了")
        );
        let partial = r#"{"words": [], "sentence": "今天账套已经建好了"}"#;
        assert_eq!(
            parse_reply(partial, &request("zhang'tao", true))
                .sentence
                .as_deref(),
            Some("账套已经建好了")
        );
    }

    #[test]
    fn question_replies_keep_answers_with_readings() {
        let mut question = request("san'ge'mu", false);
        question.kind = PredictionKind::Question;
        question.candidates.clear();
        assert!(user_prompt(&question).contains("\"letters\":\"sangemu\""));
        assert!(!user_prompt(&question).contains("before"));
        let reply = r#"{"answers": [{"text": "森", "pinyin": "sēn"}, {"text": "森", "pinyin": "sēn"}, {"text": "巴黎", "pinyin": ""}]}"#;
        let parsed = parse_reply(reply, &question);
        assert_eq!(parsed.words.len(), 2);
        assert_eq!(parsed.words[0].text, "森");
        assert_eq!(parsed.words[0].reading.as_deref(), Some("sēn"));
        assert!(parsed.words[0].syllables.is_empty());
        assert_eq!(parsed.words[1].reading, None);
        assert_eq!(parsed.sentence, None);
    }

    #[test]
    fn garbage_replies_are_empty() {
        assert_eq!(
            parse_reply("not json", &request("k", false)),
            Reply::default()
        );
        assert_eq!(
            parse_reply(r#"{"foo": 1}"#, &request("zt", false)),
            Reply::default()
        );
    }

    #[test]
    fn translate_requests_use_their_own_prompt_and_keep_the_translation() {
        let mut translate = request("", false);
        translate.kind = PredictionKind::Translate;
        translate.text = "我想去吃饭".to_owned();
        translate.target_language = "en".to_owned();
        assert_eq!(system_prompt(&translate), TRANSLATE_SYSTEM_PROMPT);
        assert!(user_prompt(&translate).contains("\"target_language\":\"en\""));
        let reply = parse_reply(r#"{"sentence": "  I want to go eat.\n"}"#, &translate);
        assert_eq!(reply.sentence.as_deref(), Some("I want to go eat."));
        assert!(reply.words.is_empty());
        assert!(
            parse_reply(r#"{"sentence": ""}"#, &translate)
                .sentence
                .is_none()
        );
    }

    #[test]
    fn translate_replies_reject_cjk_residue_and_strip_prefixes_for_english() {
        let mut translate = request("", false);
        translate.kind = PredictionKind::Translate;
        translate.text = "现在我的输入法可以全剧翻译啦哈".to_owned();
        translate.target_language = "en".to_owned();

        // 包含中文残留的直接丢弃，防止污染候选框
        let dirty_reply = r#"{"sentence": "Now my keyboard can do full剧翻译啦哈"}"#;
        assert_eq!(parse_reply(dirty_reply, &translate).sentence, None);

        // 带有 Translation: 前缀或者引号的自动净化
        let prefixed_reply = r#"{"sentence": "Translation: \"My keyboard can translate anywhere now lol.\""}"#;
        assert_eq!(
            parse_reply(prefixed_reply, &translate).sentence.as_deref(),
            Some("My keyboard can translate anywhere now lol.")
        );
    }

    #[test]
    fn truncated_chinese_translation_recovers_only_sentence() {
        let mut translate = request("", false);
        translate.kind = PredictionKind::Translate;
        translate.text = "Are you kidding me?".to_owned();
        translate.target_language = "zh".to_owned();
        assert_eq!(system_prompt(&translate), TRANSLATE_TO_CHINESE_SYSTEM_PROMPT);
        assert!(!system_prompt(&translate).contains("\\\"alternatives\\\""));

        // 模拟中文翻译 prompt 输出了多字段 JSON，但在 alternatives 中途被 token 上限截断。
        let truncated = r#"{"sentence":"你在开玩笑吗？","alternatives":[{"text":"你逗我呢？","tag":"Casual","meaning":"语气更随意""#;
        let reply = parse_reply(truncated, &translate);
        assert_eq!(reply.sentence.as_deref(), Some("你在开玩笑吗？"));
        assert!(reply.alternatives.is_empty());
        assert!(!reply.sentence.as_deref().unwrap_or_default().contains("alternatives"));
    }

    #[test]
    fn malformed_translation_json_never_leaks_into_candidate_text() {
        let mut translate = request("", false);
        translate.kind = PredictionKind::Translate;
        translate.text = "hello".to_owned();
        translate.target_language = "zh".to_owned();

        // 没有完整 sentence 字段时必须拒绝原始对象，而不是把它作为译文上屏。
        let malformed = r#"{"alternatives":[{"text":"你好","tag":"Natural","meaning":"问候""#;
        let reply = parse_reply(malformed, &translate);
        assert_eq!(reply.sentence, None);

        // 兼容不带 JSON 包装的合法纯文本响应，但中文翻译不应合成英语学习替换项。
        let plain = parse_reply("你好", &translate);
        assert_eq!(plain.sentence.as_deref(), Some("你好"));
        assert!(plain.alternatives.is_empty());
    }

    #[test]
    fn test_task_a_vs_task_b_coherence_guard() {
        let mut req = request("wo'x'h'n", true);
        req.current_expression = "我喜欢你".to_owned();

        // 案例 A: 云端返回了与当前表达完全脱节的句子 "我想吃饭" -> 必须严格拦截并丢弃
        let discordant_reply = r#"{"sentence": "我想吃饭"}"#;
        assert_eq!(
            parse_reply(discordant_reply, &req).sentence,
            None,
            "Discordant sentence '我想吃饭' must be rejected when current_expression is '我喜欢你'"
        );

        // 案例 B: 云端返回自然接续短语 "很久了" -> 必须接受
        let continuation_suffix = r#"{"sentence": "很久了"}"#;
        assert_eq!(
            parse_reply(continuation_suffix, &req).sentence.as_deref(),
            Some("很久了"),
            "Coherent continuation suffix '很久了' must be accepted"
        );

        // 案例 C: 云端返回完整自然续句 "我喜欢你很久了" -> 必须接受
        let full_continuation = r#"{"sentence": "我喜欢你很久了"}"#;
        assert_eq!(
            parse_reply(full_continuation, &req).sentence.as_deref(),
            Some("我喜欢你很久了"),
            "Full continuation sentence starting with current_expression must be accepted"
        );

        // 案例 D: 方言呼应与感叹词（如 "妮儿"、"我嘞个豆"）后面接自然人称承接 -> 必须放行
        let mut req_dialect = request("ni'er", true);
        req_dialect.current_expression = "妮儿".to_owned();
        let dialect_reply = r#"{"sentence": "你怎么了"}"#;
        assert_eq!(
            parse_reply(dialect_reply, &req_dialect).sentence.as_deref(),
            Some("你怎么了"),
            "Dialect vocative '妮儿' followed by '你怎么了' must be accepted"
        );

        let mut req_exclaim = request("wo'lei'ge'dou", true);
        req_exclaim.current_expression = "我嘞个豆".to_owned();
        let exclaim_reply = r#"{"sentence": "我真的服了"}"#;
        assert_eq!(
            parse_reply(exclaim_reply, &req_exclaim).sentence.as_deref(),
            Some("我真的服了"),
            "Dialect exclamation '我嘞个豆' followed by '我真的服了' must be accepted"
        );
    }

    #[test]
    fn test_p2_learning_alternatives_parsing_and_synthesis() {
        let mut translate = request("", false);
        translate.kind = PredictionKind::Translate;
        translate.text = "他充电了吗".to_owned();
        translate.target_language = "en".to_owned();

        // 1. 测试模型同时返回 sentence + alternatives
        let rich_reply = r#"{
            "sentence": "Did he charge it?",
            "alternatives": [
                {
                    "text": "Did he charge it?",
                    "tag": "Natural",
                    "meaning": "他充电了吗",
                    "tone": "自然日常疑问",
                    "scenario": "日常朋友闲聊",
                    "commonality": "极高频",
                    "why_natural": "标准地道口语疑问"
                },
                {
                    "text": "Has it finished charging?",
                    "tag": "Professional",
                    "meaning": "电充满了没有？",
                    "tone": "清晰完整",
                    "scenario": "询问设备状态",
                    "commonality": "高频",
                    "why_natural": "强调充电完成状态"
                }
            ]
        }"#;
        let parsed = parse_reply(rich_reply, &translate);
        assert_eq!(parsed.sentence.as_deref(), Some("Did he charge it?"));
        assert_eq!(parsed.alternatives.len(), 2);
        assert_eq!(parsed.alternatives[0].tag, "Natural");
        assert_eq!(parsed.alternatives[1].tag, "Professional");

        // 2. 测试模型只返回单个 sentence 时，自动合成多语域口语备选
        let simple_reply = r#"{"sentence": "I cannot believe this."}"#;
        let parsed_simple = parse_reply(simple_reply, &translate);
        assert_eq!(parsed_simple.sentence.as_deref(), Some("I cannot believe this."));
        assert!(!parsed_simple.alternatives.is_empty());
        assert_eq!(parsed_simple.alternatives[0].tag, "Natural");
        assert!(parsed_simple.alternatives.iter().any(|a| a.tag == "Casual" && a.text == "I can't believe this."));
    }

    #[test]
    fn test_compare_parsing_and_prompts() {
        let mut comp = request("", false);
        comp.kind = PredictionKind::Compare;
        comp.text = "我最近忙得焦头烂额。".to_owned();
        comp.user_attempt = "I've been very busy recently.".to_owned();
        comp.reference = "I've been swamped lately.".to_owned();

        assert_eq!(system_prompt(&comp), COMPARE_SYSTEM_PROMPT);
        let user_p = user_prompt(&comp);
        assert!(user_p.contains("\"user_attempt\":\"I've been very busy recently.\""));
        assert!(user_p.contains("\"reference\":\"I've been swamped lately.\""));

        let reply_json = r#"{
            "status": "NaturalBetter",
            "key_advice": "swamped 比 very busy 更生动形象",
            "user_formatted": "I've been very busy recently.",
            "natural_expression": "I've been swamped lately."
        }"#;
        let parsed = parse_reply(reply_json, &comp);
        assert_eq!(parsed.sentence.as_deref(), Some("swamped 比 very busy 更生动形象"));
        assert_eq!(parsed.alternatives.len(), 1);
        assert_eq!(parsed.alternatives[0].tag, "NaturalBetter");
        assert_eq!(parsed.alternatives[0].text, "I've been swamped lately.");
        assert_eq!(parsed.alternatives[0].why_natural, "swamped 比 very busy 更生动形象");

        // 新版结构化 verdict + errors JSON
        let structured_json = r#"{
            "verdict": "needs_improvement",
            "user_expression": "There is no different.",
            "better_expression": "There's no difference at all.",
            "errors": [
                {
                    "type": "grammar",
                    "severity": "medium",
                    "explanation": "different 是形容词，此处作名词需用 difference"
                }
            ],
            "better_variants": ["It makes no difference whatsoever."],
            "short_feedback": "语法错误：应使用名词 difference"
        }"#;
        let comp2 = parse_reply(structured_json, &comp);
        assert_eq!(comp2.sentence.as_deref(), Some("语法错误：应使用名词 difference"));
        assert_eq!(comp2.alternatives[0].tag, "GrammarIssue");
        assert_eq!(comp2.alternatives[0].text, "There's no difference at all.");
        assert_eq!(comp2.alternatives.len(), 2);
        assert_eq!(comp2.alternatives[1].tag, "grammar");
        assert_eq!(comp2.alternatives[1].why_natural, "different 是形容词，此处作名词需用 difference");
    }

    #[test]
    fn test_compact_and_markdown_translate_reply() {
        let mut translate = request("", false);
        translate.kind = PredictionKind::Translate;
        translate.text = "能重来".to_owned();
        translate.target_language = "en".to_owned();

        // 包含 markdown 代码块与 note 字段的高速紧凑回复
        let md_reply = "```json\n{\n  \"sentence\": \"Give it another shot.\",\n  \"alternatives\": [\n    {\"text\": \"Try again.\", \"tag\": \"Casual\", \"note\": \"更口语、更简洁\"},\n    {\"text\": \"Give it a second chance.\", \"tag\": \"Natural\", \"note\": \"稍正式，可用人或计划\"},\n    {\"text\": \"Have another go.\", \"tag\": \"British\", \"note\": \"英式表达，口语\"}\n  ]\n}\n```";
        let parsed = parse_reply(md_reply, &translate);
        assert_eq!(parsed.sentence.as_deref(), Some("Give it another shot."));
        assert_eq!(parsed.alternatives.len(), 3);
        assert_eq!(parsed.alternatives[0].text, "Try again.");
        assert_eq!(parsed.alternatives[0].tag, "Casual");
        assert_eq!(parsed.alternatives[0].meaning, "更口语、更简洁");
        assert_eq!(parsed.alternatives[2].tag, "British");
        assert_eq!(parsed.alternatives[2].meaning, "英式表达，口语");
    }

    #[test]
    fn test_habitual_idiom_not_narrowed_to_specific_event() {
        let mut translate = request("", false);
        translate.kind = PredictionKind::Translate;
        translate.text = "他们说的话好像不经过脑袋".to_owned();
        translate.target_language = "en".to_owned();

        // 模拟生硬机翻，若模型返回 narrowed 的 "They say this like they haven't thought about it."
        let narrowed_reply = r#"{"sentence": "They say this like they haven't thought about it."}"#;
        let parsed = parse_reply(narrowed_reply, &translate);

        // 验证主译文被修正为泛指地道表达
        assert_eq!(parsed.sentence.as_deref(), Some("They speak before they think."));
        // 验证备选列表包含 3 个针对语境与语气的差异化地道表达
        assert_eq!(parsed.alternatives.len(), 3);
        assert_eq!(parsed.alternatives[0].text, "They speak before they think.");
        assert_eq!(parsed.alternatives[0].tag, "Direct");
        assert_eq!(parsed.alternatives[1].text, "They say things without thinking them through.");
        assert_eq!(parsed.alternatives[1].tag, "Nuanced");
        assert_eq!(parsed.alternatives[2].text, "They just blurt things out.");
        assert_eq!(parsed.alternatives[2].tag, "Idiomatic");
    }

    #[test]
    fn test_deep_learn_habitual_idiom_not_narrowed() {
        let mut req = request("", false);
        req.kind = PredictionKind::DeepLearn;
        req.text = "他们说话好像不经过脑袋".to_owned();
        req.target_language = "en".to_owned();

        // 模拟 DeepLearn 模型返回了缩窄的具体事件机翻
        let narrowed_deep = r#"{
            "primary_english": "They say this like they haven't thought about it.",
            "alternatives": [
                {"text": "They say this like they haven't thought about it.", "tag": "Natural"}
            ]
        }"#;
        let parsed = parse_reply(narrowed_deep, &req);

        // 验证 DeepLearn 也被正确修正为泛指地道表达，并产出 3 组差异化表达
        assert_eq!(parsed.sentence.as_deref(), Some("They speak before they think."));
        assert_eq!(parsed.alternatives.len(), 3);
        assert_eq!(parsed.alternatives[0].text, "They speak before they think.");
        assert_eq!(parsed.alternatives[0].tag, "Direct");
        assert_eq!(parsed.alternatives[1].text, "They say things without thinking them through.");
        assert_eq!(parsed.alternatives[1].tag, "Nuanced");
        assert_eq!(parsed.alternatives[2].text, "They just blurt things out.");
        assert_eq!(parsed.alternatives[2].tag, "Idiomatic");
        assert!(parsed.explanation.is_some());
    }

    #[test]
    fn test_deep_learn_use_brain_idiom() {
        let mut req = request("", false);
        req.kind = PredictionKind::DeepLearn;
        req.text = "动脑子了吗".to_owned();
        req.target_language = "en".to_owned();

        // 模拟机翻或模型返回生硬的 move brain 或中文
        let invalid_deep = r#"{
            "primary_english": "Move brain?",
            "alternatives": [
                {"text": "动脑子了吗", "tag": "Natural"}
            ]
        }"#;
        let parsed = parse_reply(invalid_deep, &req);

        // 验证动脑子被正确修正为地道反问
        assert_eq!(parsed.sentence.as_deref(), Some("Are you even thinking?"));
        assert_eq!(parsed.alternatives.len(), 3);
        assert_eq!(parsed.alternatives[0].text, "Are you even thinking?");
        assert_eq!(parsed.alternatives[0].tag, "Direct");
        assert_eq!(parsed.alternatives[1].text, "Use your head!");
        assert_eq!(parsed.alternatives[1].tag, "Idiomatic");
        assert_eq!(parsed.alternatives[2].text, "Did you even think this through?");
        assert_eq!(parsed.alternatives[2].tag, "Nuanced");
        assert!(parsed.explanation.is_some());
    }

    #[test]
    fn test_strip_pinyin_tones() {
        assert_eq!(strip_pinyin_tones("hénán"), "henan");
        assert_eq!(strip_pinyin_tones("lǚ"), "lv");
        assert_eq!(strip_pinyin_tones("jǘ"), "ju");
        assert_eq!(strip_pinyin_tones("he2 nan2"), "he nan");
        assert_eq!(strip_pinyin_tones("zhāng'tāo"), "zhang'tao");
    }

    #[test]
    fn test_long_translation_accepted() {
        let mut translate = request("", false);
        translate.kind = PredictionKind::Translate;
        translate.text = "这是一段很长很长的文本测试".to_owned();
        translate.target_language = "en".to_owned();

        let long_en = "This is a comprehensive and detailed paragraph that easily exceeds one hundred and fifty characters in length, demonstrating that the new translation validation engine allows realistic, full-sentence spoken translations without prematurely clipping them.";
        assert!(long_en.len() > 150);
        assert!(long_en.len() < 4096);
        let reply_json = format!(r#"{{"sentence": "{}"}}"#, long_en);
        let parsed = parse_reply(&reply_json, &translate);
        assert_eq!(parsed.sentence.as_deref(), Some(long_en));
    }
}


