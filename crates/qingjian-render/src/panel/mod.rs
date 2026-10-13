//! 面板渲染基础：学习助手与 AI 问答两个面板共用的画布、图标、头像、点击表与设计 token。
//!
//! 用法：`PanelRenderer::set_scale` 设好缩放 → 先用 `measure*` 量文字排布局 → `begin` 开画布 → 在 [`PanelSurface`] 上边画边 `hit` 登记点击区域 →
//! `finish` 拿位图与点击表。窗口过程只查 [`HitMap`]，不再另算一遍布局。设计与验收见 `docs/plan/ai-panel-rebuild.md` 的 P7。

mod area;
mod avatar;
mod avatar_cache;
mod color_pair;
mod hitmap;
mod icon;
mod panel_text;
mod renderer;
mod surface;
pub mod tokens;

pub use area::Area;
pub use avatar::Avatar;
pub use color_pair::ColorPair;
pub use hitmap::HitMap;
pub use icon::Icon;
pub use panel_text::PanelText;
pub use renderer::PanelRenderer;
pub use surface::{PanelFrame, PanelSurface};
