//! 面板样式表：把 token 色板、图标、头像、标签栏、胶囊、卡片、错误条、阴影画在一张 420 pt 宽的图上，核对面板渲染基础。

use std::path::Path;

use qingjian_render::{
    Area, Avatar, ColorPair, FontLibrary, Icon, PanelRenderer, PanelSurface, PanelText, RichSpan,
    Shadow, tokens,
};

const WIDTH: f32 = tokens::WINDOW_WIDTH;
const PAD: f32 = 16.0;

pub fn run(
    name: &str,
    out: &Path,
    scale: f32,
    locale: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if name != "kit" {
        return Err(format!(
            "还没有 `{name}` 面板预览：现在只有 `kit`，学习助手与 AI 问答的整页预览随 P8 / P9 补"
        )
        .into());
    }
    let mut renderer = PanelRenderer::new(FontLibrary::system(locale)?);
    renderer.set_scale(scale);
    // 先在一张够高的画布上试画一遍量出总高度，再按实际高度画正式的一张
    let height = {
        let mut probe = renderer.begin::<()>(WIDTH, 2400.0, None)?;
        draw_kit(&mut probe)
    };
    let mut surface = renderer.begin::<()>(WIDTH, height, Some(Shadow::mac_panel()))?;
    draw_kit(&mut surface);
    let frame = surface.finish();
    let path = out.join("panel-kit.png");
    frame.pixmap.save_png(&path)?;
    println!(
        "{}（{}×{} px）",
        path.display(),
        frame.pixmap.width(),
        frame.pixmap.height()
    );
    Ok(())
}

/// 画整张样式表，返回内容高度（点）。
fn draw_kit<H>(s: &mut PanelSurface<'_, H>) -> f32 {
    let title = PanelText::new(13.0, 18.0, tokens::TEXT).bolded();
    let body = PanelText::new(12.0, 18.0, tokens::TEXT);
    let small = PanelText::new(10.0, 14.0, tokens::TEXT_2);
    let mut y = PAD;

    y = section(s, y, "色板", &title);
    let swatches = [
        ("primary", tokens::PRIMARY),
        ("selected", tokens::SELECTED_BG),
        ("tabbar", tokens::TABBAR_BG),
        ("card", tokens::CARD_BG),
        ("blue", tokens::CARD_BLUE),
        ("purple", tokens::CARD_PURPLE),
        ("border", tokens::BORDER),
        ("text", tokens::TEXT),
        ("text-2", tokens::TEXT_2),
        ("chip-b", tokens::ICON_CHIP_BLUE),
        ("chip-g", tokens::ICON_CHIP_GREEN),
        ("chip-p", tokens::ICON_CHIP_PURPLE),
        ("badge-1", tokens::BADGE_1),
        ("badge-2", tokens::BADGE_2),
        ("badge-3", tokens::BADGE_3),
        ("error", tokens::ERROR_FG),
    ];
    let cell = (WIDTH - 2.0 * PAD) / 8.0;
    for (i, (label, color)) in swatches.iter().enumerate() {
        let (col, row) = ((i % 8) as f32, (i / 8) as f32);
        let area = Area::new(PAD + col * cell, y + row * 44.0, cell - 6.0, 24.0);
        s.fill_bordered(area, tokens::RADIUS_CHIP, *color, tokens::BORDER);
        s.text(area.x, area.bottom() + 3.0, label, &small);
    }
    y += 2.0 * 44.0 + 6.0;

    y = section(s, y, "图标（Lucide，16 / 20 pt）", &title);
    let cell = (WIDTH - 2.0 * PAD) / 11.0;
    for (i, icon) in Icon::ALL.iter().enumerate() {
        let (col, row) = ((i % 11) as f32, (i / 11) as f32);
        let size = if row < 2.0 { 20.0 } else { 16.0 };
        s.icon(
            *icon,
            PAD + col * cell,
            y + row * 30.0,
            size,
            tokens::TEXT_2,
        );
    }
    y += 3.0 * 30.0 + 4.0;
    for (i, degrees) in [0.0, 45.0, 90.0, 180.0].into_iter().enumerate() {
        s.icon_rotated(
            Icon::LoaderCircle,
            PAD + i as f32 * 30.0,
            y,
            20.0,
            tokens::PRIMARY,
            degrees,
        );
    }
    y += 34.0;

    y = section(s, y, "标签栏、胶囊", &title);
    let bar = Area::new(PAD, y, WIDTH - 2.0 * PAD, 36.0);
    s.fill(bar, tokens::RADIUS_BUTTON + 2.0, tokens::TABBAR_BG);
    let tab_w = (bar.width - 8.0) / 3.0;
    for (i, label) in ["语感解析", "更自然地说", "真实对话"]
        .into_iter()
        .enumerate()
    {
        let tab = Area::new(bar.x + 4.0 + i as f32 * tab_w, bar.y + 4.0, tab_w, 28.0);
        let selected = i == 0;
        if selected {
            s.fill_bordered(
                tab,
                tokens::RADIUS_BUTTON,
                tokens::SELECTED_BG,
                tokens::PRIMARY,
            );
        }
        let color = if selected {
            tokens::PRIMARY
        } else {
            tokens::TEXT_2
        };
        let style = PanelText::new(12.0, 18.0, color);
        let width = s.measure(label, &style);
        s.text(
            tab.x + (tab.width - width) / 2.0,
            tab.y + 5.0,
            label,
            &style,
        );
    }
    y += 48.0;
    let mut x = PAD;
    for (label, pair) in [
        ("自然口语", tokens::TAG_ORANGE),
        ("鼓励支持", tokens::TAG_GREEN),
        ("温和安慰", tokens::TAG_BLUE),
        ("纠", tokens::TAG_ORANGE),
    ] {
        x += chip(s, x, y, label, pair) + 8.0;
    }
    y += 34.0;

    y = section(s, y, "卡片、换行、高亮短语", &title);
    let card = Area::new(PAD, y, WIDTH - 2.0 * PAD, 0.0);
    let inner = card.width - 24.0;
    let text_style = PanelText::new(12.0, 18.0, tokens::TEXT);
    let spans = [
        RichSpan::plain("这句话的意思是「事情很简单」，常和 "),
        RichSpan::bold("a piece of cake").colored(tokens::PRIMARY),
        RichSpan::plain(
            " 一起出现：It was a piece of cake, honestly. 比 easy 更口语，也更有画面感。",
        ),
    ];
    let measured = s.measure_rich(&spans, &text_style, inner);
    let card = Area::new(card.x, y, card.width, measured.height + 44.0);
    s.fill_bordered(card, tokens::RADIUS_CARD, tokens::CARD_BLUE, tokens::BORDER);
    s.fill(
        Area::new(card.x + 12.0, card.y + 10.0, 22.0, 22.0),
        tokens::RADIUS_BUTTON,
        tokens::ICON_CHIP_BLUE,
    );
    s.icon(
        Icon::Lightbulb,
        card.x + 15.0,
        card.y + 13.0,
        16.0,
        tokens::TITLE_BLUE,
    );
    s.text(
        card.x + 42.0,
        card.y + 12.0,
        "意思",
        &PanelText::new(13.0, 18.0, tokens::TITLE_BLUE).bolded(),
    );
    s.rich(card.x + 12.0, card.y + 38.0, inner, &spans, &text_style);
    y = card.bottom() + 12.0;

    y = section(s, y, "对话：头像 A 在左、B 在右", &title);
    s.avatar(Avatar::Boy, PAD, y, 32.0);
    let a = Area::new(PAD + 40.0, y, 240.0, 32.0);
    s.fill_bordered(a, tokens::RADIUS_CARD, tokens::CARD_BG, tokens::BORDER);
    s.text(a.x + 10.0, a.y + 7.0, "How was the exam?", &body);
    y += 40.0;
    s.avatar(Avatar::Girl, WIDTH - PAD - 32.0, y, 32.0);
    let b = Area::new(WIDTH - PAD - 40.0 - 240.0, y, 240.0, 32.0);
    s.fill_bordered(b, tokens::RADIUS_CARD, tokens::SELECTED_BG, tokens::BORDER);
    s.text(b.x + 10.0, b.y + 7.0, "A piece of cake, honestly.", &body);
    y += 48.0;

    y = section(s, y, "错误条", &title);
    let error = Area::new(PAD, y, WIDTH - 2.0 * PAD, 40.0);
    s.fill_bordered(
        error,
        tokens::RADIUS_BUTTON,
        tokens::ERROR_BG,
        tokens::ERROR_BORDER,
    );
    s.icon(
        Icon::CircleAlert,
        error.x + 12.0,
        error.y + 11.0,
        18.0,
        tokens::ERROR_FG,
    );
    s.text(
        error.x + 38.0,
        error.y + 11.0,
        "生成失败，点此重试",
        &PanelText::new(12.0, 18.0, tokens::ERROR_FG),
    );
    y += 40.0 + PAD;
    y
}

/// 小节标题，返回下一行的 y。
fn section<H>(s: &mut PanelSurface<'_, H>, y: f32, label: &str, style: &PanelText) -> f32 {
    s.text(PAD, y, label, style);
    s.hline(PAD, y + 22.0, WIDTH - 2.0 * PAD, tokens::BORDER);
    y + 30.0
}

/// 画一枚胶囊，返回它的宽度。
fn chip<H>(s: &mut PanelSurface<'_, H>, x: f32, y: f32, label: &str, pair: ColorPair) -> f32 {
    let style = PanelText::new(11.0, 16.0, pair.fg);
    let width = s.measure(label, &style) + 16.0;
    s.fill(Area::new(x, y, width, 22.0), tokens::RADIUS_CHIP, pair.bg);
    s.text(x + 8.0, y + 3.0, label, &style);
    width
}
