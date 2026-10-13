//! 输入法锚定：把组字窗和候选窗贴到输入框光标处。组字文字由系统组字窗显示，面板只负责给它定位。

use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::Input::Ime::{
    CANDIDATEFORM, CFS_CANDIDATEPOS, CFS_POINT, COMPOSITIONFORM, ImmGetContext, ImmReleaseContext,
    ImmSetCandidateWindow, ImmSetCompositionWindow,
};

use super::AiAssistantWindow;

impl AiAssistantWindow {
    /// 光标在窗口客户区里的物理像素位置与高度；还没画过为 `None`。
    fn caret_client(&self) -> Option<(i32, i32, i32)> {
        let state = self.state.borrow();
        let placed = state.placed.as_ref()?;
        let (x, y, height) = placed.caret;
        Some((
            (x * placed.scale).round() as i32 + placed.margin,
            (y * placed.scale).round() as i32 + placed.margin,
            (height * placed.scale).round() as i32,
        ))
    }

    /// 让输入法的组字窗、候选窗落在光标处（候选窗在光标下方）。
    pub(super) fn anchor_ime(&self) {
        let Some((x, y, height)) = self.caret_client() else {
            return;
        };
        unsafe {
            let himc = ImmGetContext(self.hwnd);
            if himc.is_invalid() {
                return;
            }
            let composition = COMPOSITIONFORM {
                dwStyle: CFS_POINT,
                ptCurrentPos: POINT { x, y },
                rcArea: RECT::default(),
            };
            let _ = ImmSetCompositionWindow(himc, &composition);
            let candidate = CANDIDATEFORM {
                dwIndex: 0,
                dwStyle: CFS_CANDIDATEPOS,
                ptCurrentPos: POINT {
                    x,
                    y: y + height + 2,
                },
                rcArea: RECT::default(),
            };
            let _ = ImmSetCandidateWindow(himc, &candidate);
            let _ = ImmReleaseContext(self.hwnd, himc);
        }
    }
}
