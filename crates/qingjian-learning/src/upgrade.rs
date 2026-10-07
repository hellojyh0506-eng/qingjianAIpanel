//! B2 → C1 表达升级系统 (B2 to C1 Expression Upgrade System)
//!
//! 核心原则：
//! 1. 严格区分语法正确性 (Correctness) 与表达高级度/地道度 (Sophistication/Naturalness)；
//! 2. 绝不允许把“简单但正确”判定为错误 (Anti-overcorrection)；
//! 3. 语域感知 (Register)：Casual, Natural, Professional, Formal, Diplomatic, Playful, Direct, Soft；
//! 4. 语用学 (Pragmatics)：同一含义在不同社交场景下的表达感觉与得体性；
//! 5. 沉淀结构化数据供 Learn 模式展开与个人学习画像追踪。

use qingjian_core::{
    ComparisonResult, ComparisonStatus, EvaluationFourDimensions, ExpressionUpgradeRecord,
    PragmaticNuance, Register, Sophistication,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaticPragmaticNuance {
    pub register: Register,
    pub expression: &'static str,
    pub scenario: &'static str,
    pub feeling: &'static str,
}

impl StaticPragmaticNuance {
    pub fn to_nuance(&self) -> PragmaticNuance {
        PragmaticNuance {
            register: self.register,
            expression: self.expression.to_string(),
            scenario: self.scenario.to_string(),
            feeling: self.feeling.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaticUpgradeRecord {
    pub prompt: &'static str,
    pub b2_baseline: &'static str,
    pub natural_expression: &'static str,
    pub c1_expression: &'static str,
    pub register_variants: &'static [StaticPragmaticNuance],
    pub why_better: &'static str,
    pub nuance_explanation: &'static str,
}

impl StaticUpgradeRecord {
    pub fn to_record(&self) -> ExpressionUpgradeRecord {
        ExpressionUpgradeRecord {
            prompt: self.prompt.to_string(),
            b2_baseline: self.b2_baseline.to_string(),
            natural_expression: self.natural_expression.to_string(),
            c1_expression: self.c1_expression.to_string(),
            register_variants: self.register_variants.iter().map(|v| v.to_nuance()).collect(),
            why_better: self.why_better.to_string(),
            nuance_explanation: self.nuance_explanation.to_string(),
        }
    }
}

/// 获取全部 30 个官方精选高价值 B2 → C1 中文场景表达升级基准库
pub fn get_upgrade_database() -> Vec<ExpressionUpgradeRecord> {
    UPGRADE_DB.iter().map(|s| s.to_record()).collect()
}

/// 根据中文场景或英文参考表达检索升级记录
pub fn find_upgrade(prompt: &str, reference: &str) -> Option<ExpressionUpgradeRecord> {
    let p_clean = prompt.trim().trim_matches(['。', '，', '！', '？', '.', ',', '!', '?']);
    let r_norm = normalize(reference);

    // 1. 优先按 prompt 精确或高重合包含匹配
    for record in UPGRADE_DB {
        let rec_p = record.prompt.trim();
        if rec_p == p_clean {
            return Some(record.to_record());
        }
        if (p_clean.len() >= 6 || rec_p.len() >= 6)
            && (p_clean.contains(rec_p) || rec_p.contains(p_clean))
        {
            return Some(record.to_record());
        }
    }

    // 3. 按参考表达匹配 (c1, natural, b2, 或 variants)
    if !r_norm.is_empty() {
        for record in UPGRADE_DB {
            let c1_norm = normalize(record.c1_expression);
            let nat_norm = normalize(record.natural_expression);
            let b2_norm = normalize(record.b2_baseline);

            if c1_norm == r_norm || c1_norm.contains(&r_norm) || r_norm.contains(&c1_norm)
                || nat_norm == r_norm || nat_norm.contains(&r_norm) || r_norm.contains(&nat_norm)
                || b2_norm == r_norm || b2_norm.contains(&r_norm) || r_norm.contains(&b2_norm)
            {
                return Some(record.to_record());
            }

            for v in record.register_variants {
                let v_norm = normalize(v.expression);
                if v_norm == r_norm || v_norm.contains(&r_norm) || r_norm.contains(&v_norm) {
                    return Some(record.to_record());
                }
            }

            // 核心短语词汇交集
            let r_tokens: Vec<&str> = r_norm.split_whitespace().collect();
            let c1_tokens: Vec<&str> = c1_norm.split_whitespace().collect();
            let common_tokens = r_tokens.iter().filter(|t| t.len() >= 4 && c1_tokens.contains(t)).count();
            if common_tokens >= 2 || (common_tokens >= 1 && (r_norm.contains("challenging") || r_norm.contains("swamped") || r_norm.contains("tackle") || r_norm.contains("elaborate"))) {
                return Some(record.to_record());
            }
        }
    }

    None
}

/// 评估用户输入是否为合法且正确的 B2 表达，并生成 B2 → C1 升级结果
pub fn evaluate_b2_upgrade(
    prompt: &str,
    user_input: &str,
    reference: &str,
) -> Option<ComparisonResult> {
    let upgrade = find_upgrade(prompt, reference)?;
    let norm_input = normalize(user_input);

    if norm_input.is_empty() {
        return None;
    }

    // 1. 精确匹配 C1 或 Natural
    let norm_c1 = normalize(&upgrade.c1_expression);
    let norm_nat = normalize(&upgrade.natural_expression);
    if norm_input == norm_c1 || norm_input == norm_nat {
        return Some(ComparisonResult {
            status: Some(ComparisonStatus::ExactMatch),
            key_advice: "太棒了！你的表达已达到母语者 C1 高阶地道水准，用词与语调极为自然。".to_string(),
            user_formatted: user_input.trim().to_string(),
            natural_expression: upgrade.c1_expression.clone(),
            primary_category: None,
            feedback_items: Vec::new(),
            upgrade: Some(upgrade.clone()),
            dimensions: Some(EvaluationFourDimensions {
                correctness: true,
                naturalness_score: 5,
                sophistication: Sophistication::C1,
                detected_register: Register::Natural,
                upgrade_advice: Some("已达 C1 高阶水平，完美契合母语者习惯！".to_string()),
            }),
        });
    }

    // 2. 精确匹配语域变体 (Register Variant)
    for variant in &upgrade.register_variants {
        let norm_var = normalize(&variant.expression);
        if norm_input == norm_var {
            return Some(ComparisonResult {
                status: Some(ComparisonStatus::ExactMatch),
                key_advice: format!(
                    "极佳表达！完美切合【{}】语域，适合 {}",
                    variant.register.label(),
                    variant.scenario
                ),
                user_formatted: user_input.trim().to_string(),
                natural_expression: variant.expression.clone(),
                primary_category: None,
                feedback_items: Vec::new(),
                upgrade: Some(upgrade.clone()),
                dimensions: Some(EvaluationFourDimensions {
                    correctness: true,
                    naturalness_score: 5,
                    sophistication: Sophistication::C1,
                    detected_register: variant.register,
                    upgrade_advice: Some(format!("精准命中 {} 语域风格！", variant.register.label())),
                }),
            });
        }
    }

    // 3. 精确匹配 B2 基础表达 (用户明确输入了基准 B2 句子，绝不能被后续模糊匹配拦截)
    let norm_b2 = normalize(&upgrade.b2_baseline);
    let make_b2_result = || {
        let detected_reg = if norm_input.contains("wrong") || norm_input.contains("must") {
            Register::Direct
        } else {
            Register::Natural
        };
        ComparisonResult {
            status: Some(ComparisonStatus::ValidB2Upgrade),
            key_advice: format!(
                "【语法语义完全正确 (B2)】\n✨ C1进阶推荐: \"{}\"\n💡 升级理由: {}",
                upgrade.c1_expression, upgrade.why_better
            ),
            user_formatted: user_input.trim().to_string(),
            natural_expression: upgrade.natural_expression.clone(),
            primary_category: Some(qingjian_core::ErrorMajorCategory::Naturalness),
            feedback_items: vec![qingjian_core::FeedbackItem {
                major_category: qingjian_core::ErrorMajorCategory::Naturalness,
                subcategory: qingjian_core::ErrorSubcategory::AwkwardPhrasing,
                label: "地道表达升级".to_string(),
                explanation: format!("{}（推荐升级到 C1: \"{}\"）", upgrade.why_better, upgrade.c1_expression),
                suggestion: Some(upgrade.c1_expression.clone()),
            }],
            upgrade: Some(upgrade.clone()),
            dimensions: Some(EvaluationFourDimensions {
                correctness: true,
                naturalness_score: 4,
                sophistication: Sophistication::B2,
                detected_register: detected_reg,
                upgrade_advice: Some(format!(
                    "你的表达语法语义完全正确 (B2)。进阶地道表达推荐：\"{}\"",
                    upgrade.c1_expression
                )),
            }),
        }
    };

    if norm_input == norm_b2 {
        return Some(make_b2_result());
    }

    // 4. 语域变体部分包含或高度匹配 (仅在非 B2 基准输入时)
    for variant in &upgrade.register_variants {
        let norm_var = normalize(&variant.expression);
        if (norm_var.contains(&norm_input) && norm_input.len() >= 12)
            || (norm_input.contains(&norm_var) && norm_var.len() >= 12)
            || token_similarity(&norm_input, &norm_var) >= 0.82
        {
            return Some(ComparisonResult {
                status: Some(ComparisonStatus::ExactMatch),
                key_advice: format!(
                    "极佳表达！完美切合【{}】语域，适合 {}",
                    variant.register.label(),
                    variant.scenario
                ),
                user_formatted: user_input.trim().to_string(),
                natural_expression: variant.expression.clone(),
                primary_category: None,
                feedback_items: Vec::new(),
                upgrade: Some(upgrade.clone()),
                dimensions: Some(EvaluationFourDimensions {
                    correctness: true,
                    naturalness_score: 5,
                    sophistication: Sophistication::C1,
                    detected_register: variant.register,
                    upgrade_advice: Some(format!("精准命中 {} 语域风格！", variant.register.label())),
                }),
            });
        }
    }

    // 5. C1 模糊/包含匹配 (在没有命中任何变体且非 B2 基准时)
    if (norm_c1.contains(&norm_input) && norm_input.len() >= 12)
        || (norm_input.contains(&norm_c1) && norm_c1.len() >= 12)
        || token_similarity(&norm_input, &norm_c1) >= 0.82
    {
        return Some(ComparisonResult {
            status: Some(ComparisonStatus::ExactMatch),
            key_advice: "太棒了！你的表达已达到母语者 C1 高阶地道水准，用词与语调极为自然。".to_string(),
            user_formatted: user_input.trim().to_string(),
            natural_expression: upgrade.c1_expression.clone(),
            primary_category: None,
            feedback_items: Vec::new(),
            upgrade: Some(upgrade.clone()),
            dimensions: Some(EvaluationFourDimensions {
                correctness: true,
                naturalness_score: 5,
                sophistication: Sophistication::C1,
                detected_register: Register::Natural,
                upgrade_advice: Some("已达 C1 高阶水平，完美契合母语者习惯！".to_string()),
            }),
        });
    }

    // 6. 检查用户输入是否为可接受的 B2 变体表达
    if is_acceptable_b2_variant(&norm_input, &norm_b2) {
        return Some(make_b2_result());
    }

    None
}

fn normalize(s: &str) -> String {
    s.trim()
        .to_ascii_lowercase()
        .replace(['.', ',', '!', '?', ';', ':', '\'', '"', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn token_similarity(a: &str, b: &str) -> f32 {
    let ta: Vec<&str> = a.split_whitespace().collect();
    let tb: Vec<&str> = b.split_whitespace().collect();
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let matched = ta.iter().filter(|t| tb.contains(t)).count();
    (matched as f32) / (ta.len().max(tb.len()) as f32)
}

fn is_acceptable_b2_variant(input: &str, b2: &str) -> bool {
    if input == b2 {
        return true;
    }
    let tokens_in: Vec<&str> = input.split_whitespace().collect();
    let tokens_b2: Vec<&str> = b2.split_whitespace().collect();

    if tokens_in.len() == tokens_b2.len() {
        let diffs = tokens_in.iter().zip(tokens_b2.iter()).filter(|(a, b)| a != b).count();
        if diffs <= 1 {
            return true;
        }
    }

    if (b2.contains(input) && tokens_in.len() >= 3) || (input.contains(b2) && tokens_b2.len() >= 3) {
        return true;
    }

    if token_similarity(input, b2) >= 0.6 {
        return true;
    }

    false
}

// ---------------------------------------------------------------------------
// 30 个高质量真实中英文 B2 → C1 表达升级知识库
// ---------------------------------------------------------------------------

static UPGRADE_DB: &[StaticUpgradeRecord] = &[
    // 1. 觉得这事很难
    StaticUpgradeRecord {
        prompt: "觉得这事很难",
        b2_baseline: "I think this is very difficult.",
        natural_expression: "I think this is pretty challenging.",
        c1_expression: "I think this could be quite challenging.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "This might be a bit tricky.",
                scenario: "朋友闲聊 / 轻松讨论",
                feeling: "轻松随和，不给对方施加沉重压力",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "This presents some significant challenges.",
                scenario: "职场汇报 / 项目评估",
                feeling: "客观专业，体现严谨的风险意识",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "It might require a bit more effort than we initially anticipated.",
                scenario: "跨部门协商 / 客户沟通",
                feeling: "委婉周全，留有余地，避免引发对立",
            },
            StaticPragmaticNuance {
                register: Register::Direct,
                expression: "This is going to be really tough.",
                scenario: "紧急团队攻坚",
                feeling: "直率有力，强调攻关紧迫性",
            },
        ],
        why_better: "避免过度依赖单调的 intensifier 'very' + 'difficult'；采用 'challenging' 体现建设性视角，搭配情态动词 'could be' 提升语气的客观度与柔韧性。",
        nuance_explanation: "'very difficult' 偏字面生硬；'pretty challenging' 自然口语；'could be quite challenging' 尽显 C1 的从容与客观。",
    },

    // 2. 你错了
    StaticUpgradeRecord {
        prompt: "你错了",
        b2_baseline: "You are wrong.",
        natural_expression: "I don't think that's right.",
        c1_expression: "I'm not sure that's entirely accurate.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Soft,
                expression: "I don't think that's quite right.",
                scenario: "朋友日常探讨",
                feeling: "温和指出不同看法，维系融洽氛围",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "I'm not sure I agree with that.",
                scenario: "商务谈判 / 会议沟通",
                feeling: "表达不同意见而不具人身攻击性",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "That doesn't quite align with the data we have.",
                scenario: "工作复盘 / 事实对齐",
                feeling: "对事不对人，以客观事实与数据为依托",
            },
            StaticPragmaticNuance {
                register: Register::Direct,
                expression: "You're mistaken on that point.",
                scenario: "严肃纠错",
                feeling: "语气明确坚定，直指谬误",
            },
        ],
        why_better: "在人际与跨文化沟通中直接说 'You are wrong' 极具攻击性；使用 'I'm not sure...' 将焦点从指责对方转向表达己方疑虑，高级且得体。",
        nuance_explanation: "'You are wrong' 容易激化情绪；'I don't think that's quite right' 温和礼貌；'not entirely accurate' 兼顾各方体面。",
    },

    // 3. 我同意你的看法
    StaticUpgradeRecord {
        prompt: "我同意你的看法",
        b2_baseline: "I agree with you.",
        natural_expression: "I'm on board with that.",
        c1_expression: "I completely share your perspective on this.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "Totally, I'm with you on that.",
                scenario: "日常随聊赞同",
                feeling: "默契亲近，表达毫无保留的认同",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "I'm fully aligned with that approach.",
                scenario: "业务对齐 / 架构决策",
                feeling: "职场利落规范，体现团队一致性",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "That resonates well with our overall direction.",
                scenario: "多方协调 / 宏观战略",
                feeling: "站在大局高度表态，升华提议价值",
            },
        ],
        why_better: "超越单一平淡的 'agree'；在职场中使用 'aligned with' 或 'share your perspective' 展现成熟职业素养与多元协作能力。",
        nuance_explanation: "'I agree' 略显机械单薄；'I'm on board' 强调积极参与；'fully aligned' 是科技企业最高频地道认同。",
    },

    // 4. 我们需要解决这个问题
    StaticUpgradeRecord {
        prompt: "我们需要解决这个问题",
        b2_baseline: "We need to solve this problem.",
        natural_expression: "We need to figure this out.",
        c1_expression: "We need to tackle this issue.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "Let's sort this out.",
                scenario: "协同排查 / 日常事务",
                feeling: "通俗务实，轻松推进问题理顺",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "We need to address this bottleneck.",
                scenario: "研发攻坚 / 系统瓶颈",
                feeling: "精准切中主要矛盾，展现架构深度",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "We should look into a viable resolution here.",
                scenario: "利益相关方协调",
                feeling: "稳妥探寻各方接受的可行方案",
            },
        ],
        why_better: "'solve the problem' 是典型的应试英语套话；母语者口语常用短语动词 'figure this out / sort out'，高阶职场偏好动态词汇 'tackle / address'。",
        nuance_explanation: "'solve' 偏数学解题；'tackle' 展现主动出击的担当；'address' 强调系统性应对。",
    },

    // 5. 我最近太忙了
    StaticUpgradeRecord {
        prompt: "我最近太忙了",
        b2_baseline: "I am very busy lately.",
        natural_expression: "I've been super busy lately.",
        c1_expression: "I've been completely swamped lately.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "My hands are totally full right now.",
                scenario: "熟人闲聊 / 微信交流",
                feeling: "形象生动，自然表达事务繁重",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "I'm currently at capacity with several deliverables.",
                scenario: "职场挡需求 / 沟通排期",
                feeling: "专业得体，客观说明工作负荷饱和",
            },
            StaticPragmaticNuance {
                register: Register::Playful,
                expression: "I'm drowning in work over here!",
                scenario: "同事吐槽 / 亲近伙伴",
                feeling: "诙谐自嘲，增进情感联结",
            },
        ],
        why_better: "用生动的比喻词 'swamped' (被事务淹没) 或外企专业表达 'at capacity' 替代苍白的 'very busy'，表现力成倍提升。",
        nuance_explanation: "'very busy' 平淡机械；'swamped' 是纯正地道口语；'at capacity' 是高阶职场优雅拒绝加塞的利器。",
    },

    // 6. 你能帮我做这个吗
    StaticUpgradeRecord {
        prompt: "你能帮我做这个吗",
        b2_baseline: "Can you help me do this?",
        natural_expression: "Could you give me a hand with this?",
        c1_expression: "Would you mind lending me a hand with this when you have a moment?",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "Can you help me out for a sec?",
                scenario: "顺手求助 / 同事互助",
                feeling: "顺手帮忙，毫无心理负担",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "Would you have bandwidth to assist with this?",
                scenario: "跨团队协同 / 资源协调",
                feeling: "充分尊重对方工作负荷与安排",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "If your schedule allows, I'd really appreciate your input on this.",
                scenario: "向前辈/专家请教",
                feeling: "谦逊恭谨，给对方保留充分退路",
            },
        ],
        why_better: "初阶直译 'Can you help me' 稍显生硬命令感；C1 擅长加入 'Would you mind' 与缓冲状语 'when you have a moment'，极具情商。",
        nuance_explanation: "'Can you' 较生硬；'give me a hand' 亲切地道；'have bandwidth' 是科技大厂必备职业素养词汇。",
    },

    // 7. 我不知道
    StaticUpgradeRecord {
        prompt: "我不知道",
        b2_baseline: "I don't know.",
        natural_expression: "I'm not really sure.",
        c1_expression: "I don't have that information off the top of my head.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "No clue, honestly.",
                scenario: "朋友闲聊",
                feeling: "坦诚随性，真实可信",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "I'll look into it and circle back with you.",
                scenario: "会议答疑 / 客户提问",
                feeling: "不说不知道，展现主动跟进解决的专业作风",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "I can't say for certain at this moment.",
                scenario: "严谨场合 / 方案答辩",
                feeling: "严谨审慎，不轻易给出无凭据结论",
            },
        ],
        why_better: "直接甩出 'I don't know' 容易被解读为冷漠或不负责任；C1 表达既说明现状，又给出主动跟进承诺 ('circle back')。",
        nuance_explanation: "'I don't know' 终结对话；'off the top of my head' 表达需核实；'circle back' 展现行动力。",
    },

    // 8. 等等我
    StaticUpgradeRecord {
        prompt: "等等我",
        b2_baseline: "Wait for me.",
        natural_expression: "Hang on a second.",
        c1_expression: "Give me just a second to catch up.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "Hold on, I'm coming!",
                scenario: "出行散步 / 朋友同行",
                feeling: "亲近自然，脱口而出",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "Bear with me for a moment while I pull this up.",
                scenario: "会议演示 / 调取资料",
                feeling: "沉稳得体，专业安抚听众",
            },
            StaticPragmaticNuance {
                register: Register::Playful,
                expression: "Don't leave me in the dust!",
                scenario: "跑步嬉闹 / 朋友聚会",
                feeling: "俏皮生动，极具生活趣味",
            },
        ],
        why_better: "口语中很少像机器人一样命令 'Wait for me'；母语者偏好短语动词 'hang on / hold on' 或社交安抚利器 'bear with me'。",
        nuance_explanation: "'Wait for me' 像小孩子直译；'hang on' 自然口语；'bear with me' 适合一切演讲与汇报场合。",
    },

    // 9. 我不同意
    StaticUpgradeRecord {
        prompt: "我不同意",
        b2_baseline: "I disagree.",
        natural_expression: "I see it a bit differently.",
        c1_expression: "I have a slightly different take on this.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "I don't know about that.",
                scenario: "闲聊微争论",
                feeling: "轻松表达保留意见，不伤和气",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "From my perspective, there's another angle to consider.",
                scenario: "方案评审 / 头脑风暴",
                feeling: "建设性补充视角，而非直接否定",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "I appreciate your point, though I see a potential risk.",
                scenario: "高管沟通 / 商务对齐",
                feeling: "先肯定再转折，体现顶级包容心与洞察力",
            },
        ],
        why_better: "直接抛出 'I disagree' 将对话瞬间降维成争辩；C1 使用 'different take / another angle' 将异见包装为高价值增量视角。",
        nuance_explanation: "'I disagree' 关上大门；'different take' 启发新思考；'another angle' 体现成熟多元思维。",
    },

    // 10. 随便
    StaticUpgradeRecord {
        prompt: "随便",
        b2_baseline: "Whatever.",
        natural_expression: "Either is fine with me.",
        c1_expression: "I'm easy either way; whatever works best for you.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "Up to you, I'm good with anything.",
                scenario: "选餐馆 / 挑选活动",
                feeling: "随和开朗，易于相处",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "I'm open to whichever option suits the team better.",
                scenario: "方案二选一决策",
                feeling: "以团队整体利益为重，大度配合",
            },
            StaticPragmaticNuance {
                register: Register::Soft,
                expression: "I don't have a strong preference.",
                scenario: "征询意向",
                feeling: "温和客观，把选择权赋予对方",
            },
        ],
        why_better: "中文的“随便”若直译为 'Whatever'，在美语中是极具冒犯性和敌意的反击；地道表达应用 'I'm easy either way' 或 'Up to you'。",
        nuance_explanation: "'Whatever' 包含‘关我屁事’的粗鲁感；'I'm easy' 则散发温暖随和、为他人着想的修养。",
    },

    // 11. 请说大声点
    StaticUpgradeRecord {
        prompt: "请说大声点",
        b2_baseline: "Please speak louder.",
        natural_expression: "Could you speak up a little?",
        c1_expression: "You're coming through a bit faint; could you speak up slightly?",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "Sorry, you're breaking up a bit!",
                scenario: "网络电话 / 微信通话",
                feeling: "自然将问题归因于网络信号",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "The audio is a bit low on my end, would you mind turning up your mic?",
                scenario: "跨国远程会议",
                feeling: "客气指出设备问题，避免让对方尴尬",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "I want to make sure I don't miss anything you're sharing.",
                scenario: "正式演讲 / 听众互动",
                feeling: "极度尊重说话者，凸显内容的重要性",
            },
        ],
        why_better: "'Please speak louder' 像在命令对方大喊；C1 用短语动词 'speak up'，配合将原因归咎于环境/设备 ('on my end / faint')。",
        nuance_explanation: "'speak louder' 突兀生硬；'speak up' 亲切地道；'coming through faint' 细腻体面。",
    },

    // 12. 我觉得这个方案不好
    StaticUpgradeRecord {
        prompt: "我觉得这个方案不好",
        b2_baseline: "I think this plan is not good.",
        natural_expression: "I'm not sure this plan will work.",
        c1_expression: "I have a few reservations about this proposal.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "I don't think this is our best bet.",
                scenario: "内部头脑风暴",
                feeling: "借用博弈比喻，通俗有力切中要害",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "There are a few trade-offs here that warrant closer scrutiny.",
                scenario: "架构评审 / 技术推演",
                feeling: "资深技术架构师水准，讲权衡取舍与严密审视",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "While I appreciate the intent, I wonder if the timeline is feasible.",
                scenario: "管理层战略推演",
                feeling: "先肯定初衷愿景，再聚焦可行性风险",
            },
        ],
        why_better: "'not good' 词汇匮乏且极为主观；C1 会用专业名词 'reservations' (保留意见) 或 'trade-offs' (权衡取舍)，言之有物。",
        nuance_explanation: "'not good' 像小学生打分；'have reservations' 是高阶母语标配；'warrant closer scrutiny' 具备学术级洞察力。",
    },

    // 13. 你可以走了
    StaticUpgradeRecord {
        prompt: "你可以走了",
        b2_baseline: "You can go.",
        natural_expression: "You're good to go.",
        c1_expression: "I won't keep you any longer; feel free to head out.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "All set, you're free to bounce!",
                scenario: "朋友聚会散场",
                feeling: "俏皮轻快，气氛活跃",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "That covers everything on my end, so I'll give you some time back.",
                scenario: "会议提前结束",
                feeling: "职场顶级礼貌，体贴归还对方时间",
            },
            StaticPragmaticNuance {
                register: Register::Soft,
                expression: "Thanks so much for your time, that's all I needed.",
                scenario: "问询/审核完毕",
                feeling: "致谢体面，毫无生硬逐客感",
            },
        ],
        why_better: "'You can go' 听起来像居高临下的逐客令；母语者偏好温馨利索的 'You're good to go' 或贴心的 'give you some time back'。",
        nuance_explanation: "'You can go' 让人心慌尴尬；'You're good to go' 温暖清爽；'give you time back' 尽显顶级外企职业修养。",
    },

    // 14. 我知道了
    StaticUpgradeRecord {
        prompt: "我知道了",
        b2_baseline: "I know.",
        natural_expression: "Got it.",
        c1_expression: "Understood, thanks for the heads-up.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "Gotcha!",
                scenario: "口头快速确认",
                feeling: "爽朗利索，沟通无间",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "Noted with thanks; I'll take it from here.",
                scenario: "工作群接单 / 任务交接",
                feeling: "展现极强执行力与责任担当",
            },
            StaticPragmaticNuance {
                register: Register::Direct,
                expression: "Crystal clear.",
                scenario: "关键指令确认",
                feeling: "彻底领会，绝无歧义",
            },
        ],
        why_better: "中文的“我知道了”若译成 'I know'，常带有‘用不着你多管闲事’的不耐烦语气；地道表达应使用 'Got it / Noted / Understood'。",
        nuance_explanation: "'I know' 往往引发反感；'Got it' 干净利落；'thanks for the heads-up' 诚恳表达对提醒的感谢。",
    },

    // 15. 我有另一个想法
    StaticUpgradeRecord {
        prompt: "我有另一个想法",
        b2_baseline: "I have another idea.",
        natural_expression: "Here's an alternative.",
        c1_expression: "Here's a thought from a different angle.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "What if we try this instead?",
                scenario: "日常头脑风暴",
                feeling: "互动启发，毫无距离感",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "I'd like to propose an alternative approach.",
                scenario: "技术方案立项汇报",
                feeling: "严谨规范，条理清晰",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "Building on that, what if we also explored this option?",
                scenario: "承接前人发言",
                feeling: "借势拓展，肯定前人并升华方案",
            },
        ],
        why_better: "初学者用 'I have another idea' 像小学生举手发言；C1 善用 'Building on that' 或 'Here's a thought'，彰显自信与协作共赢。",
        nuance_explanation: "'I have an idea' 平铺直叙；'Here's a thought' 自信松弛；'Building on that' 极受团队欢迎。",
    },

    // 16. 我们尽快做
    StaticUpgradeRecord {
        prompt: "我们尽快做",
        b2_baseline: "We will do it as soon as possible.",
        natural_expression: "We'll get right on it.",
        c1_expression: "We'll prioritize this and have an update shortly.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "On it!",
                scenario: "快速确认需求",
                feeling: "雷厉风行，极具行动力",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "We'll treat this with high urgency.",
                scenario: "突发线上事故 / 故障响应",
                feeling: "沉着果断，带给客户强大确定性",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "We'll do our best to expedite the process.",
                scenario: "推进跨部门审批",
                feeling: "全力以赴但合理控制预期",
            },
        ],
        why_better: "'ASAP' 沦为泛滥空泛套话；C1 使用富有生机与具体行动承诺的 'get right on it / prioritize this / expedite'。",
        nuance_explanation: "'ASAP' 苍白无力；'On it' 是最帅气口语接单；'expedite' 是纯正高端商务用语。",
    },

    // 17. 别担心
    StaticUpgradeRecord {
        prompt: "别担心",
        b2_baseline: "Don't worry.",
        natural_expression: "No worries at all.",
        c1_expression: "Rest assured, we've got this fully under control.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "All good, you're fine!",
                scenario: "朋友小疏忽安慰",
                feeling: "大度包容，不留尴尬",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "Rest assured, we're monitoring the situation closely.",
                scenario: "业务服务监控汇报",
                feeling: "专业控场，带给上级完全安心感",
            },
            StaticPragmaticNuance {
                register: Register::Soft,
                expression: "Take your time, no rush whatsoever.",
                scenario: "安抚对方迟到/延期",
                feeling: "温暖体贴，有效卸下心理防备",
            },
        ],
        why_better: "'Don't worry' 听起来苍白；C1 用 'Rest assured / under control' 展现绝对的专业实力与强大的控场底气。",
        nuance_explanation: "'Don't worry' 略显敷衍；'No worries' 万能随和；'Rest assured' 属于资深专家级定心丸。",
    },

    // 18. 请尽快回复我
    StaticUpgradeRecord {
        prompt: "请尽快回复我",
        b2_baseline: "Please reply to me quickly.",
        natural_expression: "Let me know when you get a chance.",
        c1_expression: "I'd appreciate it if you could get back to me at your earliest convenience.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "Hit me back whenever you can.",
                scenario: "日常私聊留言",
                feeling: "随意亲密，无催迫感",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "Looking forward to hearing from you at your earliest convenience.",
                scenario: "商务往来邮件尾注",
                feeling: "规范优雅，催而不迫",
            },
            StaticPragmaticNuance {
                register: Register::Direct,
                expression: "A prompt response would be greatly appreciated.",
                scenario: "紧急事务催单",
                feeling: "礼貌但明确强调时效紧迫性",
            },
        ],
        why_better: "纠正 'reply me' 遗漏介词 'to' 的语法瑕疵，并用成熟地道的 'at your earliest convenience' 替代急躁生硬的 'quickly'。",
        nuance_explanation: "'reply quickly' 显得粗鲁焦虑；'get back to me' 亲切大方；'earliest convenience' 是外企邮件黄金典范。",
    },

    // 19. 我没听懂你的意思
    StaticUpgradeRecord {
        prompt: "我没听懂你的意思",
        b2_baseline: "I don't understand what you mean.",
        natural_expression: "I'm not following.",
        c1_expression: "Could you clarify what you mean by that?",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "Wait, what do you mean by that?",
                scenario: "闲聊跟不上节奏",
                feeling: "坦诚好奇，自然追问",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "Could you elaborate a bit more on that point?",
                scenario: "技术讲座 / 方案阐述",
                feeling: "启发对方展开深挖，展现求知欲",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "Just to ensure we're on the same page, are you saying...?",
                scenario: "需求确认对齐",
                feeling: "顶级高情商对齐，既澄清事实又保全对方体面",
            },
        ],
        why_better: "'I don't understand your meaning' 是中国学生最典型直译；母语者习惯说 'I'm not following'，把没跟上的责任归于自己，体贴入微。",
        nuance_explanation: "'understand your meaning' 极其别扭；'I'm not following' 归因自我；'on the same page' 是团队共识神器。",
    },

    // 20. 这没道理
    StaticUpgradeRecord {
        prompt: "这没道理",
        b2_baseline: "That makes no sense.",
        natural_expression: "That doesn't make any sense.",
        c1_expression: "That seems a bit counterintuitive.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "That makes zero sense!",
                scenario: "吐槽怪事",
                feeling: "情绪鲜明，脱口而出",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "The reasoning behind this seems a bit tenuous.",
                scenario: "学术/代码方案质疑",
                feeling: "逻辑严谨，指出论据支撑薄弱",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "I struggle to see the underlying logic here.",
                scenario: "委婉质疑方案",
                feeling: "陈述自己的困惑，避免直接判定对方愚蠢",
            },
        ],
        why_better: "纠正字面直译 'This has no reason'；C1 引入 'counterintuitive' (反直觉) 或 'tenuous' (牵强)，表达客观思辨深度。",
        nuance_explanation: "'has no reason' 是 Chinglish；'doesn't make sense' 通俗实用；'counterintuitive' 展现高雅思辨水准。",
    },

    // 21. 我想取消这个会议
    StaticUpgradeRecord {
        prompt: "我想取消这个会议",
        b2_baseline: "I want to cancel this meeting.",
        natural_expression: "Let's call off the meeting.",
        c1_expression: "I'm afraid we'll need to postpone this meeting.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "Let's skip today's sync.",
                scenario: "敏捷小团队例会",
                feeling: "轻快务实，不拖泥带水",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "Given the circumstances, I suggest we reschedule for later this week.",
                scenario: "正式商务改期",
                feeling: "说明背景并主动奉上备选改期方案",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "Something urgent came up; apologies for the last-minute change.",
                scenario: "临时突发变动致歉",
                feeling: "真诚周到，最大限度降低对方反感",
            },
        ],
        why_better: "单纯用 'cancel' 冰冷生硬且断绝后续；C1 偏向用动词短语 'call off' 或富于建设性的 'reschedule / postpone'。",
        nuance_explanation: "'cancel' 冰冷单向；'call off' 地道生动；'reschedule' 积极维护长期合作关系。",
    },

    // 22. 这取决于你的决定
    StaticUpgradeRecord {
        prompt: "这取决于你的决定",
        b2_baseline: "This depends on your decision.",
        natural_expression: "It's totally up to you.",
        c1_expression: "The ball is in your court on this one.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "Your call, boss!",
                scenario: "同事/朋友打趣定夺",
                feeling: "轻松赋权，完全尊重对方",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "We'll take our cue from your final decision.",
                scenario: "听候领导/客户最终指令",
                feeling: "纪律严明，紧密配合执行",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "We're ready to proceed whenever you give the green light.",
                scenario: "推进重大流程审批",
                feeling: "万事俱备，静候首肯",
            },
        ],
        why_better: "'depends on your decision' 语句平板；C1 巧妙融合高频习语 'The ball is in your court' 或 'take our cue from'，语言鲜活传神。",
        nuance_explanation: "'depends on' 较平淡；'up to you' 实用自然；'The ball is in your court' 尽显地道习语修养。",
    },

    // 23. 你明白我的意思吗
    StaticUpgradeRecord {
        prompt: "你明白我的意思吗",
        b2_baseline: "Do you understand what I mean?",
        natural_expression: "Does that make sense?",
        c1_expression: "Am I explaining this clearly?",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "You with me on this?",
                scenario: "即兴叙述中途确认",
                feeling: "亲切互动，确认听众同频",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "Does that align with your understanding?",
                scenario: "架构或业务需求复核",
                feeling: "平等对齐认知，绝无居高临下感",
            },
            StaticPragmaticNuance {
                register: Register::Soft,
                expression: "Let me know if I should clarify anything.",
                scenario: "复杂推演后关怀",
                feeling: "谦虚温和，主动承担解释成本",
            },
        ],
        why_better: "'Do you understand me?' 在英语文化中极具审讯与冒犯感；母语者永远问 'Does that make sense?' 或将理解成本归于自己的 'Am I explaining clearly?'。",
        nuance_explanation: "'Do you understand' 令人反感；'Does that make sense' 是最经典日常问询；'Am I explaining clearly' 是顶级情商典范。",
    },

    // 24. 我会尽量按时完成
    StaticUpgradeRecord {
        prompt: "我会尽量按时完成",
        b2_baseline: "I will try to finish on time.",
        natural_expression: "I'll do my best to hit the deadline.",
        c1_expression: "I'm fully committed to delivering this on schedule.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "I'll make sure it's done in time.",
                scenario: "口头爽快承诺",
                feeling: "随性有底气，给人踏实感",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "We are on track to meet the targeted milestone.",
                scenario: "项目周报 / 敏捷冲刺",
                feeling: "严谨规范的项目管理语言",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "Barring any unforeseen roadblocks, we should be right on schedule.",
                scenario: "稳妥留有余地的进度预警",
                feeling: "留出意外风险余地，严密周全",
            },
        ],
        why_better: "'try to finish' 听起来心虚且缺乏承诺力；C1 使用动词短语 'hit the deadline'、'committed to delivering' 或 'on track' 传达可靠专业信誉。",
        nuance_explanation: "'try to' 给人没把握感；'hit the deadline' 口语鲜活；'on track' 是敏捷管理全球标准口径。",
    },

    // 25. 这是个好消息
    StaticUpgradeRecord {
        prompt: "这是个好消息",
        b2_baseline: "This is good news.",
        natural_expression: "That's great news!",
        c1_expression: "That's quite an encouraging development.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "Awesome, love to hear that!",
                scenario: "同伴报喜庆祝",
                feeling: "热情真诚，高频共情",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "This marks a very positive milestone for our project.",
                scenario: "成果通报会",
                feeling: "赋予商业与里程碑高度，提振士气",
            },
            StaticPragmaticNuance {
                register: Register::Playful,
                expression: "Music to my ears!",
                scenario: "期盼已久的喜讯传来",
                feeling: "俏皮形象，生动鲜明",
            },
        ],
        why_better: "纠正 'a good news' 的不可数语法缺陷；将普通 'good news' 升维为富有洞察力的 'encouraging development' 或形象生动的 'Music to my ears'。",
        nuance_explanation: "'good news' 平凡常见；'encouraging development' 展现分析力；'love to hear that' 极富年轻人感染力。",
    },

    // 26. 我认为我们应该等等看
    StaticUpgradeRecord {
        prompt: "我认为我们应该等等看",
        b2_baseline: "I think we should wait and see.",
        natural_expression: "Let's hold off for a bit.",
        c1_expression: "I'd recommend taking a wait-and-see approach for now.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "Let's sit tight and see how things shake out.",
                scenario: "静观局势变化",
                feeling: "俚语生动，表现从容不乱阵脚",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "It would be prudent to defer our decision until more data emerges.",
                scenario: "投资 / 战略风控研判",
                feeling: "审慎睿智，彰显资深风控素养",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "Perhaps we could pause and reassess once the dust settles.",
                scenario: "争议风波后的建议",
                feeling: "缓和紧张对抗，借尘埃落定喻理",
            },
        ],
        why_better: "用短语动词 'hold off' 替换呆板的 'wait'；在宏观研判中使用 'prudent to defer' 或 'wait-and-see approach'，表达更成熟稳健。",
        nuance_explanation: "'wait and see' 偏口语；'sit tight' 充满生活气息；'prudent to defer' 尽显高管风范。",
    },

    // 27. 我完全忘了这件事
    StaticUpgradeRecord {
        prompt: "我完全忘了这件事",
        b2_baseline: "I completely forgot about this.",
        natural_expression: "It completely slipped my mind.",
        c1_expression: "It totally escaped my attention; thank you for bringing it up.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "My bad, it totally slipped my mind!",
                scenario: "朋友间小疏忽",
                feeling: "爽快认账，亲切坦诚",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "Apologies, that fell through the cracks amidst the recent launch.",
                scenario: "工作交接偶有疏漏",
                feeling: "解释客观大背景，诚恳担责并快速补救",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "Thank you for reminding me; I'll address it immediately.",
                scenario: "被客户或主管指出漏项",
                feeling: "化被动为主动，以雷厉风行赢回信任",
            },
        ],
        why_better: "'forgot this' 显得单薄粗心；母语者高频使用 'slipped my mind' 或 'fell through the cracks'，自然生动且体面客观。",
        nuance_explanation: "'I forgot' 冰冷生硬；'slipped my mind' 委婉自然；'fell through the cracks' 是职场失误解释的经典惯用语。",
    },

    // 28. 别灰心
    StaticUpgradeRecord {
        prompt: "别灰心",
        b2_baseline: "Don't be discouraged.",
        natural_expression: "Keep your chin up.",
        c1_expression: "Don't let this setback discourage you.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "Hang in there, buddy!",
                scenario: "宽慰失意好友",
                feeling: "哥们伙伴间的坚实打气与支撑",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "Treat this as a valuable learning opportunity.",
                scenario: "导师复盘项目挫折",
                feeling: "建设性重塑认知，将挫折升华为成长动力",
            },
            StaticPragmaticNuance {
                register: Register::Soft,
                expression: "You did your best, and that's what truly counts.",
                scenario: "抚慰失落同伴",
                feeling: "温暖体贴，提供情绪安全感",
            },
        ],
        why_better: "直译 'Don't be sad' 词汇匮乏且缺乏共情；地道英语习惯使用 'Keep your chin up'、'Hang in there' 或以理性视角重构 'Don't let this setback discourage you'。",
        nuance_explanation: "'Don't be sad' 显得苍白；'Keep your chin up' 纯正地道美式；'setback' 赋予逆境积极建设意义。",
    },

    // 29. 这是最重要的事情
    StaticUpgradeRecord {
        prompt: "这是最重要的事情",
        b2_baseline: "This is the most important thing.",
        natural_expression: "This is our top priority.",
        c1_expression: "This takes precedence over everything else.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "This is what matters most right now.",
                scenario: "日常闲聊点出重点",
                feeling: "直切本质，简明扼要",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "This should take priority on our roadmap.",
                scenario: "产品迭代规划 / 研发排期",
                feeling: "符合现代敏捷团队标准规划口吻",
            },
            StaticPragmaticNuance {
                register: Register::Direct,
                expression: "This is non-negotiable.",
                scenario: "强调关键底线与硬约束",
                feeling: "不容置疑，态度坚决明确",
            },
        ],
        why_better: "'the most important thing' 略显稚嫩平庸；母语者偏好高级搭配 'top priority'、'takes precedence over' 或铿锵有力的 'non-negotiable'。",
        nuance_explanation: "'important thing' 平淡无奇；'top priority' 规范地道；'take precedence' 尽显 C1 高雅典范。",
    },

    // 30. 我改变主意了
    StaticUpgradeRecord {
        prompt: "我改变主意了",
        b2_baseline: "I changed my mind.",
        natural_expression: "I've had a change of heart.",
        c1_expression: "Upon further reflection, I've reconsidered my position.",
        register_variants: &[
            StaticPragmaticNuance {
                register: Register::Casual,
                expression: "Actually, scratch that—let's do this instead.",
                scenario: "随性快速转向",
                feeling: "雷厉风行，不拘泥于原计划",
            },
            StaticPragmaticNuance {
                register: Register::Professional,
                expression: "After weighing the recent data, I'm leaning toward a different option.",
                scenario: "依新数据调整方案决策",
                feeling: "数据驱动，客观理性做出调整",
            },
            StaticPragmaticNuance {
                register: Register::Diplomatic,
                expression: "Having heard everyone's thoughts, I'm reconsidering my initial stance.",
                scenario: "团队民主探讨后妥协",
                feeling: "善于倾听兼收并蓄，极富领袖胸襟",
            },
        ],
        why_better: "纠正 'changed my idea' 的生硬中式搭配；在 C1 语境中使用 'Upon further reflection' 或地道俚语 'scratch that'，极具语言掌控力。",
        nuance_explanation: "'changed my idea' 搭配错误；'change of heart' 饱含温情；'Upon further reflection' 是高阶学者的慎思明辨。",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_30_scenarios_have_complete_pragmatics() {
        let db = get_upgrade_database();
        assert_eq!(db.len(), 30, "必须包含严格的 30 个官方 B2→C1 场景库");

        for record in db {
            assert!(!record.prompt.is_empty());
            assert!(!record.b2_baseline.is_empty());
            assert!(!record.natural_expression.is_empty());
            assert!(!record.c1_expression.is_empty());
            assert!(!record.why_better.is_empty());
            assert!(!record.nuance_explanation.is_empty());
            assert!(
                record.register_variants.len() >= 2,
                "每个场景至少具备 2 个语域变体：{}",
                record.prompt
            );
            for v in record.register_variants {
                assert!(!v.expression.is_empty());
                assert!(!v.scenario.is_empty());
                assert!(!v.feeling.is_empty());
            }
        }
    }

    #[test]
    fn test_b2_upgrade_distinguishes_correctness_from_sophistication() {
        // 用户输入语法完全正确的 B2: "I think this is very difficult."
        let res = evaluate_b2_upgrade(
            "觉得这事很难",
            "I think this is very difficult.",
            "I think this could be quite challenging.",
        );
        assert!(res.is_some());
        let comp = res.unwrap();

        // 绝不能判为 GrammarIssue / Chinglish
        assert_eq!(comp.status, Some(ComparisonStatus::ValidB2Upgrade));
        let dims = comp.dimensions.expect("需提供四维评价");
        assert!(dims.correctness, "正确性必须为 true，不能将简单正确判错");
        assert_eq!(dims.sophistication, Sophistication::B2);
        assert!(comp.upgrade.is_some());
    }

    #[test]
    fn test_c1_or_variant_input_recognized_as_exact_match() {
        // 用户直接输入 C1 高阶: "I think this could be quite challenging."
        let res = evaluate_b2_upgrade(
            "觉得这事很难",
            "I think this could be quite challenging.",
            "I think this is pretty challenging.",
        );
        assert!(res.is_some());
        let comp = res.unwrap();
        assert_eq!(comp.status, Some(ComparisonStatus::ExactMatch));
        assert_eq!(comp.dimensions.unwrap().sophistication, Sophistication::C1);
    }

    #[test]
    fn test_diplomatic_pragmatic_nuance_match() {
        // 用户输入语域变体: "I'm not sure I agree with that." (Diplomatic)
        let res = evaluate_b2_upgrade(
            "你错了",
            "I'm not sure I agree with that.",
            "I'm not sure that's entirely accurate.",
        );
        assert!(res.is_some());
        let comp = res.unwrap();
        assert_eq!(comp.status, Some(ComparisonStatus::ExactMatch));
        assert_eq!(comp.dimensions.unwrap().detected_register, Register::Diplomatic);
    }
}
