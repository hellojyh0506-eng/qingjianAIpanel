//! 窗口里视图模型的状态转换：区块到达、选标签、选场景、失败重试。不碰 Win32，纯数据，可以脱离窗口测试。

use qingjian_predict::companion::{Block, Speaker};
use qingjian_render::Avatar;
use qingjian_render::companion::{Alternative, CompanionView, Detail, Example, TabStatus, Turn};

/// 打开一条新表达：全部清空，语感解析标签直接进入加载。
pub(super) fn reset(view: &mut CompanionView, zh: &str, en: &str) {
    let pinned = view.pinned;
    *view = CompanionView::new(zh, en);
    view.pinned = pinned;
    view.nuance_status = TabStatus::Loading;
}

/// 切到标签 `tab`；这个标签还没请求过就置为加载中并返回 `true`，调用方据此发请求。
pub(super) fn select_tab(view: &mut CompanionView, tab: usize) -> bool {
    let tab = tab.min(2);
    view.tab = tab;
    view.scroll = 0.0;
    view.scene_menu_open = false;
    let status = status_mut(view, tab);
    if *status == TabStatus::Idle {
        *status = TabStatus::Loading;
        true
    } else {
        false
    }
}

/// 切场景：清掉旧对话，进入加载；同一个场景再点不动，返回 `false`。
pub(super) fn select_scene(view: &mut CompanionView, scene: usize) -> bool {
    view.scene_menu_open = false;
    if view.scene == scene && view.dialogue_status != TabStatus::Idle {
        return false;
    }
    view.scene = scene;
    view.dialogue = Default::default();
    view.dialogue_status = TabStatus::Loading;
    view.scroll = 0.0;
    true
}

/// 重新生成一个标签：清掉它的内容，回到加载中。
pub(super) fn retry(view: &mut CompanionView, tab: usize) {
    match tab {
        0 => view.nuance = Default::default(),
        1 => view.alternatives.clear(),
        _ => view.dialogue = Default::default(),
    }
    *status_mut(view, tab) = TabStatus::Loading;
    view.scroll = 0.0;
}

/// 一个区块到了：填进对应卡片，状态转成流式中。
/// 对话区块属于哪个场景由 `scene` 说了算，与当前场景不一致（用户已经切走）就丢掉。
pub(super) fn apply_block(view: &mut CompanionView, tab: usize, scene: usize, block: Block) {
    if tab == 2 && scene != view.scene {
        return;
    }
    let status = status_mut(view, tab);
    if matches!(status, TabStatus::Idle | TabStatus::Loading) {
        *status = TabStatus::Streaming;
    }
    match block {
        Block::Header { tags, level } => {
            view.tags = tags;
            view.level = level;
        }
        Block::Meaning { text } => view.nuance.meaning = text,
        Block::Tone { text } => view.nuance.tone = text,
        Block::Usage { text } => view.nuance.usage = text,
        Block::Example { en, zh, hl } => view.nuance.example = Some(Example { en, zh, hl }),
        Block::Detail { title, body } => view.nuance.details.push(Detail { title, body }),
        Block::Alt {
            expr,
            zh,
            tag,
            tone,
            diff,
            ex_en,
            ex_zh,
            ..
        } => view.alternatives.push(Alternative {
            expression: expr,
            zh,
            tag,
            tone,
            diff,
            example: Example {
                en: ex_en,
                zh: ex_zh,
                hl: String::new(),
            },
        }),
        Block::Scene { title } => view.dialogue.title = title,
        Block::Question { en, zh } => view.dialogue.turns.push(Turn {
            speaker: Avatar::Boy,
            en,
            zh,
            hl: String::new(),
        }),
        Block::Turn { sp, en, zh, hl } => view.dialogue.turns.push(Turn {
            speaker: match sp {
                Speaker::A => Avatar::Boy,
                Speaker::B => Avatar::Girl,
            },
            en,
            zh,
            hl,
        }),
        Block::Point { text } => view.dialogue.points.push(text),
    }
}

/// 一路流正常结束。
pub(super) fn finish(view: &mut CompanionView, tab: usize, scene: usize) {
    if tab == 2 && scene != view.scene {
        return;
    }
    *status_mut(view, tab) = TabStatus::Done;
}

/// 一路流失败；已经显示出来的内容留着，状态条提示重试。
pub(super) fn fail(view: &mut CompanionView, tab: usize, scene: usize, message: String) {
    if tab == 2 && scene != view.scene {
        return;
    }
    *status_mut(view, tab) = TabStatus::Failed(message);
}

/// 任何一个标签还在生成：转圈动画要继续。
pub(super) fn is_busy(view: &CompanionView) -> bool {
    [
        &view.nuance_status,
        &view.alternatives_status,
        &view.dialogue_status,
    ]
    .into_iter()
    .any(|status| matches!(status, TabStatus::Loading | TabStatus::Streaming))
}

fn status_mut(view: &mut CompanionView, tab: usize) -> &mut TabStatus {
    match tab {
        0 => &mut view.nuance_status,
        1 => &mut view.alternatives_status,
        _ => &mut view.dialogue_status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opened() -> CompanionView {
        let mut view = CompanionView::default();
        reset(&mut view, "尽量", "Do your best");
        view
    }

    fn turn(sp: Speaker) -> Block {
        Block::Turn {
            sp,
            en: "Hi.".into(),
            zh: "你好。".into(),
            hl: String::new(),
        }
    }

    #[test]
    fn opening_starts_loading_the_first_tab_only_and_keeps_the_pin() {
        let mut view = CompanionView {
            pinned: true,
            scroll: 50.0,
            ..CompanionView::default()
        };
        reset(&mut view, "尽量", "Do your best");
        assert!(view.pinned);
        assert_eq!(view.scroll, 0.0);
        assert_eq!(view.nuance_status, TabStatus::Loading);
        assert_eq!(view.alternatives_status, TabStatus::Idle);
        assert_eq!(view.dialogue_status, TabStatus::Idle);
    }

    #[test]
    fn selecting_an_untouched_tab_asks_for_a_request_once() {
        let mut view = opened();
        assert!(select_tab(&mut view, 1));
        assert_eq!(view.alternatives_status, TabStatus::Loading);
        assert!(!select_tab(&mut view, 1), "已经在加载，不重复请求");
        assert!(!select_tab(&mut view, 0), "语感解析打开时就在请求");
        assert!(select_tab(&mut view, 9), "越界按最后一个标签处理");
        assert_eq!(view.tab, 2);
    }

    #[test]
    fn blocks_fill_cards_and_move_the_status_forward() {
        let mut view = opened();
        apply_block(
            &mut view,
            0,
            0,
            Block::Header {
                tags: vec!["自然口语".into()],
                level: "B1-B2".into(),
            },
        );
        assert_eq!(view.nuance_status, TabStatus::Streaming);
        assert_eq!(view.level, "B1-B2");
        apply_block(
            &mut view,
            0,
            0,
            Block::Meaning {
                text: "尽力".into(),
            },
        );
        apply_block(
            &mut view,
            0,
            0,
            Block::Detail {
                title: "a".into(),
                body: "b".into(),
            },
        );
        assert_eq!(view.nuance.meaning, "尽力");
        assert_eq!(view.nuance.details.len(), 1);
        finish(&mut view, 0, 0);
        assert_eq!(view.nuance_status, TabStatus::Done);
    }

    #[test]
    fn dialogue_blocks_for_a_scene_the_user_left_are_dropped() {
        let mut view = opened();
        select_tab(&mut view, 2);
        assert!(select_scene(&mut view, 2));
        apply_block(&mut view, 2, 0, turn(Speaker::A));
        assert!(view.dialogue.turns.is_empty());
        assert_eq!(view.dialogue_status, TabStatus::Loading);
        apply_block(
            &mut view,
            2,
            2,
            Block::Question {
                en: "Q?".into(),
                zh: "问？".into(),
            },
        );
        apply_block(&mut view, 2, 2, turn(Speaker::B));
        let speakers: Vec<_> = view.dialogue.turns.iter().map(|t| t.speaker).collect();
        assert_eq!(speakers, [Avatar::Boy, Avatar::Girl]);
        fail(&mut view, 2, 0, "旧场景的错误".into());
        assert_eq!(view.dialogue_status, TabStatus::Streaming);
        fail(&mut view, 2, 2, "超时".into());
        assert_eq!(view.dialogue_status, TabStatus::Failed("超时".into()));
    }

    #[test]
    fn same_scene_is_a_no_op_but_retry_clears_and_reloads() {
        let mut view = opened();
        select_tab(&mut view, 2);
        apply_block(&mut view, 2, 0, turn(Speaker::A));
        finish(&mut view, 2, 0);
        assert!(!select_scene(&mut view, 0));
        assert_eq!(view.dialogue.turns.len(), 1);
        retry(&mut view, 2);
        assert!(view.dialogue.turns.is_empty());
        assert_eq!(view.dialogue_status, TabStatus::Loading);
    }

    #[test]
    fn busy_means_any_tab_is_still_generating() {
        let mut view = opened();
        assert!(is_busy(&view));
        finish(&mut view, 0, 0);
        assert!(!is_busy(&view));
        select_tab(&mut view, 1);
        assert!(is_busy(&view));
    }
}
