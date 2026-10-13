//! 学习助手面板的落点：优先停在组句矩形右侧，放不下换下方、上方、左侧，都不行贴工作区右下角；用户拖过就听用户的。

use windows::Win32::Foundation::{POINT, RECT};

use crate::ui::monitor;

/// 窗口与组句矩形之间的空隙（物理像素）。
const GAP: i32 = 8;

/// 兜底位置离工作区右下角的距离。
const CORNER_RIGHT: i32 = 20;
const CORNER_BOTTOM: i32 = 60;

/// 把内容左上角 `pos` 夹进它所在显示器的工作区；`margin` 是阴影留白（物理像素）。
pub(super) fn clamp_to_work_area(pos: (i32, i32), content: (i32, i32), margin: i32) -> (i32, i32) {
    let work = monitor::work_area_near(POINT { x: pos.0, y: pos.1 });
    let x = pos.0.clamp(
        work.left + margin,
        (work.right - content.0 - margin).max(work.left),
    );
    let y = pos.1.clamp(
        work.top + margin,
        (work.bottom - content.1 - margin).max(work.top),
    );
    (x, y)
}

/// 没有用户位置时，靠近 `anchor`（组句矩形）摆。
pub(super) fn near_anchor(anchor: RECT, content: (i32, i32), margin: i32) -> (i32, i32) {
    let work = monitor::work_area_near(POINT {
        x: anchor.left,
        y: anchor.top,
    });
    let fit_x = |x: i32| {
        x.clamp(
            work.left + margin,
            (work.right - content.0 - margin).max(work.left),
        )
    };
    let fit_y = |y: i32| {
        y.clamp(
            work.top + margin,
            (work.bottom - content.1 - margin).max(work.top),
        )
    };

    let right_x = anchor.right + GAP;
    if right_x + content.0 + margin <= work.right {
        return (right_x, fit_y(anchor.top));
    }
    let bottom_y = anchor.bottom + GAP;
    if bottom_y + content.1 + margin <= work.bottom {
        return (fit_x(anchor.left), bottom_y);
    }
    let top_y = anchor.top - content.1 - GAP;
    if top_y >= work.top + margin {
        return (fit_x(anchor.left), top_y);
    }
    let left_x = anchor.left - content.0 - GAP;
    if left_x >= work.left + margin {
        return (left_x, fit_y(anchor.top));
    }
    (
        (work.right - content.0 - margin - CORNER_RIGHT).max(work.left + margin),
        (work.bottom - content.1 - margin - CORNER_BOTTOM).max(work.top + margin),
    )
}
