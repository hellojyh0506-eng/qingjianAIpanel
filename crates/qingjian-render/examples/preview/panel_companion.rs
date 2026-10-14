//! `--panel companion-*`：学习助手面板整页预览，数据取自严格参考图里的「尽量 / Do your best」。

use std::path::Path;

use qingjian_render::companion::{
    Alternative, CompanionView, Detail, DialogueContent, Example, NuanceContent, TabStatus, Turn,
};
use qingjian_render::{Avatar, FontLibrary, PanelRenderer, Shadow};

/// 预览名 → 视图；不认识返回 `None`。
fn view_for(name: &str) -> Option<CompanionView> {
    let mut view = sample();
    match name {
        "companion-tab0" => view.tab = 0,
        "companion-tab1" => view.tab = 1,
        "companion-tab2" => view.tab = 2,
        "companion-details" => view.details_open = true,
        "companion-menu" => {
            view.tab = 2;
            view.scene_menu_open = true;
        }
        "companion-loading" => {
            view.nuance_status = TabStatus::Loading;
            view.nuance = NuanceContent::default();
            view.tags.clear();
            view.level.clear();
        }
        "companion-streaming" => {
            view.nuance_status = TabStatus::Streaming;
            view.nuance.usage.clear();
            view.nuance.tone.clear();
            view.nuance.example = None;
            view.nuance.details.clear();
        }
        "companion-error" => {
            view.nuance_status = TabStatus::Failed("网络连接超时，请检查网络后重试。".into());
            view.nuance = NuanceContent::default();
        }
        "companion-empty" => {
            view.nuance_status = TabStatus::Idle;
            view.nuance = NuanceContent::default();
        }
        "companion-minimized" => view.minimized = true,
        "companion-scrolled" => {
            view.tab = 2;
            view.scroll = 400.0;
        }
        "companion-notice" => {
            view.notice = Some("已复制".into());
        }
        _ => return None,
    }
    Some(view)
}

pub fn run(
    name: &str,
    out: &Path,
    scale: f32,
    locale: &str,
    shadow: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(view) = view_for(name) else {
        return Err(format!("没有 `{name}` 预览").into());
    };
    let mut renderer = PanelRenderer::new(FontLibrary::system(locale)?);
    renderer.set_scale(scale);
    let frame =
        qingjian_render::companion::render(&mut renderer, &view, shadow.then(Shadow::mac_panel))?;
    let path = out.join(format!("{name}.png"));
    frame.frame.pixmap.save_png(&path)?;
    println!(
        "{}（{}×{} px，可滚动 {:.0} pt）",
        path.display(),
        frame.frame.pixmap.width(),
        frame.frame.pixmap.height(),
        frame.max_scroll
    );
    Ok(())
}

fn example(en: &str, zh: &str, hl: &str) -> Example {
    Example {
        en: en.into(),
        zh: zh.into(),
        hl: hl.into(),
    }
}

fn turn(speaker: Avatar, en: &str, zh: &str, hl: &str) -> Turn {
    Turn {
        speaker,
        en: en.into(),
        zh: zh.into(),
        hl: hl.into(),
    }
}

fn alternative(
    expression: &str,
    zh: &str,
    tag: &str,
    tone: &str,
    diff: &str,
    ex: Example,
) -> Alternative {
    Alternative {
        expression: expression.into(),
        zh: zh.into(),
        tag: tag.into(),
        tone: tone.into(),
        diff: diff.into(),
        example: ex,
    }
}

fn sample() -> CompanionView {
    let mut view = CompanionView::new("尽量", "Do your best");
    view.tags = vec!["自然口语".into(), "鼓励支持".into(), "常用表达".into()];
    view.level = "B1-B2".into();
    view.nuance = NuanceContent {
        meaning: "尽你所能；尽力就好。".into(),
        tone: "鼓励、支持，强调努力，不要求结果一定完美。".into(),
        usage: "用于安慰或鼓励别人，例如考试、工作、比赛等场景。".into(),
        example: Some(example(
            "Just do your best. That's all you can do.",
            "尽力就好，你能做的就是把自己能做的做到最好。",
            "do your best",
        )),
        details: vec![
            Detail {
                title: "do one's best".into(),
                body: "固定搭配，one's 随主语变化：I did my best.".into(),
            },
            Detail {
                title: "best 的词性".into(),
                body: "这里 best 是名词，表示「最大努力」。".into(),
            },
            Detail {
                title: "do your best to + 动词".into(),
                body: "后面接动词原形：Do your best to finish it.".into(),
            },
        ],
    };
    view.nuance_status = TabStatus::Done;
    view.alternatives = vec![
        alternative(
            "Give it your best shot",
            "尽力一试；试试看。",
            "日常口语",
            "鼓励、轻松",
            "强调尝试，不确定能否成功时更自然。",
            example(
                "You should give it your best shot. Nothing to lose!",
                "你应该尽力一试，反正没有损失！",
                "",
            ),
        ),
        alternative(
            "Do the best you can",
            "尽自己所能即可。",
            "温和安慰",
            "温和，不给压力",
            "比 do your best 更随意，常用于安慰别人。",
            example(
                "Just do the best you can, and I'll support you.",
                "尽力就好，我会支持你。",
                "",
            ),
        ),
        alternative(
            "Give it your all",
            "全力以赴。",
            "更有力度",
            "更强烈、积极",
            "强调投入全部精力，语气比 do your best 更有冲劲。",
            example(
                "He always gives his all in every game.",
                "他在每场比赛中总是全力以赴。",
                "",
            ),
        ),
    ];
    view.alternatives_status = TabStatus::Done;
    view.dialogue = DialogueContent {
        title: "朋友聊天（鼓励安慰）".into(),
        turns: vec![
            turn(
                Avatar::Boy,
                "I have a big presentation tomorrow. I'm really nervous.",
                "我明天有个重要的演讲，有点紧张。",
                "",
            ),
            turn(
                Avatar::Girl,
                "You'll be fine. Just do your best.",
                "你会没问题的，尽力就好。",
                "do your best",
            ),
            turn(
                Avatar::Boy,
                "Thanks! That makes me feel better.",
                "谢谢！这样我感觉好多了。",
                "",
            ),
        ],
        points: vec![
            "在安慰和鼓励场景中非常自然。".into(),
            "语气积极友好，适合日常对话。".into(),
        ],
    };
    view.dialogue_status = TabStatus::Done;
    view
}
