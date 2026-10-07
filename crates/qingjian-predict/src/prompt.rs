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

/// 翻译的系统提示：为口语练习与日常交流提供地道鲜活的 Gen Z 当代美式口语。拒绝机器翻译与死板应试英语。
pub const TRANSLATE_SYSTEM_PROMPT: &str = "\
你是一个专为年轻人日常口语交流量身打造的英汉口语私教与即时翻译引擎。你会收到一段 JSON：\
text（待翻译的原文）、target_language（目标语言代码：zh 中文、en 英语、ja 日语）。\
核心使命：帮助用户练习和掌握最自然鲜活的当代美式口语（Gen Z / Young Native Speaker 口语实战）。\
要求：\
1. 严格且仅翻译 text 字段中的文本本身，绝对严禁翻译任何外部推测，严禁扩写或串入历史上下文；\
2. 当 target_language 为 en（英语）时：\
   - 必须是 100% 地道真实的美式口语（Authentic Gen Z Spoken American English），如同母语青年在日常微信/聊天、发消息、打语音、街头咖啡馆对话中脱口而出的自然表达；\
   - 彻底拒绝死板教科书、考试做题英语（如 CET-4/6、中高考或语法试卷式死板英语）以及生硬字对字机器翻译；\
   - 大量采用真实口语自然的缩写（I'm、you're、gonna、wanna、kinda、gotta）、地道口语动词短语与高频习惯搭配（例如「搞定了」译为「All set.」或「Got it covered.」；「累瘫了」译为「I'm beat.」或「I'm wiped out.」；「太逗了/笑死我了」译为「That's hilarious.」或「I'm dead.」；「你咋还不来？」译为「Where you at?」或「Why aren't you here yet?」；「别墨迹了」译为「Stop dragging your feet.」或「Quit stalling.」）；\
   - 原文含有方言、调侃或情绪（如「我日他哥」、「我嘞个豆」、「得劲」）时，自然捕捉对应语气的鲜活美式口语（如「No freaking way.」「Holy crap.」「That feels so good.」），神形兼备；\
   - 语调必须真实可信，严禁在没有语境的前提下强行尬塞网络烂梗；\
   - 提供 1 个最佳表达 sentence，以及 2~4 个不同语境/语气/难度的替换表达 alternatives，每个包含 text、tag（Casual/Natural/Professional/Formal/Idiomatic）、meaning（中文含义）、tone（语气）、scenario（适用场景）、commonality（是否常见）、why_natural（更自然的原因）；\
3. 保留原文的语气、标点与情感；原文已经是目标语言时：目标不是中文就改译成中文，目标是中文就原样返回；\
4. 绝不解释、不加引号、不加「译文：」之类的前缀，严格只输出单一 JSON。\
输出 JSON 格式：\
{\"sentence\": \"最佳口语译文\", \"alternatives\": [{\"text\": \"…\", \"tag\": \"Casual/Natural/Professional/Formal/Idiomatic\", \"meaning\": \"…\", \"tone\": \"…\", \"scenario\": \"…\", \"commonality\": \"…\", \"why_natural\": \"…\"}]}";

/// 主动回忆对比的系统提示：为口语学习者提供对比与反馈。
pub const COMPARE_SYSTEM_PROMPT: &str = "\
你是一个专为英语口语学习者提供对比与反馈的 AI 助教。你会收到一段 JSON：\
prompt（中文原文/提示）、user_attempt（用户的英文表达尝试）、reference（更地道的母语口语参考表达）。\
你的任务是评估用户的表达，并提供专业、鼓励、精炼的反馈：\
1. status: \"ExactMatch\"（表达地道准确）/ \"NaturalBetter\"（能理解但参考表达更自然地道）/ \"GrammarIssue\"（有明显语法时态搭配错误）/ \"Chinglish\"（中式直译）；\
2. key_advice: 简短的一两句话解释原因（例如指出为何参考词汇如 swamped 比用户的 very busy 更地道生动，或指出具体语法搭配问题）；\
3. error_categories: 错误或提升维度列表，如 [\"Collocation\"], [\"Grammar\"], [\"Chinglish\"], [\"Naturalness\"] 等（完全正确时为空列表）；\
4. natural_expression: 最推荐的地道美式口语表达。\
输出 JSON 格式：\
{\"status\": \"…\", \"key_advice\": \"…\", \"error_categories\": [\"…\"], \"natural_expression\": \"…\"}";

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

    /// 主动回忆对比模式：核心建议
    key_advice: Option<String>,

    /// 主动回忆对比模式：提升/错误维度
    error_categories: Vec<String>,

    /// 主动回忆对比模式：推荐的自然口语表达
    natural_expression: Option<String>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct RawAlternative {
    text: String,
    tag: String,
    meaning: String,
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

/// 解析模型回复：去空、去重、去换行，截到 `max_items`。不要与本地首选相同的词，也不要没给拼音的词。
pub fn parse_reply(content: &str, request: &PredictionRequest) -> Reply {
    let raw: RawReply = match serde_json::from_str(content.trim()) {
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
        let cat_str = if raw.error_categories.is_empty() {
            "Collocation".to_string()
        } else {
            raw.error_categories.join(", ")
        };

        let mut alternatives = Vec::new();
        alternatives.push(PredictionAlternative {
            text: nat_expr,
            tag: status_str,
            meaning: request.text.clone(),
            tone: cat_str,
            scenario: String::new(),
            commonality: String::new(),
            why_natural: key_advice.clone().unwrap_or_default(),
        });

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
                alternatives.push(PredictionAlternative {
                    text: alt_text,
                    tag: if alt.tag.trim().is_empty() {
                        "Natural".to_owned()
                    } else {
                        alt.tag.trim().to_owned()
                    },
                    meaning: if alt.meaning.trim().is_empty() {
                        request.text.clone()
                    } else {
                        alt.meaning.trim().to_owned()
                    },
                    tone: alt.tone.trim().to_owned(),
                    scenario: alt.scenario.trim().to_owned(),
                    commonality: alt.commonality.trim().to_owned(),
                    why_natural: alt.why_natural.trim().to_owned(),
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
    let has_cjk = s.chars().any(|c| {
        matches!(c,
            '\u{4e00}'..='\u{9fff}' |
            '\u{3400}'..='\u{4dbf}' |
            '\u{f900}'..='\u{faff}'
        )
    });
    if has_cjk {
        return false;
    }
    s.chars().any(|c| c.is_ascii_alphabetic())
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
}

