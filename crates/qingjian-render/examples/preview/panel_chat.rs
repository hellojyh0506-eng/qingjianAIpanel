//! `--panel ai-chat*`：AI 问答面板预览，数据取自严格参考图里的「Do your best」会话。

use std::path::Path;

use qingjian_render::chat::{ChatMessage, ChatView, Phase, Role, SessionItem};
use qingjian_render::{FontLibrary, PanelRenderer, Shadow};

/// 严格参考图里的窗口只有约 708 × 270（截图被裁过），对照用。
const STRICT_SIZE: (f32, f32) = (708.0, 269.0);

/// 预览名 → 视图；不认识返回 `None`。
fn view_for(name: &str) -> Option<ChatView> {
    let mut view = base();
    match name {
        "ai-chat-strict" => {
            (view.width, view.height) = STRICT_SIZE;
        }
        "ai-chat" => {}
        "ai-chat-conversation" => conversation(&mut view),
        "ai-chat-reasoning" => {
            conversation(&mut view);
            view.deep = true;
            view.messages[1].set_reasoning_open(true);
            view.notice = Some("已复制".into());
        }
        "ai-chat-streaming" => {
            conversation(&mut view);
            let mut partial = ChatMessage::new(Role::Assistant, "");
            partial.push_reasoning("用户想知道 do your best 的用法，先想想语气……");
            partial.push_text("**Do your best** 常用在鼓励别人时：\n- 考试前：");
            view.messages.push(partial);
            view.phase = Phase::Streaming;
        }
        "ai-chat-waiting" => {
            conversation(&mut view);
            view.phase = Phase::Waiting;
            view.spin = 40.0;
        }
        "ai-chat-error" => {
            conversation(&mut view);
            view.messages
                .push(ChatMessage::new(Role::Error, "请求失败：网络连接超时"));
        }
        "ai-chat-input" => {
            view.input = "How do I say 尽力而为 in a more casual way?\nAnd when is it too casual to use with my boss at work?".into();
            view.cursor = view.input.len();
            view.web = true;
            view.deep = true;
        }
        "ai-chat-composition" => {
            view.input = "什么时候".into();
            view.cursor = view.input.len();
            view.composition = "yongdao".into();
        }
        "ai-chat-maximized" => {
            view.width = 1280.0;
            view.height = 780.0;
            view.maximized = true;
            view.pinned = true;
            for i in 0..14 {
                view.sessions.push(SessionItem {
                    title: format!("会话 {i}"),
                    subtitle: format!("{}条消息 · 10/{:02}", i + 2, 20 - i.min(19)),
                    selected: false,
                });
            }
            conversation(&mut view);
        }
        "ai-chat-free" => {
            view.topic = None;
            view.suggestions = vec![
                "怎么说更地道？".into(),
                "请给我 IELTS 口语题".into(),
                "美国年轻人会怎么说？".into(),
            ];
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
    let Some(mut view) = view_for(name) else {
        return Err(format!("没有 `{name}` 预览").into());
    };
    let mut renderer = PanelRenderer::new(FontLibrary::system(locale)?);
    renderer.set_scale(scale);
    let frame =
        qingjian_render::chat::render(&mut renderer, &mut view, shadow.then(Shadow::mac_panel))?;
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

fn base() -> ChatView {
    let sessions = [
        ("Do your best", "3条消息 · 10/24", true),
        ("雅思口语练习", "6条消息 · 10/08", false),
        ("日常口语表达", "4条消息 · 10/07", false),
        ("已保存的会话", "8条消息 · 10/06", false),
    ];
    ChatView {
        topic: Some("Do your best".into()),
        sessions: sessions
            .into_iter()
            .map(|(title, subtitle, selected)| SessionItem {
                title: title.into(),
                subtitle: subtitle.into(),
                selected,
            })
            .collect(),
        suggestions: vec![
            "什么时候用 Do your best？".into(),
            "请给我 IELTS 口语题".into(),
            "和 Give it your all 有什么区别？".into(),
            "美国年轻人会怎么说？".into(),
        ],
        ..ChatView::default()
    }
}

fn conversation(view: &mut ChatView) {
    view.messages.push(ChatMessage::new(
        Role::User,
        "什么时候用 Do your best？和 Try your best 有区别吗？",
    ));
    let mut answer = ChatMessage::new(
        Role::Assistant,
        "## 用法\n**Do your best** 是鼓励别人时最常用的一句，意思是「尽你所能」。\n\n- 考试前、比赛前对朋友说\n- 对自己做心理建设：`I'll do my best.`\n1. 语气温和，不带压力\n2. 比 *Try your best* 更口语\n> 注意：对老板说时更常用 I'll do my utmost.\n\n```\nGood luck! Just do your best.\n```\n希望对你有帮助。",
    );
    answer.push_reasoning("先对比两个说法的语气，再给出场景和例句。");
    answer.set_sources(vec!["Cambridge Dictionary · do your best".into()]);
    view.messages.push(answer);
}
