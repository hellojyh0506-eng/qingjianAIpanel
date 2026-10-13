//! 拼写纠错 / 敲错边 / 模糊音。

use super::*;

/// 每个音节都合法、整句却不通的输入（`meiganxi`）：词图里的敲错边把 没关系 读出来，作为词候选插到最前；
/// 上屏按敲的字母消耗、记个人敲错表，退格重选时退回；原样说得通（`meiganxie` 没感谢）时不改。
#[test]
fn typo_edges_in_the_lattice_correct_legal_but_unlikely_pinyin() {
    let dictionary = Dictionary::parse(
        "没关系\tmei guan xi\t800000\n关系\tguan xi\t500000\n没\tmei\t900000\n干\tgan\t200000\n\
             洗\txi\t100000\n美感\tmei gan\t300000\n感谢\tgan xie\t300000\n谢\txie\t50000\n",
    )
    .unwrap();
    let mut engine =
        Engine::new(dictionary).with_learner(Box::new(CountingLearner(HashMap::new())));
    engine.set_input("meiganxi");
    let query = engine.query().unwrap();
    assert!(query.correction.is_none());
    let first = query.candidates.items[0].clone();
    assert_eq!(first.text, "没关系");
    assert_eq!(first.kind, CandidateKind::Chinese);
    assert_eq!(first.syllables, ["mei", "guan", "xi"]);
    // 词级候选不带敲错变体：原样的 美感 还在后面
    assert!(query.candidates.items.iter().any(|c| c.text == "美感"));
    assert_eq!(engine.commit(&first), "没关系");
    assert!(engine.composition().is_empty());
    assert_eq!(engine.learner().typo_count("gan", "guan"), 1);
    assert_eq!(engine.learner().choice_weight("meiganxi", "没关系"), 1);
    // 整个删掉、同一段拼音改选 美感：敲错的记录退回
    for _ in 0..3 {
        engine.note_backspace();
    }
    engine.set_input("meiganxi");
    let meigan = engine
        .query()
        .unwrap()
        .candidates
        .items
        .into_iter()
        .find(|c| c.text == "美感")
        .unwrap();
    engine.commit(&meigan);
    assert_eq!(engine.learner().typo_count("gan", "guan"), 0);
    assert_eq!(engine.composition().text(), "xi");
    // 原样读得通：没 + 感谢 是正常整句，不动
    engine.set_input("meiganxie");
    let query = engine.query().unwrap();
    assert_eq!(query.candidates.items[0].text, "没感谢");
    assert_eq!(query.candidates.items[0].kind, CandidateKind::Sentence);
    // 两个音节也纠：ganxi → 关系
    engine.set_input("ganxi");
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "关系");
    // 太短不纠（不到 4 个字母）
    engine.set_input("gan");
    assert!(
        engine
            .query()
            .unwrap()
            .candidates
            .items
            .iter()
            .all(|c| c.text != "关系")
    );
    // 敲的拼音本身正好是一个词（按另一种切分）：不许敲错边压过它。jineng 最优切分是 jin eng，词图里读出 近藤，
    // 但 技能 的音节正好拼成整段输入
    let dictionary = Dictionary::parse(
        "技能\tji neng\t300000\n近藤\tjin teng\t900000\n近\tjin\t500000\n\
             机\tji\t400000\n能\tneng\t600000\n",
    )
    .unwrap();
    let mut engine = Engine::new(dictionary);
    engine.set_input("jineng");
    let query = engine.query().unwrap();
    assert_eq!(query.segmentations[0].joined("'"), "jin'eng");
    assert_eq!(query.candidates.items[0].text, "技能");
    assert!(!query.candidates.items[0].corrected);
    // 纠正候选最多排第 2 位，并标「纠」
    let teng = query.candidates.items.iter().position(|c| c.text == "近藤");
    assert_eq!(teng, Some(1));
    assert!(query.candidates.items[1].corrected);
    // 退回原样的路径时原样的整句照出：shude 词图里 是的（shu → shi 相邻键）赢，但 树德 正好拼成 shude，
    // 于是整句退回 属的
    let dictionary = Dictionary::parse(
            "树德\tshu de\t500\n是的\tshi de\t5000000\n属\tshu\t100000\n的\tde\t8000000\n是\tshi\t7000000\n",
        )
        .unwrap();
    let mut engine = Engine::new(dictionary);
    engine.set_input("shude");
    let query = engine.query().unwrap();
    assert_eq!(query.candidates.items[0].text, "属的");
    assert_eq!(query.candidates.items[0].kind, CandidateKind::Sentence);
    assert!(!query.candidates.items[0].corrected);
    assert_eq!(query.candidates.items[1].text, "是的");
    assert!(query.candidates.items[1].corrected);
}

/// 同一长串里多个合法音节各自敲错时，词图可以组合多个低成本纠错边。
/// `laiga` 按原样是 `lai ga`，在词库的局部证据下应能恢复成 `kai fa`（开发）。
#[test]
fn multiple_syllable_typos_can_be_combined_in_one_long_input() {
    let dictionary =
        Dictionary::parse("开发\tkai fa\t900000\n来\tlai\t2000\n开\tkai\t10000\n发\tfa\t10000\n")
            .unwrap();
    let mut engine = Engine::new(dictionary);
    engine.set_input("laiga");
    let query = engine.query().unwrap();

    let intended = query
        .candidates
        .items
        .iter()
        .find(|candidate| candidate.text == "开发")
        .expect("word formed from two typo edges");
    assert_eq!(query.candidates.items[0].text, "开发");
    assert_eq!(intended.syllables, ["kai", "fa"]);
}

/// 长串需要两处整体修复时，只在本地有完整词典/语言模型路径时恢复，且正确处理上屏映射和拼音框标记。
#[test]
fn bounded_two_edit_correction_recovers_long_unsegmentable_pinyin() {
    let dictionary = Dictionary::parse(
        "你好吗\tni hao ma\t5000\n你好\tni hao\t9000\n你\tni\t90000\n好\thao\t80000\n吗\tma\t70000\n",
    )
    .unwrap();
    let mut engine =
        Engine::new(dictionary).with_learner(Box::new(CountingLearner(HashMap::new())));
    engine.set_input("nihoomma");
    let query = engine.query().unwrap();

    assert_eq!(
        query
            .correction
            .as_ref()
            .map(|correction| correction.corrected.as_str()),
        Some("nihaoma")
    );
    assert_eq!(query.candidates.items[0].text, "你好吗");
    assert_eq!(
        query
            .marked_segments()
            .iter()
            .map(|segment| (segment.text.clone(), segment.kind))
            .collect::<Vec<_>>(),
        [
            ("ni'h".to_owned(), MarkedKind::Typed),
            ("o".to_owned(), MarkedKind::Corrected),
            ("ao'm".to_owned(), MarkedKind::Typed),
            ("m".to_owned(), MarkedKind::Corrected),
            ("a".to_owned(), MarkedKind::Typed),
        ]
    );
    assert_eq!(query.marked_cursor(), 11);

    let first = query.candidates.items[0].clone();
    assert_eq!(engine.commit(&first), "你好吗");
    assert!(engine.composition().is_empty());
    assert_eq!(engine.learner().typo_count("hoo", "hao"), 1);
    assert_eq!(engine.learner().typo_count("mma", "ma"), 1);

    // 用户准确输入的合法长拼音不能被多编辑兜底误纠。
    engine.set_input("nihaoma");
    assert!(engine.query().unwrap().correction.is_none());
}

/// 一长串里好几处漏字母（切不动）时，分段修复逐处补上，整句转换挑出通顺的那条；上屏吃掉整串原始键，每处错都记进个人敲错表。
#[test]
fn segment_repair_fixes_several_missing_letters_in_one_long_input() {
    let dictionary = Dictionary::parse(
        "今天\tjin tian\t90000\n我们\two men\t90000\n去\tqu\t80000\n去吃饭\tqu chi fan\t90000\n吃饭\tchi fan\t70000\n\
         吃\tchi\t60000\n饭\tfan\t50000\n",
    )
    .unwrap();
    let mut engine =
        Engine::new(dictionary).with_learner(Box::new(CountingLearner(HashMap::new())));
    engine.set_input("jintinwomenqchifn");
    let query = engine.query().unwrap();

    let correction = query
        .correction
        .as_ref()
        .expect("three missing letters repaired");
    assert_eq!(correction.corrected, "jintianwomenquchifan");
    assert_eq!(correction.edit.components().len(), 3);
    assert_eq!(query.candidates.items[0].text, "今天我们去吃饭");

    let first = query.candidates.items[0].clone();
    assert_eq!(engine.commit(&first), "今天我们去吃饭");
    assert!(engine.composition().is_empty());
    assert_eq!(engine.learner().typo_count("tin", "tian"), 1);

    // 敲对的长拼音不碰
    engine.set_input("jintianwomenquchifan");
    assert!(engine.query().unwrap().correction.is_none());
}

/// 模糊音命中的词按敲的字母消耗拼音（`zi` 对 `zhi`），不算敲错。
#[test]
fn fuzzy_hits_consume_the_typed_syllables() {
    let dictionary =
        Dictionary::parse("知识\tzhi shi\t500000\n只是\tzhi shi\t600000\n资\tzi\t1000\n").unwrap();
    let mut engine =
        Engine::new(dictionary).with_learner(Box::new(CountingLearner(HashMap::new())));
    engine.set_fuzzy(FuzzyRules {
        z_zh: true,
        ..FuzzyRules::default()
    });
    engine.set_input("zishi");
    let zhishi = engine
        .query()
        .unwrap()
        .candidates
        .items
        .into_iter()
        .find(|c| c.text == "知识")
        .unwrap();
    engine.commit(&zhishi);
    assert!(engine.composition().is_empty());
    assert_eq!(engine.learner().typo_count("zi", "zhi"), 0);
}

/// 接受整段一处编辑的纠正也记个人敲错表：敲的那段字母对纠正后的音节。
#[test]
fn accepted_whole_string_correction_feeds_the_typo_table() {
    let dictionary = Dictionary::parse(
            "你好吗\tni hao ma\t5000\n你好\tni hao\t9000\n你\tni\t90000\n好\thao\t80000\n吗\tma\t70000\n",
        )
        .unwrap();
    let mut engine =
        Engine::new(dictionary).with_learner(Box::new(CountingLearner(HashMap::new())));
    engine.set_input("nihooma");
    let query = engine.query().unwrap();
    assert_eq!(query.correction.as_ref().unwrap().corrected, "nihaoma");
    let first = query.candidates.items[0].clone();
    engine.commit(&first);
    assert_eq!(engine.learner().typo_count("hoo", "hao"), 1);
    // 只选到 你 就上屏：没吃到编辑处，不算接受
    engine.set_input("nihooma");
    let ni = engine
        .query()
        .unwrap()
        .candidates
        .items
        .into_iter()
        .find(|c| c.text == "你")
        .unwrap();
    engine.commit(&ni);
    assert_eq!(engine.learner().typo_count("hoo", "hao"), 1);
}

#[test]
fn spelling_correction_fixes_one_edit_and_learns_from_enter() {
    let dictionary = Dictionary::parse(
            "你好吗\tni hao ma\t5000\n你好\tni hao\t9000\n你\tni\t90000\n好\thao\t80000\n吗\tma\t70000\n\
             和\the\t50000\n何\the\t3000\n咯\tlo\t100\n你猴\tni hou\t10\n",
        )
        .unwrap();
    let mut engine = Engine::new(dictionary)
        .with_english(WordList::parse("hello\n").unwrap())
        .with_learner(Box::new(CountingLearner(HashMap::new())));
    // 换错一个字母：nihooma → nihaoma，候选来自纠正后的拼音，拼音行画出被改掉的 o
    engine.set_input("nihooma");
    let query = engine.query().unwrap();
    let correction = query.correction.clone().expect("corrected");
    assert_eq!(correction.corrected, "nihaoma");
    assert_eq!(query.candidates.items[0].text, "你好吗");
    let kinds: Vec<(String, MarkedKind)> = query
        .marked_segments()
        .iter()
        .map(|s| (s.text.clone(), s.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("ni'h".to_owned(), MarkedKind::Typed),
            ("o".to_owned(), MarkedKind::Corrected),
            ("ao'ma".to_owned(), MarkedKind::Typed),
        ]
    );
    assert_eq!(query.marked_cursor(), 10);
    // 选纠正后的词吃掉整段原串，并按原输入串记选择
    let first = query.candidates.items[0].clone();
    assert_eq!(engine.commit(&first), "你好吗");
    assert!(engine.composition().is_empty());
    engine.set_input("nihooma");
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "你好吗");
    // 相邻换位
    engine.set_input("nihoama");
    let query = engine.query().unwrap();
    assert_eq!(query.correction.as_ref().unwrap().corrected, "nihaoma");
    // 选前缀词 你好 只吃到 nihoa 之后：换位不改长度，剩 ma
    let nihao = query
        .candidates
        .items
        .iter()
        .find(|c| c.text == "你好")
        .cloned()
        .unwrap();
    engine.commit(&nihao);
    assert_eq!(engine.composition().text(), "ma");
    // 整段是英文词的不纠（hello 不会变成 和咯）
    engine.set_input("hello");
    assert!(engine.query().unwrap().correction.is_none());
    // 原样已经说得通的合法简拼不纠：nhao 是 你好 的简拼，纠成 nihao 得分一样、扣掉编辑代价就输了
    engine.set_input("nhao");
    assert!(engine.query().unwrap().correction.is_none());
    // 末尾单字母只试相邻换位：nihoa → nihao
    engine.set_input("nihoa");
    assert_eq!(
        engine.query().unwrap().correction.unwrap().corrected,
        "nihao"
    );
    // 短串不纠
    engine.set_input("nih");
    assert!(engine.query().unwrap().correction.is_none());
    // 回车原样上屏过的串以后不纠
    engine.set_input("nihooma");
    assert!(engine.query().unwrap().correction.is_some());
    assert_eq!(engine.take_raw(), "nihooma");
    engine.set_input("nihooma");
    assert!(engine.query().unwrap().correction.is_none());
}

#[test]
fn transposition_is_corrected_while_the_last_syllable_is_still_unfinished() {
    let dictionary = Dictionary::parse(
        "明天\tming tian\t8000\n明\tming\t12000\n天\ttian\t9000\n米\tmi\t3000\n给\tgei\t5000\n\
         你\tni\t90000\n太\ttai\t4000\n啊\ta\t6000\n",
    )
    .unwrap();
    let mut engine = Engine::new(dictionary);
    // 敲反了 gn 之后还在往下敲：mignt → ming't，候选按前缀出 明天，拼音行划掉 gn
    engine.set_input("mignt");
    let query = engine.query().unwrap();
    let correction = query.correction.clone().expect("corrected");
    assert_eq!(correction.corrected, "mingt");
    assert!(correction.segmentation.last_is_partial());
    let kinds: Vec<(String, MarkedKind)> = query
        .marked_segments()
        .iter()
        .map(|s| (s.text.clone(), s.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("mi".to_owned(), MarkedKind::Typed),
            ("gn".to_owned(), MarkedKind::Corrected),
            ("ng't".to_owned(), MarkedKind::Typed),
        ]
    );
    assert_eq!(query.candidates.items[0].text, "明天");
    // 再敲两键：migntia → ming'tia…，仍是 明天；选它吃掉整段原串
    engine.set_input("migntia");
    let query = engine.query().unwrap();
    assert_eq!(query.correction.as_ref().unwrap().corrected, "mingtia");
    let first = query.candidates.items[0].clone();
    assert_eq!(first.text, "明天");
    assert_eq!(engine.commit(&first), "明天");
    assert!(engine.composition().is_empty());
    // 敲完整个词：migntian → ming'tian
    engine.set_input("migntian");
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "明天");
}

#[test]
fn commit_alignment_backtracks_over_typo_variants() {
    let engine = engine();
    let syllables =
        |list: &[&str]| -> Vec<String> { list.iter().map(|s| (*s).to_owned()).collect() };
    // pingyin 上屏 拼音：pin 原样只吃三个字母会剩 gyin，退回来按敲错变体 ping → pin 吃四个，整段吃光并记敲错
    let alignment = engine.align("pingyin", &syllables(&["pin", "yin"]));
    assert_eq!(alignment.consumed, 7);
    assert_eq!(alignment.typos, [("ping".to_owned(), "pin".to_owned())]);
    // 原样、没打完的前缀、前缀词、带 ' 的输入照旧
    assert_eq!(
        engine.align("nihao", &syllables(&["ni", "hao"])).consumed,
        5
    );
    assert_eq!(
        engine
            .align("mingt", &syllables(&["ming", "tian"]))
            .consumed,
        5
    );
    assert_eq!(
        engine.align("nihaoma", &syllables(&["ni", "hao"])).consumed,
        5
    );
    assert_eq!(
        engine.align("ni'hao", &syllables(&["ni", "hao"])).consumed,
        6
    );
    // 对不上的候选还是贪心对到哪算哪
    assert_eq!(engine.align("nihao", &syllables(&["ni", "ma"])).consumed, 2);
}

#[test]
fn unfinished_last_syllable_is_not_recorded_as_a_typo() {
    let engine = engine();
    let syllables =
        |list: &[&str]| -> Vec<String> { list.iter().map(|s| (*s).to_owned()).collect() };
    // shijia 选 时间 是 jian 没敲完：jia 虽是 jian 的敲错变体，也不进个人敲错表
    for (input, list) in [
        ("shijia", ["shi", "jian"]),
        ("zhegua", ["zhe", "guan"]),
        ("woxia", ["wo", "xian"]),
    ] {
        let alignment = engine.align(input, &syllables(&list));
        assert_eq!(alignment.consumed, input.len());
        assert!(alignment.typos.is_empty(), "{input}: {:?}", alignment.typos);
    }
}

#[test]
fn fuzzy_rules_add_homophones_behind_exact_hits() {
    // 词库里只有 kai fa 系列加一个 哈；敲 kaiha 没开 f/h 时只有前缀词 开（开哈 原样读得通，词图的敲错边翻不过它），
    // 开了就出 开发（模糊命中）且覆盖更多字母排第一
    let mut engine = Engine::new(Dictionary::parse(&format!("{SAMPLE}哈\tha\t50000\n")).unwrap());
    engine.set_input("kaiha");
    let before = texts_of(&engine);
    assert!(!before.contains(&"开发".to_owned()));
    assert!(before.contains(&"开".to_owned()));
    engine.set_fuzzy(FuzzyRules {
        f_h: true,
        ..FuzzyRules::default()
    });
    let after = texts_of(&engine);
    // ha 是前缀，换成 fa 前缀后 开放（词频更高）与 开发 都出，覆盖更多字母排在 开 前面
    assert_eq!(after[0], "开放");
    assert!(after.contains(&"开发".to_owned()));
    assert!(after.contains(&"开".to_owned()));
    // 整句转换走同一套写法：xiangkaiha → 想开发
    engine.set_input("xiangkaiha");
    let query = engine.query().unwrap();
    assert_eq!(query.candidates.items[0].text, "想开发");
    assert_eq!(query.candidates.items[0].kind, CandidateKind::Sentence);
    // 敲对的仍然优先：kaifa 第一位还是 开发，且不重复
    engine.set_input("kaifa");
    let all = texts_of(&engine);
    assert_eq!(all[0], "开发");
    assert_eq!(all.iter().filter(|t| *t == "开发").count(), 1);
}

/// 用来试各类敲错的小词库。
fn typo_dictionary() -> Dictionary {
    Dictionary::parse(
        "你好吗\tni hao ma\t5000\n你好\tni hao\t9000\n你\tni\t90000\n好\thao\t80000\n吗\tma\t70000\n\
         今天\tjin tian\t90000\n明天\tming tian\t80000\n天气\ttian qi\t60000\n我们\two men\t90000\n\
         吃饭\tchi fan\t70000\n吃\tchi\t60000\n饭\tfan\t50000\n中文\tzhong wen\t80000\n\
         输入法\tshu ru fa\t70000\n输入\tshu ru\t80000\n开发\tkai fa\t70000\n开\tkai\t70000\n发\tfa\t60000\n\
         今\tjin\t50000\n天\ttian\t50000\n我\two\t90000\n们\tmen\t60000\n中\tzhong\t50000\n文\twen\t50000\n\
         学校\txue xiao\t80000\n学习\txue xi\t80000\n中国\tzhong guo\t90000\n人民\tren min\t80000\n\
         工作\tgong zuo\t80000\n",
    )
    .unwrap()
}

/// 每一类敲错各给几条典型例子：本地都能纠回来，纠正出来的整串候选排第一并标「纠」。
#[test]
fn typical_typos_of_each_kind_are_corrected_and_marked() {
    let cases: &[(&str, &str, &str, &str)] = &[
        ("换错相邻键", "nihooma", "nihaoma", "你好吗"),
        ("换错相邻键", "wpmen", "women", "我们"),
        ("换错相邻键", "jintiqn", "jintian", "今天"),
        ("多敲", "wommen", "women", "我们"),
        ("多敲", "zhonggwen", "zhongwen", "中文"),
        ("多敲", "rennmin", "renmin", "人民"),
        ("漏敲", "zhonwen", "zhongwen", "中文"),
        ("漏敲", "gonzuo", "gongzuo", "工作"),
        ("漏敲", "chifn", "chifan", "吃饭"),
        ("颠倒", "mingtain", "mingtian", "明天"),
        ("颠倒", "zhognwen", "zhongwen", "中文"),
        ("颠倒", "xuexaio", "xuexiao", "学校"),
        ("多处", "zhonwenshuruaf", "zhongwenshurufa", "中文输入法"),
        ("多处", "xuexioagonzuo", "xuexiaogongzuo", "学校工作"),
        (
            "多处",
            "jintinwomenchifn",
            "jintianwomenchifan",
            "今天我们吃饭",
        ),
    ];
    let mut engine = Engine::new(typo_dictionary());
    for (kind, typed, corrected, text) in cases {
        engine.set_input(typed);
        let query = engine.query().unwrap();
        let correction = query
            .correction
            .as_ref()
            .unwrap_or_else(|| panic!("{kind} {typed} 没有纠正"));
        assert_eq!(correction.corrected, *corrected, "{kind} {typed}");
        let first = &query.candidates.items[0];
        assert_eq!(first.text, *text, "{kind} {typed}");
        assert!(first.corrected, "{kind} {typed} 首选没标「纠」");
    }
}

/// 敲对的拼音不纠、不标「纠」：误纠是比漏纠更伤人的错。
#[test]
fn correct_input_is_never_corrected_or_marked() {
    let mut engine = Engine::new(typo_dictionary());
    for typed in [
        "nihao",
        "nihaoma",
        "jintian",
        "mingtian",
        "xuexiao",
        "zhongwen",
        "zhongwenshurufa",
        "womenchifan",
        "gongzuo",
        "kaifa",
        "renmin",
        "jintianwomenchif",
    ] {
        engine.set_input(typed);
        let query = engine.query().unwrap();
        assert!(query.correction.is_none(), "{typed} 被纠了");
        assert!(
            query.candidates.items.iter().all(|c| !c.corrected),
            "{typed} 有候选标了「纠」"
        );
    }
}

/// 整串替换的纠正里，只盖住前半截的词（`你好` 对 `nihooma`）不标「纠」：标记只给真正覆盖整串的那条。
#[test]
fn only_candidates_covering_the_whole_corrected_string_are_marked() {
    let mut engine = Engine::new(typo_dictionary());
    engine.set_input("nihooma");
    let query = engine.query().unwrap();
    let marked: Vec<&str> = query
        .candidates
        .items
        .iter()
        .filter(|c| c.corrected)
        .map(|c| c.text.as_str())
        .collect();
    assert_eq!(marked, ["你好吗"]);
    assert!(
        query
            .candidates
            .items
            .iter()
            .any(|c| c.text == "你好" && !c.corrected)
    );
}

/// 原串没有覆盖整串的精确候选：纠正候选放第 1 位；原串合法且有精确候选：纠正候选最多第 2 位。
/// 个人敲错表里同一对 (敲的, 要的) 接受过两次以上算高把握，放回第 1 位。
#[test]
fn correction_slot_follows_how_plausible_the_original_is() {
    // 没有 meiganxi 的原样候选：没关系 靠敲错边读出，放第 1 位
    let dictionary = Dictionary::parse(
        "没关系\tmei guan xi\t800000\n关系\tguan xi\t500000\n没\tmei\t900000\n干\tgan\t200000\n\
         洗\txi\t100000\n美感\tmei gan\t300000\n",
    )
    .unwrap();
    let mut engine = Engine::new(dictionary);
    engine.set_input("meiganxi");
    let query = engine.query().unwrap();
    assert_eq!(query.candidates.items[0].text, "没关系");
    assert!(query.candidates.items[0].corrected);

    // jineng 原样有 技能：近藤 最多第 2 位
    let dictionary = Dictionary::parse(
        "技能\tji neng\t300000\n近藤\tjin teng\t900000\n近\tjin\t500000\n机\tji\t400000\n能\tneng\t600000\n",
    )
    .unwrap();
    let mut engine =
        Engine::new(dictionary).with_learner(Box::new(CountingLearner(HashMap::new())));
    engine.set_input("jineng");
    let query = engine.query().unwrap();
    assert_eq!(query.candidates.items[0].text, "技能");
    assert_eq!(query.candidates.items[1].text, "近藤");
    assert!(query.candidates.items[1].corrected);

    // 同一对敲错被接受两次：高把握，纠正放第 1 位，原样退到第 2
    engine.learner_mut().record_typo("eng", "teng");
    engine.set_input("jineng");
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "技能");
    engine.learner_mut().record_typo("eng", "teng");
    engine.set_input("jineng");
    let query = engine.query().unwrap();
    assert_eq!(query.candidates.items[0].text, "近藤");
    assert!(query.candidates.items[0].corrected);
    assert_eq!(query.candidates.items[1].text, "技能");
}

/// 学习闭环：纠正放第 1 位却被选回原串，连续两次后不再自动放第 1 位；选了纠正候选则清零；回车原样上屏过的串不再纠。
#[test]
fn rejecting_the_correction_twice_demotes_it_and_enter_stops_it() {
    let dictionary = Dictionary::parse(
        "技能\tji neng\t300000\n近藤\tjin teng\t900000\n近\tjin\t500000\n机\tji\t400000\n能\tneng\t600000\n",
    )
    .unwrap();
    let mut engine =
        Engine::new(dictionary).with_learner(Box::new(CountingLearner(HashMap::new())));
    engine.learner_mut().record_typo("eng", "teng");
    engine.learner_mut().record_typo("eng", "teng");
    let pick = |engine: &mut Engine, text: &str| {
        engine.set_input("jineng");
        let item = engine
            .query()
            .unwrap()
            .candidates
            .items
            .into_iter()
            .find(|c| c.text == text)
            .unwrap();
        engine.commit(&item);
    };
    engine.set_input("jineng");
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "近藤");
    // 第一次选回原串：还放第 1 位
    pick(&mut engine, "技能");
    assert_eq!(engine.learner().rejected_correction_count("jineng"), 1);
    engine.set_input("jineng");
    assert_eq!(engine.query().unwrap().candidates.items[0].text, "近藤");
    // 选纠正候选：清零
    pick(&mut engine, "近藤");
    assert_eq!(engine.learner().rejected_correction_count("jineng"), 0);
    // 连续两次选回原串：不再放第 1 位
    pick(&mut engine, "技能");
    pick(&mut engine, "技能");
    assert_eq!(engine.learner().rejected_correction_count("jineng"), 2);
    engine.set_input("jineng");
    let query = engine.query().unwrap();
    assert_eq!(query.candidates.items[0].text, "技能");
    assert_eq!(query.candidates.items[1].text, "近藤");
    // 只上屏前半截不算表态
    engine.set_input("jineng");
    let ji = engine
        .query()
        .unwrap()
        .candidates
        .items
        .into_iter()
        .find(|c| c.text == "机")
        .unwrap();
    engine.commit(&ji);
    assert_eq!(engine.learner().rejected_correction_count("jineng"), 2);
    engine.clear();

    // 摆出了纠正候选而用户回车要原样：以后这个串不再猜（词图里也不再加敲错边）
    let dictionary = Dictionary::parse(
        "没关系\tmei guan xi\t800000\n关系\tguan xi\t500000\n没\tmei\t900000\n干\tgan\t200000\n\
         洗\txi\t100000\n美感\tmei gan\t300000\n",
    )
    .unwrap();
    let mut engine =
        Engine::new(dictionary).with_learner(Box::new(CountingLearner(HashMap::new())));
    engine.set_input("meiganxi");
    assert!(engine.query().unwrap().candidates.items[0].corrected);
    assert_eq!(engine.take_raw(), "meiganxi");
    engine.set_input("meiganxi");
    let query = engine.query().unwrap();
    assert!(query.candidates.items.iter().all(|c| !c.corrected));
    assert_ne!(query.candidates.items[0].text, "没关系");
}
