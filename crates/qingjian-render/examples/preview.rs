//! 离线预览：`cargo run --release -p qingjian-render --example preview -- --out target/render-preview`
//! 把样例帧按浅 / 深色、竖 / 横排画成 PNG，与各平台原生候选窗截图并排比；`--measure` 只量几段文字的宽度与原生对数；
//! 末尾列出验收行每个字形落到了哪家字体。不是日常工具，改渲染器时拿来核对。

use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use qingjian_render::{
    ComparisonResult, DrillStage, FontLibrary, Frame, Layout, LearningCard, LearningExpression,
    LearningMode, LearningStatus, Preedit, PreeditSegment, PreeditStyle, RecallAttempt, Renderer,
    Row, Shadow, StatusCell, Theme, Tone,
};

#[derive(Parser)]
struct Args {
    /// PNG 输出目录。
    #[arg(long, default_value = "target/render-preview")]
    out: PathBuf,

    /// 点 → 像素倍数（Retina 为 2）。
    #[arg(long, default_value_t = 2.0)]
    scale: f32,

    /// 中日字形回退用的 locale。
    #[arg(long, default_value = "zh-CN")]
    locale: String,

    /// 不画阴影（对照壳自己带系统阴影的截图时用）。
    #[arg(long)]
    no_shadow: bool,

    /// 只量几段文字的宽度（点），不出图；与 AppKit 的 NSAttributedString.size() 对数。
    #[arg(long)]
    measure: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "qingjian_render=debug".into()),
        )
        .init();
    let args = Args::parse();
    std::fs::create_dir_all(&args.out)?;

    let started = Instant::now();
    let library = FontLibrary::system(&args.locale)?;
    println!(
        "字体库：{:?}，界面字体 {}",
        started.elapsed(),
        library.ui_family()
    );
    println!("已加载字族：{}", library.families().join(" / "));
    let mut renderer = Renderer::new(library);
    if args.measure {
        for text in [
            "int. hello · int. hi",
            "hello",
            "ni'hao",
            "1/6",
            "你好",
            "phr. you change",
            "int. ",
            "·",
            " · ",
            "hi",
            "你好像",
            "開発する",
        ] {
            let widths: Vec<String> = [11.0, 12.0, 16.0]
                .into_iter()
                .map(|size| format!("{size}pt={:.2}", renderer.measure_points(text, size)))
                .collect();
            println!("{text:<24} {}", widths.join("  "));
        }
        return Ok(());
    }
    let shadow = (!args.no_shadow).then_some(Shadow::mac_panel());

    let scenes: [(&str, Frame, Layout); 12] = [
        ("matrix-horizontal", matrix(), Layout::Horizontal),
        ("nihao-vertical", nihao(), Layout::Vertical),
        (
            "nihao-horizontal",
            nihao_with_sentence(),
            Layout::Horizontal,
        ),
        ("cloud-vertical", cloud(), Layout::Vertical),
        ("corrected-vertical", corrected_japanese(), Layout::Vertical),
        (
            "corrected-horizontal",
            corrected_japanese(),
            Layout::Horizontal,
        ),
        ("probe", probe(), Layout::Vertical),
        ("daily-mode", scene_daily(), Layout::Horizontal),
        ("learn-mode", scene_learn(), Layout::Horizontal),
        ("more-translations", scene_more_translations(), Layout::Horizontal),
        ("drill-input", scene_drill_input(), Layout::Horizontal),
        ("drill-compare", scene_drill_compare(), Layout::Horizontal),
    ];
    for (theme_name, theme) in [("light", Theme::light()), ("dark", Theme::dark())] {
        for (scene, frame, layout) in &scenes {
            let started = Instant::now();
            let rendered = renderer.render(frame, *layout, &theme, args.scale, shadow.as_ref())?;
            let elapsed = started.elapsed();
            let path = args.out.join(format!("{scene}-{theme_name}.png"));
            rendered.pixmap.save_png(&path)?;
            let (w, h) = rendered.content_size_points();
            println!(
                "{:<28} {:>4.0}×{:<4.0}pt  {:>8.2?}  {}",
                format!("{scene}-{theme_name}"),
                w,
                h,
                elapsed,
                path.display()
            );
        }
    }

    // Windows 的悬浮状态条：四格（Daily灰/Learn青/Drill橙）
    let status_scenes: [(&str, [StatusCell; 4]); 3] = [
        ("status-daily", [
            StatusCell::text("中 · 小鹤", true),
            StatusCell::text("，。", true),
            StatusCell::text("学", false),
            StatusCell::Gear,
        ]),
        ("status-learn", [
            StatusCell::text("中 · 小鹤", true),
            StatusCell::text("，。", true),
            StatusCell::text("学", true),
            StatusCell::Gear,
        ]),
        ("status-drill", [
            StatusCell::text("中 · 小鹤", true),
            StatusCell::text("，。", true),
            StatusCell::colored("学", qingjian_render::Color::rgb(249, 115, 22)),
            StatusCell::Gear,
        ]),
    ];
    for (theme_name, theme) in [("light", Theme::light()), ("dark", Theme::dark())] {
        for (scene_name, cells) in &status_scenes {
            let status = renderer.render_status(cells, &theme, args.scale, shadow.as_ref())?;
            let path = args.out.join(format!("{scene_name}-{theme_name}.png"));
            status.rendered.pixmap.save_png(&path)?;
            let (w, h) = status.rendered.content_size_points();
            println!(
                "{:<28} {:>4.0}×{:<4.0}pt  格边界 {:?}  {}",
                format!("{scene_name}-{theme_name}"),
                w,
                h,
                status.cell_edges,
                path.display()
            );
        }
    }

    for probe in [
        "青简 hello 🙂 日本語 骨直曜",
        "開発(かいはつ)する",
        "int. hello · int. hi",
    ] {
        println!(
            "「{probe}」各字形字体：{}",
            renderer.trace_families(probe, &Theme::light()).join(" → ")
        );
    }
    Ok(())
}

// 样例帧：与真机上敲同样拼音看到的候选窗对照，所以内容要和引擎当时给的一致（人工从截图抄）。

/// 真机敲「nihao」看到的第一页（2026-09-13 从截图抄），拼音行带光标、译文、生词橙色、页码。
/// 横排展开成矩阵：6 行 × 9 列，第二行高亮着一条被截断的长候选，末行不满，有一个云端词。
fn matrix() -> Frame {
    let words = [
        "是",
        "时",
        "事",
        "市",
        "使",
        "世",
        "式",
        "十",
        "实",
        "时候",
        "事情",
        "世界",
        "是不是因为我们今天没有去",
        "实际",
        "市场",
        "使用",
        "十分",
        "试试",
        "视频",
        "室内",
        "食物",
        "失败",
        "始终",
        "适合",
        "诗人",
        "石头",
        "时代",
        "示范",
        "士兵",
        "事实上",
        "实验室",
        "视角",
        "世纪",
        "试卷",
        "释放",
        "拾起",
        "师傅",
        "诗歌",
        "时尚",
        "失去",
        "湿度",
        "十月",
        "石油",
        "史诗",
        "驶向",
        "誓言",
        "逝去",
        "柿子",
        "嗜好",
    ];
    let mut rows: Vec<Row> = words
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let mut row = Row::plain(i % 9, *text);
            // 序号只标在高亮所在的第二行
            if i / 9 != 1 {
                row.index.clear();
            }
            row
        })
        .collect();
    rows[8].cloud = true;
    rows[12].annotation = vec![
        ("phr. ".into(), Tone::Faint),
        ("is it because we didn't go today".into(), Tone::Gloss),
    ];
    Frame {
        preedit: Some(Preedit {
            segments: vec![PreeditSegment {
                text: "shi".into(),
                style: PreeditStyle::Typed,
            }],
            cursor: 3,
        }),
        rows,
        highlighted: Some(12),
        columns: 9,
        column_ems: (0..9)
            .map(|column| {
                words
                    .iter()
                    .skip(column)
                    .step_by(9)
                    .map(|word| word.chars().count().min(4) as f32)
                    .fold(0.0, f32::max)
                    + if column == 8 { 1.1 } else { 0.0 }
            })
            .collect(),
        footer: Some("2/12".into()),
        sentence: None,
        status: None,
        learning_card: None,
        ..Default::default()
    }
}

fn nihao() -> Frame {
    Frame {
        preedit: Some(Preedit::plain("ni'hao", 6)),
        rows: vec![
            annotated(
                0,
                "你好",
                &[
                    ("int. ", Tone::Faint),
                    ("hello", Tone::Gloss),
                    (" · ", Tone::Faint),
                    ("int. ", Tone::Faint),
                    ("hi", Tone::Gloss),
                ],
                false,
            ),
            annotated(1, "👋", &[("你好", Tone::Gloss)], false),
            annotated(2, "你好好", &[], false),
            annotated(
                3,
                "你好像",
                &[("phr. ", Tone::Faint), ("you seem", Tone::Fresh)],
                false,
            ),
            annotated(4, "你好久", &[], false),
            annotated(5, "你好看", &[], false),
            annotated(6, "你哈", &[], false),
            annotated(
                7,
                "你换",
                &[
                    ("phr. ", Tone::Faint),
                    ("you change", Tone::Fresh),
                    (" · ", Tone::Faint),
                    ("phr. ", Tone::Faint),
                    ("you swap", Tone::Fresh),
                ],
                false,
            ),
            annotated(
                8,
                "你会",
                &[("phr. ", Tone::Faint), ("you will", Tone::Fresh)],
                false,
            ),
        ],
        highlighted: Some(0),
        columns: 0,
        column_ems: Vec::new(),
        footer: Some("1/6".to_owned()),
        sentence: None,
        status: None,
        learning_card: None,
        ..Default::default()
    }
}

/// 横排真机截图那一次云端整句到了：拼音行右侧带云朵的整句补全。
fn nihao_with_sentence() -> Frame {
    let mut frame = nihao();
    frame.sentence = Some("你好，很高兴认识你！".to_owned());
    frame
}

/// 带云联想的一页：整句补全与云端词各带云朵。
fn cloud() -> Frame {
    let mut frame = nihao();
    frame.rows.truncate(3);
    frame
        .rows
        .push(annotated(3, "你好吗", &[("hello?", Tone::Gloss)], true));
    frame.sentence = Some("你好，世界".to_owned());
    frame.footer = Some("1/3".to_owned());
    frame
}

/// 纠错后的拼音行（删除线 + 淡色剩余）加日文译词（汉字注假名）。
fn corrected_japanese() -> Frame {
    Frame {
        preedit: Some(Preedit {
            segments: vec![
                PreeditSegment {
                    text: "kai".to_owned(),
                    style: PreeditStyle::Typed,
                },
                PreeditSegment {
                    text: "fs".to_owned(),
                    style: PreeditStyle::Struck,
                },
                PreeditSegment {
                    text: "'fa".to_owned(),
                    style: PreeditStyle::Rest,
                },
            ],
            cursor: 5,
        }),
        rows: vec![
            annotated(
                0,
                "开发",
                &[
                    ("v. ", Tone::Faint),
                    ("開発", Tone::Gloss),
                    ("(かいはつ)", Tone::Faint),
                    ("する", Tone::Gloss),
                ],
                false,
            ),
            annotated(
                1,
                "开",
                &[
                    ("v. ", Tone::Faint),
                    ("開", Tone::Fresh),
                    ("(ひら)", Tone::Faint),
                    ("く", Tone::Fresh),
                ],
                false,
            ),
        ],
        highlighted: Some(1),
        columns: 0,
        column_ems: Vec::new(),
        footer: None,
        sentence: None,
        status: Some("已删除「开放」".to_owned()),
        learning_card: None,
        ..Default::default()
    }
}

/// 四条验收用的一行：汉字（zh 字形）、英文、彩色 emoji、日文假名与汉字。
fn probe() -> Frame {
    Frame {
        preedit: None,
        rows: vec![Row::plain(0, "青简 hello 🙂 日本語 骨直曜")],
        highlighted: None,
        columns: 0,
        column_ems: Vec::new(),
        footer: None,
        sentence: None,
        status: None,
        learning_card: None,
        ..Default::default()
    }
}

fn annotated(index: usize, text: &str, annotation: &[(&str, Tone)], cloud: bool) -> Row {
    Row {
        index: (index + 1).to_string(),
        text: text.to_owned(),
        code: None,
        annotation: annotation
            .iter()
            .map(|(s, tone)| ((*s).to_owned(), *tone))
            .collect(),
        cloud,
    }
}

fn scene_daily() -> Frame {
    Frame {
        preedit: Some(Preedit::plain("nihao", 5)),
        rows: vec![
            Row::plain(0, "你好"),
            Row::plain(1, "您好"),
            Row::plain(2, "泥好"),
        ],
        highlighted: Some(0),
        learning_mode: LearningMode::Daily,
        learning_card: None,
        ..Default::default()
    }
}

fn scene_learn() -> Frame {
    Frame {
        preedit: Some(Preedit::plain("fangsong", 8)),
        rows: vec![
            Row::plain(0, "放松"),
            Row::plain(1, "放宽"),
        ],
        highlighted: Some(0),
        learning_mode: LearningMode::Learn,
        learning_card: Some(LearningCard {
            target_text: "放松".to_owned(),
            status: LearningStatus::Ready,
            primary: Some(LearningExpression {
                text: "Take it easy".to_owned(),
                tag: "Natural".to_owned(),
                meaning: "放轻松，别紧张".to_owned(),
                ..Default::default()
            }),
            alternatives: vec![
                LearningExpression {
                    text: "Take it easy".to_owned(),
                    tag: "Natural".to_owned(),
                    meaning: "放轻松，别紧张".to_owned(),
                    ..Default::default()
                },
                LearningExpression {
                    text: "Relax".to_owned(),
                    tag: "Casual".to_owned(),
                    meaning: "放松点".to_owned(),
                    ..Default::default()
                },
                LearningExpression {
                    text: "Chill out".to_owned(),
                    tag: "Casual".to_owned(),
                    meaning: "别激动，冷静下".to_owned(),
                    ..Default::default()
                },
            ],
            expanded: false,
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn scene_more_translations() -> Frame {
    let mut frame = scene_learn();
    if let Some(card) = &mut frame.learning_card {
        card.expanded = true;
        card.selected_index = Some(0);
    }
    frame
}

fn scene_drill_input() -> Frame {
    Frame {
        rows: vec![Row::plain(0, "放轻松")],
        learning_mode: LearningMode::Drill,
        learning_card: Some(LearningCard {
            target_text: "放轻松".to_owned(),
            drill_stage: Some(DrillStage::Input),
            drill_input: "Take it easy".to_owned(),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn scene_drill_compare() -> Frame {
    Frame {
        rows: vec![Row::plain(0, "放轻松")],
        learning_mode: LearningMode::Drill,
        learning_card: Some(LearningCard {
            target_text: "放轻松".to_owned(),
            drill_stage: Some(DrillStage::Compared),
            drill_input: "Relax yourself".to_owned(),
            drill_attempt: Some(RecallAttempt {
                user_attempt: "Relax yourself".to_owned(),
                comparison: ComparisonResult {
                    natural_expression: "Take it easy".to_owned(),
                    key_advice: "口语中用 Take it easy 或 Relax 即可，加 yourself 显得生硬不自然。".to_owned(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}
