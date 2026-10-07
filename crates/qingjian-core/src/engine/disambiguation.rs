//! 上下文消歧与同音词/混淆词重排。
//!
//! 在 Level 2 本地零延迟通道中，结合光标前文（surrounding before）与会话链上下文，
//! 对高频歧义词（如 傻子/啥子、她/他/它、在/再、北京/背景 等）进行上下文感知打分加权。

/// 从光标前文中提取最近的有效上下文词（前词与再前词）。
pub fn extract_context_from_before(before: &str) -> (Option<String>, Option<String>) {
    let trimmed = before.trim_end();
    if trimmed.is_empty() {
        return (None, None);
    }
    // 截取最后一个标点符号之后的文本段
    let last_segment = trimmed
        .rsplit(|c: char| {
            matches!(
                c,
                '，' | '。'
                    | '！'
                    | '？'
                    | '；'
                    | '：'
                    | ','
                    | '.'
                    | '!'
                    | '?'
                    | ';'
                    | ':'
                    | '\n'
                    | '\r'
            )
        })
        .next()
        .unwrap_or(trimmed);
    let chars: Vec<char> = last_segment.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.is_empty() {
        return (None, None);
    }
    let n = chars.len();
    if n == 1 {
        let prev: String = chars[0].into();
        return (Some(prev), None);
    }
    if n == 2 {
        let prev: String = chars.iter().collect();
        return (Some(prev), None);
    }
    // 长度 >= 3
    let last_two: String = chars[n - 2..].iter().collect();
    let earlier_two: String = if n >= 4 {
        chars[n - 4..n - 2].iter().collect()
    } else {
        chars[..n - 2].iter().collect()
    };
    (Some(last_two), Some(earlier_two))
}

/// 上下文语义消歧加权（对数空间加分/减分）。
pub fn contextual_score_adjustment(before_text: Option<&str>, candidate_text: &str) -> f64 {
    // 0. 固有专有名词与非法搭配判定（无需前文上下文）
    if candidate_text == "微信输入法" {
        return 5.0;
    } else if candidate_text == "为新输入法" || candidate_text == "为新" {
        return -5.0;
    } else if candidate_text == "很那话" {
        return -10.0;
    } else if candidate_text == "河南话" {
        return 6.0;
    } else if candidate_text == "解耦" {
        return 6.0;
    } else if candidate_text == "为什么" {
        return 8.0;
    } else if candidate_text == "我是吗" {
        return -8.0;
    }

    // 句首/从头输入时的特定虚词与副词模式
    let is_clause_start = match before_text {
        None => true,
        Some(b) => {
            let t = b.trim();
            t.is_empty()
                || t.ends_with('！')
                || t.ends_with('!')
                || t.ends_with('。')
                || t.ends_with('.')
                || t.ends_with('？')
                || t.ends_with('?')
                || t.ends_with('，')
                || t.ends_with(',')
                || t.ends_with('；')
                || t.ends_with(';')
        }
    };
    if is_clause_start {
        if candidate_text == "到底" {
            return 5.0;
        } else if candidate_text == "到的" {
            return -6.0;
        }
    }

    let Some(before) = before_text else {
        return 0.0;
    };
    if before.is_empty() {
        return 0.0;
    }

    // 0.05 缩写/简拼在不同上下文中的意图消歧
    // z'j'd'l: 真的假的 (传闻/吃惊/质疑) vs 自己的了 (归属/占有/陈述)
    if candidate_text == "真的假的" {
        if before.contains("他说")
            || before.contains("听讲")
            || before.contains("听说")
            || before.contains("不会吧")
            || before.contains("中奖")
            || before.contains("发财")
            || before.contains("竟然")
            || before.contains("居然")
            || before.contains("离谱")
        {
            return 8.0;
        }
    } else if candidate_text == "自己的了" || candidate_text == "自己的" {
        if before.contains("这是我")
            || before.contains("属于我")
            || before.contains("变成我")
            || before.contains("变成他")
            || before.contains("是我")
            || before.contains("算我")
            || before.contains("归我")
        {
            return 8.0;
        }
    }

    // 0.06 河南方言高频词汇与口语表达消歧/偏好
    if candidate_text == "我嘞个豆" || candidate_text == "我勒个豆" {
        return 6.0;
    } else if candidate_text == "我日他哥" {
        return 6.0;
    } else if candidate_text == "妮儿" {
        if is_clause_start || before.ends_with("小") || before.ends_with("个") || before.ends_with("这") || before.ends_with("那") {
            return 6.0;
        }
    } else if candidate_text == "弄啥哩" || candidate_text == "弄啥类" || candidate_text == "弄啥嘞" || candidate_text == "弄啥呢" {
        return 6.0;
    } else if candidate_text == "得劲" || candidate_text == "可得劲" {
        if before.ends_with("真") || before.ends_with("挺") || before.ends_with("可") || before.ends_with("不") || is_clause_start {
            return 6.0;
        }
    } else if candidate_text == "乖乖嘞" || candidate_text == "信球" || candidate_text == "不中" || candidate_text == "可中" {
        return 5.0;
    } else if candidate_text == "恁" {
        if before.ends_with("看") || before.ends_with("听") || is_clause_start {
            return 4.0;
        }
    }

    // n's'b: 你是不是 (反问/推测) vs 你上班 (工作/考勤/时间) vs 你身边 (空间方位)
    if candidate_text == "你上班" {
        if before.contains("周一")
            || before.contains("今天")
            || before.contains("明天")
            || before.contains("周末")
            || before.contains("放假")
            || before.contains("几点")
            || before.contains("去公司")
            || before.contains("单位")
        {
            return 8.0;
        }
    } else if candidate_text == "你身边" {
        if before.ends_with("在")
            || before.ends_with("到")
            || before.ends_with("回到")
            || before.ends_with("留在")
            || before.ends_with("陪在")
            || before.contains("靠在")
            || before.contains("留在")
            || before.contains("陪在")
            || before.contains("守在")
            || before.contains("在他")
            || before.contains("在她")
        {
            return 8.0;
        }
    } else if candidate_text == "你是不是" {
        if before.contains("我看")
            || before.contains("你看")
            || before.contains("怎么")
            || before.contains("好像")
            || before.contains("难道")
            || before.contains("怪怪的")
            || before.contains("觉得")
            || before.contains("感觉")
        {
            return 8.0;
        }
    }

    // 0.1 句末疑问/语气助词：吗 vs 骂，呢，吧，嘛
    if candidate_text == "吗" {
        if before.ends_with("了")
            || before.ends_with("过")
            || before.ends_with("着")
            || before.ends_with("好")
            || before.ends_with("对")
            || before.ends_with("行")
            || before.ends_with("是")
            || before.ends_with("能")
            || before.ends_with("会")
            || before.ends_with("去")
            || before.ends_with("来")
            || before.ends_with("吃")
            || before.ends_with("喝")
            || before.ends_with("看")
            || before.ends_with("听")
            || before.ends_with("做")
            || before.ends_with("搞")
            || before.ends_with("充")
            || before.ends_with("充电")
            || before.ends_with("动")
            || before.ends_with("买")
            || before.ends_with("卖")
            || before.ends_with("读")
            || before.ends_with("写")
            || before.contains("充电了")
            || before.contains("吃饭了")
            || before.contains("搞定")
            || before.contains("可以")
        {
            return 6.0;
        }
    } else if candidate_text == "骂" {
        if before.ends_with("了") || before.ends_with("过") || before.ends_with("着") {
            return -6.0;
        }
    } else if candidate_text == "呢" {
        if before.ends_with("在干嘛")
            || before.ends_with("在做什么")
            || before.ends_with("在做啥")
            || before.ends_with("怎么了")
            || before.ends_with("干嘛")
            || before.ends_with("怎么")
            || before.ends_with("哪儿")
            || before.ends_with("为什么")
            || before.ends_with("正在")
            || before.ends_with("在")
            || before.ends_with("了")
        {
            return 6.0;
        }
    } else if candidate_text == "吧" {
        if before.ends_with("走")
            || before.ends_with("去")
            || before.ends_with("看")
            || before.ends_with("行")
            || before.ends_with("算")
            || before.ends_with("对")
            || before.ends_with("好")
            || before.ends_with("是个傻子")
            || before.ends_with("傻子")
        {
            return 4.5;
        }
    } else if candidate_text == "嘛" {
        if before.ends_with("干") || before.ends_with("在干") {
            return 4.5;
        }
    }

    // 0.2 关联副词与结构短语：到底 vs 到的
    if candidate_text == "到底" {
        if before.ends_with("你")
            || before.ends_with("他")
            || before.ends_with("她")
            || before.ends_with("这")
            || before.ends_with("那")
            || before.ends_with("究竟")
            || is_clause_start
        {
            return 6.0;
        }
    } else if candidate_text == "到的" {
        if is_clause_start || before.ends_with("你到底") || before.ends_with("到底") {
            return -6.0;
        }
    }

    // 0.3 A-not-A 问句结构 (行不行、好不好、能不能、是不是)
    if candidate_text == "行" {
        if before.ends_with("行不") || before.ends_with("到底行不") {
            return 6.0;
        }
    } else if candidate_text == "行不行" {
        if before.ends_with("到底") || before.contains("到底") || is_clause_start {
            return 6.0;
        }
    }

    // 1. 傻子 vs 啥子
    if candidate_text == "傻子" {
        if before.ends_with("个")
            || before.ends_with("是个")
            || before.ends_with("像个")
            || before.ends_with("真是个")
            || before.ends_with("就是个")
            || before.ends_with("大")
            || before.ends_with("装")
            || before.contains("不会是")
        {
            return 4.0;
        }
    } else if candidate_text == "啥子" {
        if before.contains("做")
            || before.contains("为")
            || before.contains("搞")
            || before.contains("算")
            || before.ends_with("干")
        {
            return 2.5;
        } else if before.ends_with("个") || before.ends_with("是个") || before.contains("不会是") {
            return -3.0;
        }
    }

    // 2. 她 vs 他 vs 它 指代消歧
    if candidate_text == "她" {
        let has_ta_female = before.contains('她');
        let has_ta_male = before.contains('他');
        if has_ta_female && !has_ta_male {
            return 3.5;
        } else if has_ta_male && !has_ta_female {
            return -2.5;
        }
    } else if candidate_text == "他" {
        let has_ta_female = before.contains('她');
        let has_ta_male = before.contains('他');
        if has_ta_male && !has_ta_female {
            return 3.5;
        } else if has_ta_female && !has_ta_male {
            return -2.5;
        }
    } else if candidate_text == "它" {
        if before.contains("它") {
            return 3.0;
        }
        if before.contains("她") || before.contains("他") {
            return -4.0;
        }
    }

    // 3. 在 vs 再
    if candidate_text == "再" {
        // 时间/重复/意图副词之后
        if before.ends_with("明天")
            || before.ends_with("后天")
            || before.ends_with("下次")
            || before.ends_with("回头")
            || before.ends_with("稍后")
            || before.ends_with("北京")
            || before.ends_with("上海")
            || before.ends_with("想")
            || before.ends_with("要")
            || before.ends_with("再")
        {
            return 2.5;
        }
    } else if candidate_text == "在" {
        // 存在/居住动词之后
        if before.ends_with("人在")
            || before.ends_with("现居")
            || before.ends_with("住")
            || before.ends_with("生")
            || before.ends_with("放")
            || before.ends_with("坐")
        {
            return 2.5;
        }
    }

    // 4. 再去 vs 在去
    if candidate_text == "再去" {
        if before.ends_with("明天") || before.ends_with("想") || before.ends_with("我") {
            return 3.5;
        }
    } else if candidate_text == "在去" {
        if before.ends_with("明天") || before.ends_with("想") {
            return -3.5;
        }
    }

    // 5. 北京 vs 背景
    if candidate_text == "北京" {
        if before.ends_with("人在")
            || before.ends_with("在")
            || before.ends_with("去")
            || before.ends_with("飞往")
        {
            return 4.0;
        }
    } else if candidate_text == "背景" {
        if before.ends_with("人在") || before.ends_with("飞往") {
            return -4.0;
        }
    }

    // 6. 再住一天 vs 在住一天
    if candidate_text == "再住一天" {
        if before.contains("北京") || before.contains("在") {
            return 3.5;
        }
    } else if candidate_text == "在住一天" {
        return -3.5;
    }

    // 7. 智障 vs 纸张 (Case A)
    if candidate_text == "智障" {
        if before.ends_with("很")
            || before.ends_with("是个")
            || before.ends_with("太")
            || before.ends_with("真")
            || before.ends_with("像个")
            || before.contains("是很")
            || before.contains("这人")
        {
            return 4.5;
        }
    } else if candidate_text == "纸张" {
        if before.ends_with("很") || before.ends_with("太") || before.ends_with("真") || before.contains("是很") {
            return -4.5;
        } else if before.ends_with("打印") || before.ends_with("复印") || before.ends_with("白") || before.ends_with("张") {
            return 3.5;
        }
    }

    // 8. 啥 vs 傻 (Case B & G)
    if candidate_text == "啥" {
        if before.ends_with("在说")
            || before.ends_with("说")
            || before.ends_with("干")
            || before.ends_with("做")
            || before.ends_with("看")
            || before.ends_with("搞")
            || before.ends_with("算")
            || before.contains("你在说")
            || before.contains("不知道你在说")
        {
            return 4.5;
        }
    } else if candidate_text == "傻" {
        if before.ends_with("在说") || before.contains("你在说") {
            return -3.5;
        }
    }

    // 9. 微信词级加分
    if candidate_text == "微信" {
        return 3.0;
    }

    // 10. 的 vs 地 vs 得
    if candidate_text == "得" {
        if before.ends_with("跑")
            || before.ends_with("走")
            || before.ends_with("做")
            || before.ends_with("玩")
            || before.ends_with("吃")
            || before.ends_with("睡")
            || before.ends_with("写")
            || before.ends_with("长")
            || before.ends_with("变")
            || before.ends_with("看")
            || before.ends_with("打")
            || before.ends_with("过")
            || before.ends_with("说")
            || before.ends_with("听")
            || before.ends_with("笑")
            || before.ends_with("哭")
            || before.ends_with("动")
            || before.ends_with("跳")
            || before.ends_with("显")
            || before.ends_with("觉")
            || before.ends_with("搞")
            || before.ends_with("办")
            || before.ends_with("修")
            || before.ends_with("算")
            || before.ends_with("飞")
            || before.ends_with("记")
        {
            return 4.5;
        }
    } else if candidate_text == "地" {
        if before.ends_with("慢")
            || before.ends_with("快")
            || before.ends_with("悄")
            || before.ends_with("默")
            || before.ends_with("静")
            || before.ends_with("努力")
            || before.ends_with("拼命")
            || before.ends_with("认真")
            || before.ends_with("仔细")
            || before.ends_with("开心")
            || before.ends_with("高兴")
            || before.ends_with("疯狂")
            || before.ends_with("坚定")
            || before.ends_with("绝望")
            || before.ends_with("温柔")
            || before.ends_with("深情")
            || before.ends_with("痛苦")
            || before.ends_with("不住")
        {
            return 4.5;
        }
    } else if candidate_text == "的" {
        if before.ends_with("我")
            || before.ends_with("你")
            || before.ends_with("他")
            || before.ends_with("她")
            || before.ends_with("它")
            || before.ends_with("我们")
            || before.ends_with("你们")
            || before.ends_with("他们")
            || before.ends_with("谁")
            || before.ends_with("这")
            || before.ends_with("那")
        {
            return 3.5;
        } else if before.ends_with("跑")
            || before.ends_with("走")
            || before.ends_with("玩")
            || before.ends_with("长")
            || before.ends_with("显")
            || before.ends_with("觉")
            || before.ends_with("慢")
            || before.ends_with("悄")
            || before.ends_with("默")
        {
            return -3.5;
        }
    }

    // 11. 做 vs 作
    if candidate_text == "做" {
        if before.ends_with("想")
            || before.ends_with("要")
            || before.ends_with("打算")
            || before.ends_with("准备")
            || before.ends_with("正在")
            || before.ends_with("在")
            || before.ends_with("去")
            || before.ends_with("会")
            || before.ends_with("能")
            || before.ends_with("敢")
            || before.ends_with("喜欢")
            || before.ends_with("替我")
            || before.ends_with("帮我")
        {
            return 3.5;
        }
    } else if candidate_text == "作" {
        if before.ends_with("好")
            || before.ends_with("新")
            || before.ends_with("大")
            || before.ends_with("名")
            || before.ends_with("文学")
            || before.ends_with("艺术")
            || before.ends_with("家庭")
            || before.ends_with("布置")
            || before.ends_with("完成")
            || before.ends_with("合")
            || before.ends_with("工")
        {
            return 3.5;
        } else if before.ends_with("打算") || before.ends_with("准备") || before.ends_with("正在") {
            return -2.5;
        }
    }

    // 12. 哪 vs 那
    if candidate_text == "哪" {
        if before.ends_with("去")
            || before.ends_with("在")
            || before.ends_with("到")
            || before.ends_with("从")
            || before.ends_with("是")
            || before.ends_with("选")
            || before.contains("？")
            || before.contains("?")
            || before.contains("谁")
            || before.contains("请问")
            || before.contains("到底")
            || before.contains("究竟")
            || before.contains("不知道你在")
            || before.contains("你想去")
        {
            return 3.5;
        }
    } else if candidate_text == "那" {
        if before.ends_with("就")
            || before.ends_with("就是")
            || before.ends_with("就在")
            || before.ends_with("就去")
            || before.ends_with("走到")
            || before.ends_with("看到")
            || before.ends_with("当时")
            || before.ends_with("后来")
            || before.ends_with("既然")
        {
            return 3.5;
        } else if before.contains("你想去") || before.contains("请问你在") {
            return -2.5;
        }
    }

    // 13. 水 vs 谁/说/睡 (液体/水源修饰搭配)
    if candidate_text == "水" {
        if before.ends_with("蒸馏")
            || before.ends_with("纯净")
            || before.ends_with("矿泉")
            || before.ends_with("自来")
            || before.ends_with("温开")
            || before.ends_with("白开")
            || before.ends_with("喝")
            || before.ends_with("饮用")
            || before.ends_with("倒")
        {
            return 8.0;
        }
    } else if candidate_text == "谁" || candidate_text == "说" || candidate_text == "睡" {
        if before.ends_with("蒸馏")
            || before.ends_with("纯净")
            || before.ends_with("矿泉")
            || before.ends_with("自来")
            || before.ends_with("温开")
            || before.ends_with("白开")
        {
            return -8.0;
        }
    }

    0.0
}

/// 全局整句语义一致性评分（用于长句子内部远距离依存与指代消歧，如句首代词与句尾搭配）。
pub fn sentence_semantic_consistency(sentence: &str) -> f64 {
    let mut score = 0.0;

    // 1. 性别依存一致性：
    // 女性特征名词/搭配：裙子、口红、高跟鞋、连衣裙、文胸、生孩子、怀孕、坐月子、痛经、例假、产后
    // 男性特征名词/搭配：胡须、剃须刀、前列腺
    let has_female_markers = sentence.contains("裙子")
        || sentence.contains("口红")
        || sentence.contains("高跟鞋")
        || sentence.contains("连衣裙")
        || sentence.contains("文胸")
        || sentence.contains("生孩子")
        || sentence.contains("怀孕")
        || sentence.contains("坐月子")
        || sentence.contains("痛经")
        || sentence.contains("产后");

    let has_male_markers = sentence.contains("胡须")
        || sentence.contains("剃须刀")
        || sentence.contains("前列腺");

    if has_female_markers {
        if sentence.starts_with('她') || sentence.contains("在她") || sentence.contains("是她") {
            score += 4.5;
        } else if sentence.starts_with('他') || sentence.contains("在他") || sentence.contains("是他") {
            score -= 4.0;
        }
    } else if has_male_markers {
        if sentence.starts_with('他') || sentence.contains("在他") || sentence.contains("是他") {
            score += 4.5;
        } else if sentence.starts_with('她') || sentence.contains("在她") || sentence.contains("是她") {
            score -= 4.0;
        }
    }

    // 2. 的/地/得 句子内部搭配一致性
    if sentence.contains("跑的快") || sentence.contains("做的很好") || sentence.contains("飞的高") {
        score -= 3.0;
    }
    if sentence.contains("跑得快") || sentence.contains("做得很好") || sentence.contains("飞得高") {
        score += 3.0;
    }

    // 3. 量词与名词/形容词搭配一致性：
    // "一件...裙子/衣服/很棒" (合法数量搭配) vs "意见...裙子/衣服/很棒" (语病错切)
    if sentence.contains("意见") && (sentence.contains("裙子") || sentence.contains("衣服") || sentence.contains("很棒")) {
        score -= 10.0;
    }
    if sentence.contains("一件") && (sentence.contains("裙子") || sentence.contains("衣服") || sentence.contains("很棒")) {
        score += 5.0;
    }

    // 4. 句末时态与语气助词句法一致性 (如: "了吗" 合法疑问 vs "了骂" 语法断裂)
    if sentence.ends_with("了骂") || sentence.ends_with("过骂") || sentence.ends_with("着骂") {
        score -= 10.0;
    }
    if sentence.ends_with("了吗") || sentence.ends_with("了吗？") || sentence.ends_with("了吗?") {
        score += 6.0;
    }
    if sentence.ends_with("了呢") || sentence.ends_with("干嘛呢") || sentence.ends_with("怎么了呢") {
        score += 6.0;
    }

    // 5. 句首关联副词 vs 结构虚词 (如: "到底" vs "到的")
    if sentence.starts_with("到的") {
        score -= 8.0;
    }
    if sentence.starts_with("到底") {
        score += 5.0;
    }
    if sentence.contains("到底行不行") {
        score += 10.0;
    } else if sentence.contains("行不行") || sentence.contains("好不好") || sentence.contains("能不能") {
        score += 6.0;
    }

    // 6. 常见专有名词与液体修饰搭配 (如: 蒸馏水 vs 蒸馏谁/蒸馏说)
    if sentence.contains("蒸馏谁") || sentence.contains("蒸馏说") || sentence.contains("纯净谁") || sentence.contains("自来谁") {
        score -= 10.0;
    }
    if sentence.contains("蒸馏水") || sentence.contains("纯净水") || sentence.contains("自来水") || sentence.contains("矿泉水") {
        score += 8.0;
    }

    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_context() {
        let (prev, earlier) = extract_context_from_before("今天她吃饭但是");
        assert_eq!(prev.as_deref(), Some("但是"));
        assert_eq!(earlier.as_deref(), Some("吃饭"));

        let (prev, earlier) = extract_context_from_before("你好，我明天");
        assert_eq!(prev.as_deref(), Some("明天"));
        assert_eq!(earlier.as_deref(), Some("我"));

        let (prev, _) = extract_context_from_before("个");
        assert_eq!(prev.as_deref(), Some("个"));
    }

    #[test]
    fn test_disambiguation_cases() {
        assert!(contextual_score_adjustment(Some("你不会是个"), "傻子") > 3.0);
        assert!(contextual_score_adjustment(Some("你不会是个"), "啥子") < 0.0);

        assert!(contextual_score_adjustment(Some("今天她吃饭但是"), "她") > 2.0);
        assert!(contextual_score_adjustment(Some("今天她吃饭但是"), "他") < 0.0);

        assert!(contextual_score_adjustment(Some("他今天告诉我"), "他") > 2.0);
        assert!(contextual_score_adjustment(Some("他今天告诉我"), "她") < 0.0);

        assert!(contextual_score_adjustment(Some("我明天"), "再去") > 2.0);
        assert!(contextual_score_adjustment(Some("我明天"), "在去") < 0.0);

        assert!(contextual_score_adjustment(Some("我人在"), "北京") > 2.0);
        assert!(contextual_score_adjustment(Some("我人在"), "背景") < 0.0);

        assert!(contextual_score_adjustment(Some("我在北京"), "再住一天") > 2.0);
        assert!(contextual_score_adjustment(Some("我在北京"), "在住一天") < 0.0);

        // Case A: 是很 -> 智障 > 纸张
        assert!(contextual_score_adjustment(Some("是很"), "智障") > 3.0);
        assert!(contextual_score_adjustment(Some("是很"), "纸张") < 0.0);

        // Case B: 我不知道你在说 -> 啥 > 傻
        assert!(contextual_score_adjustment(Some("我不知道你在说"), "啥") > 3.0);
        assert!(contextual_score_adjustment(Some("我不知道你在说"), "傻") < 0.0);

        // Case E: 微信输入法 > 为新输入法
        assert!(contextual_score_adjustment(Some(""), "微信输入法") > 3.0);
        assert!(contextual_score_adjustment(Some(""), "为新输入法") < 0.0);

        // Case 6: 的 vs 地 vs 得
        assert!(contextual_score_adjustment(Some("跑"), "得") > 3.0);
        assert!(contextual_score_adjustment(Some("跑"), "的") < 0.0);
        assert!(contextual_score_adjustment(Some("慢慢"), "地") > 3.0);
        assert!(contextual_score_adjustment(Some("慢慢"), "的") < 0.0);
        assert!(contextual_score_adjustment(Some("我"), "的") > 2.0);

        // Case 6: 做 vs 作
        assert!(contextual_score_adjustment(Some("打算"), "做") > 2.0);
        assert!(contextual_score_adjustment(Some("打算"), "作") < 0.0);
        assert!(contextual_score_adjustment(Some("好"), "作") > 2.0);

        // Case 6: 哪 vs 那
        assert!(contextual_score_adjustment(Some("你想去"), "哪") > 2.0);
        assert!(contextual_score_adjustment(Some("你想去"), "那") < 0.0);
        assert!(contextual_score_adjustment(Some("就在"), "那") > 2.0);

        // Case Jieou & Wsm
        assert!(contextual_score_adjustment(None, "解耦") > 3.0);
        assert!(contextual_score_adjustment(None, "为什么") > 3.0);
        assert!(contextual_score_adjustment(None, "我是吗") < 0.0);
    }
}
