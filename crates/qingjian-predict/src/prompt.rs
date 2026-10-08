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

/// 翻译的系统提示：为日常口语交流提供最自然鲜活的地道当代美式口语。短小、精悍、快速。
pub const TRANSLATE_SYSTEM_PROMPT: &str = "\
你是一个日常口语即时翻译与美式表达引擎。你会收到一段 JSON：\
text（待翻译中文）、target_language（目标语言代码：zh 中文、en 英语、ja 日语）。\
核心使命：为当前中文提供地道鲜活的美式口语表达。\
要求：\
1. 严格且仅翻译 text 本身，绝不扩写、不解释、不加引号、不加「译文：」等前缀；\
2. 当 target_language 为 en（英语）时：\
   - 必须是 100% 地道真实的美式口语（Authentic Gen Z / Spoken American English），优先采用口语缩写（I'm、you're、gonna、wanna、gotta）与地道动词短语搭配；\
   - sentence 给出 1 个最佳口语表达（短小、地道、精悍）；\
   - alternatives 最多给出 1~3 个不同风格的精简备选（可为空数组 []），每个包含 text、tag（Natural/Casual/British）、note（极短中文说明）；\
3. 保留原文语气；原文已是目标语言时原样返回；\
4. 严格只输出单一紧凑 JSON。\
输出格式：\
{\"sentence\": \"最佳口语译文\", \"alternatives\": [{\"text\": \"…\", \"tag\": \"Natural\", \"note\": \"极短说明\"}]}";

/// 主动回忆对比的系统提示：为口语学习者提供对比与结构化错误分类 (Error Taxonomy) 反馈。
pub const COMPARE_SYSTEM_PROMPT: &str = "\
你是一个专为英语口语进阶者（帮助 B2 跃升至 C1）量身打造的美式口语私教与评测引擎。你会收到一段 JSON：\
prompt（中文原文/提示）、user_attempt（用户的英文尝试）、reference（地道美式参考表达）。\
你的任务是深入分析用户的表达，并基于统一 Error Taxonomy 输出结构化反馈：

统一错误分类体系：
1. Grammar（语法）：article（冠词）、tense（时态语态）、preposition（介词）、word order（词序）、agreement（主谓一致）、countability（单复数/不可数）
2. Vocabulary（用词）：wrong word（选词不当）、literal translation（字面直译）、vocabulary precision（词汇精度）
3. Collocation（搭配）：unnatural collocation（搭配生硬）、incorrect verb-noun pairing（动宾不当）、adjective-noun pairing（修饰不当）、fixed expression（固定用法差异）
4. Naturalness（自然度）：awkward phrasing（表达别扭）、redundancy（冗余）、unnatural sentence rhythm（节奏生硬）、Chinglish（中式直译）
5. Phrasal / Idiomatic（短语习语）：missing phrasal verb（缺少短语动词）、wrong idiom（习语误用）、unnatural literal equivalent（生硬对等词）
6. Pragmatics（语用语域）：too direct（过于直接）、too formal（偏正式书面）、too casual（过于随意）、wrong social tone（语气不合社交场景）

核心评测与纠错原则（绝对硬约束）：
1. 重点放在 B2 → C1 突破瓶颈：naturalness, collocation, lexical precision, phrasal verbs, pragmatics。
2. 优先告诉用户「最值得改的一项」，最多输出 1~3 条核心反馈（feedback_items），绝对不要为了显得专业而罗列十几个琐碎问题。
3. 严格防范 AI 胡乱纠错（Anti-Pedantry）：
   - 明确正确、符合母语者习惯的表达严禁强行挑刺改写！
   - 合理的美式口语缩略（如 I've been, gonna, wanna, kinda, gotta, I'm good, no worries）绝不得判错！
   - 纯粹的地域差异（英美差异）或风格选择不得当作语法错误；
   - 用户的词句没有语法硬伤时，明确说明表达本身没错（如“busy 没有错，但 swamped 更口语、更符合忙得不可开交的语义”）。
4. status 判定：
   - ExactMatch: 用户的表达准确地道，完全符合母语者习惯；
   - NaturalBetter: 语法表达本身基本正确，但参考表达显著更地道更精准；
   - GrammarIssue: 存在客观语法、时态、介词或搭配错误；
   - Chinglish: 存在典型中式直译思维；
   - EmptyInput: 用户未输入内容。

输出格式（严格单一 JSON，无额外 Markdown 或解释）：
{
  \"status\": \"ExactMatch/NaturalBetter/GrammarIssue/Chinglish\",
  \"primary_category\": \"Grammar/Vocabulary/Collocation/Naturalness/PhrasalIdiomatic/Pragmatics\",
  \"key_advice\": \"简短核心建议，指出最关键的提升点\",
  \"natural_expression\": \"推荐的地道表达\",
  \"feedback_items\": [
    {
      \"major_category\": \"Grammar/Vocabulary/Collocation/Naturalness/PhrasalIdiomatic/Pragmatics\",
      \"subcategory\": \"具体子类型，如 Tense, UnnaturalCollocation, AwkwardPhrasing, Chinglish\",
      \"label\": \"中文短标签，如 时态冲突、搭配生硬、地道升级\",
      \"explanation\": \"简明扼要的原因说明\",
      \"suggestion\": \"具体改进词句\"
    }
  ]
}";

pub fn system_prompt(request: &PredictionRequest) -> &'static str {
    match request.kind {
        PredictionKind::Compose => SYSTEM_PROMPT,
        PredictionKind::Question => QUESTION_SYSTEM_PROMPT,
        PredictionKind::Translate => TRANSLATE_SYSTEM_PROMPT,
        PredictionKind::Compare => COMPARE_SYSTEM_PROMPT,
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
    /// 云端词（组句联想）或答案（问字模式，音节为空、带读音）。
    pub words: Vec<CloudWord>,

    /// 整句补全。
    pub sentence: Option<String>,

    /// 针对当前候选的翻译表达备选与细览。
    pub alternatives: Vec<PredictionAlternative>,
}

impl Reply {
    /// 什么都没给：没有词也没有整句。
    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.sentence.is_none() && self.alternatives.is_empty()
    }
}

/// 模型回复的原始形状，缺的字段当空。
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawReply {
    words: Vec<RawWord>,

    sentence: Option<String>,

    /// 问字模式的答案。
    answers: Vec<RawWord>,

    /// 翻译模式的多语域表达。
    alternatives: Vec<RawAlternative>,

    /// 主动回忆对比模式：评估状态
    status: Option<String>,

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
struct RawFeedbackItem {
    major_category: String,
    subcategory: String,
    label: String,
    explanation: String,
    suggestion: Option<String>,
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

/// 解析模型回复：去空、去重、去换行，截到 `max_items`。不要与本地首选相同的词，也不要没给拼音的词。
pub fn parse_reply(content: &str, request: &PredictionRequest) -> Reply {
    let json_str = extract_json_str(content);
    let raw: RawReply = match serde_json::from_str(json_str) {
        Ok(raw) => raw,
        Err(_) => return Reply::default(),
    };
    if request.kind == PredictionKind::Question {
        return parse_answers(raw.answers, request.max_items);
    }
    if request.kind == PredictionKind::Compare {
        let key_advice = raw
            .key_advice
            .or(raw.sentence)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
        let nat_expr = raw
            .natural_expression
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| request.reference.clone());
        let status_str = raw.status.unwrap_or_else(|| "NaturalBetter".to_string());
        let primary_cat = raw.primary_category.unwrap_or_else(|| {
            if raw.error_categories.is_empty() {
                "Naturalness".to_string()
            } else {
                raw.error_categories[0].clone()
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

        // 附加结构化建议细项 (最多 3 条)
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

        return Reply {
            words: Vec::new(),
            sentence: key_advice,
            alternatives,
        };
    }
    if request.kind == PredictionKind::Translate {
        let sentence = raw
            .sentence
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());

        if let Some(s) = sentence {
            let clean_s = strip_translation_prefix(&s);
            if request.target_language == "en" && !is_valid_english_translation(&clean_s) {
                return Reply::default();
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
            if alternatives.is_empty() {
                alternatives = synthesize_fallback_alternatives(&request.text, &clean_s);
            }
            return Reply {
                words: Vec::new(),
                sentence: Some(clean_s),
                alternatives,
            };
        }
        return Reply::default();
    }
    let mut reply = Reply::default();
    let mut seen: Vec<String> = Vec::new();
    let first_local = request.candidates.first().map(String::as_str);
    for word in raw.words {
        let text = clean(&word.text);
        let syllables: Vec<String> = word
            .pinyin
            .split(|c: char| c.is_whitespace() || c == '\'')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase())
            .collect();
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

fn is_valid_english_translation(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.len() > 150 {
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
}

