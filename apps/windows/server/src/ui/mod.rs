//! Server 进程内的 UI 线程：候选窗口与悬浮状态条都在这条线程上自绘（普通置顶窗会被商店 / 任务栏搜索
//! 这些高 z-band 宿主盖住，只有本进程配合 uiAccess 签名才能盖过）。
//!
//! HWND 线程亲和：Router 在工人线程上产出内容，经通道 + `PostThreadMessageW` 唤醒交给 UI 线程应用。
//! 线程句柄是 [`UiHandle`]，命令在 [`command`]，候选窗口在 [`candidates`]，状态条在 [`status`]，分层窗口合成在 [`layered`]。

mod candidates;
mod command;
pub mod companion;
pub mod drill;
pub mod ai_assistant;
mod layered;
mod monitor;
mod painter;
mod status;
mod window_class;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use windows::Win32::Foundation::{E_FAIL, HINSTANCE, LPARAM, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ASFW_ANY, AllowSetForegroundWindow, DispatchMessageW, GetMessageW, MSG, PostThreadMessageW,
    TranslateMessage, WM_APP,
};
use windows::core::{Error, Result};

use qingjian_platform::protocol::{Frame, ScreenRect};

use self::candidates::CandidateWindow;
use self::command::UiCommand;
use self::painter::{Painter, SharedPainter};
use self::status::StatusBar;
use crate::dispatch::{
    CandidateAction, CandidateSink, RenderSettings, StatusEvent, StatusSink, StatusView,
};

/// 状态条上的操作（点格子 / 拖动结束）回给 Router 的回调，UI 线程上调。
pub type StatusEvents = Box<dyn Fn(StatusEvent) + Send>;

/// 候选卡片上的交互动作回给 Router 的回调，UI 线程上调。
pub type CandidateEvents = Box<dyn Fn(CandidateAction) + Send>;

/// 唤醒 UI 线程去排空命令队列的线程消息。
const WM_WAKE: u32 = WM_APP;

/// UI 线程句柄：发命令 = 投进通道 + 一条 `WM_WAKE`。线程 detached，随进程存活。
#[derive(Clone)]
pub struct UiHandle {
    /// 命令通道的发送端。
    sender: Sender<UiCommand>,

    /// UI 线程 id，`PostThreadMessageW` 用。
    thread_id: u32,
}

impl UiHandle {
    /// 起 UI 线程并等它建好候选窗口。失败返回 `Err`，调用方退化为不画。
    pub fn spawn(on_status: StatusEvents, on_candidate: CandidateEvents) -> Result<Self> {
        // 用 Option<u32> 而非 Result 回报，免得 windows Error 跨线程。
        let (ready_tx, ready_rx) = mpsc::channel::<Option<u32>>();
        let (command_tx, command_rx) = mpsc::channel::<UiCommand>();
        thread::Builder::new()
            .name("qingjian-candidates".to_owned())
            .spawn(move || run(command_rx, &ready_tx, on_status, on_candidate))
            .map_err(|_| Error::from(E_FAIL))?;
        match ready_rx.recv() {
            Ok(Some(thread_id)) => Ok(Self {
                sender: command_tx,
                thread_id,
            }),
            _ => Err(Error::from(E_FAIL)),
        }
    }

    /// 线程已退出（通道断）时静默丢弃。
    fn post(&self, command: UiCommand) {
        if self.sender.send(command).is_ok() {
            let _ = unsafe { PostThreadMessageW(self.thread_id, WM_WAKE, WPARAM(0), LPARAM(0)) };
        }
    }
}

impl CandidateSink for UiHandle {
    fn show(&self, frame: Frame, rect: ScreenRect) {
        self.post(UiCommand::Show(Box::new((frame, rect))));
    }

    fn hide(&self) {
        self.post(UiCommand::Hide);
    }

    fn configure(&self, settings: RenderSettings) {
        self.post(UiCommand::Configure(settings));
    }

    fn show_companion(&self, data: companion::CompanionData, rect: ScreenRect) {
        self.post(UiCommand::CompanionShow(Box::new((data, rect))));
    }

    fn hide_companion(&self) {
        self.post(UiCommand::CompanionHide);
    }

    fn start_drill(&self, target_chinese: String, reference: String, rect: ScreenRect) {
        self.post(UiCommand::DrillStart(Box::new((target_chinese, reference, rect))));
    }

    fn show_drill_result(&self, result: drill::DrillResultData) {
        self.post(UiCommand::DrillResult(Box::new(result)));
    }

    fn hide_drill(&self) {
        self.post(UiCommand::DrillHide);
    }

    fn show_ai_assistant(&self, anchor: Option<windows::Win32::Foundation::RECT>, initial_prompt: Option<String>) {
        self.post(UiCommand::AiAssistantShow(Box::new((anchor, initial_prompt))));
    }

    fn hide_ai_assistant(&self) {
        self.post(UiCommand::AiAssistantHide);
    }

    fn send_ai_chunk(&self, chunk: String) {
        self.post(UiCommand::AiAssistantChunk(chunk));
    }

    fn send_ai_web_sources(&self, sources: Vec<qingjian_predict::SearchResult>) {
        self.post(UiCommand::AiAssistantWebSources(sources));
    }

    fn send_ai_finish(&self, ttft_ms: u128, total_ms: u128) {
        self.post(UiCommand::AiAssistantFinish { ttft_ms, total_ms });
    }

    fn send_ai_error(&self, error: String) {
        self.post(UiCommand::AiAssistantError(error));
    }

    fn clone_sink(&self) -> Box<dyn CandidateSink> {
        Box::new(self.clone())
    }
}

impl StatusSink for UiHandle {
    fn show_status(&self, view: StatusView) {
        self.post(UiCommand::StatusShow(Box::new(view)));
    }

    fn hide_status(&self) {
        self.post(UiCommand::StatusHide);
    }

    fn open_settings(&self) {
        open_settings();
    }

    fn open_download(&self) {
        let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
        let opened = std::process::Command::new("explorer")
            .arg(qingjian_update::DOWNLOAD_URL)
            .spawn();
        if let Err(error) = opened {
            tracing::warn!(%error, "打开下载页失败");
        }
    }
}

/// 起与本 exe 同目录的设置程序。设置程序已开时由新实例把它带到前台，得先把前台权让出去。
pub(crate) fn open_settings() {
    let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    let exe = std::env::current_exe().map(|exe| exe.with_file_name("qingjian-settings.exe"));
    let spawned = exe.and_then(|exe| std::process::Command::new(exe).spawn());
    if let Err(error) = spawned {
        tracing::warn!(%error, "打开设置程序失败");
    }
}

/// 本进程 exe 的模块句柄（注册窗口类 / 建窗口用）。
pub(super) fn module_handle() -> HINSTANCE {
    let module = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
    HINSTANCE(module.0)
}

/// UI 线程主体：建窗口、报回线程 id、跑消息循环。
fn run(
    commands: Receiver<UiCommand>,
    ready: &Sender<Option<u32>>,
    on_status: StatusEvents,
    on_candidate: CandidateEvents,
) {
    // 按物理像素定位，与应用报来的组句屏幕矩形对齐；已设过会失败，忽略。
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let thread_id = unsafe { GetCurrentThreadId() };
    // 装上时随 Configure 命令建。
    let painter: SharedPainter = Rc::new(RefCell::new(None));
    let on_candidate_rc: Rc<Box<dyn Fn(CandidateAction) + Send>> = Rc::new(on_candidate);
    // 先建窗口再报 id：建窗口顺带建起本线程的消息队列，之后 PostThreadMessageW 才有处可投。
    let window = match CandidateWindow::new(painter.clone(), {
        let cb = on_candidate_rc.clone();
        Box::new(move |action| (cb)(action))
    }) {
        Ok(window) => window,
        Err(error) => {
            tracing::error!(%error, "建候选窗口失败，Server 将不显示候选框");
            let _ = ready.send(None);
            return;
        }
    };
    let status = match StatusBar::new(on_status, painter.clone()) {
        Ok(status) => Some(status),
        Err(error) => {
            tracing::error!(%error, "建悬浮状态条失败，将不显示状态条");
            None
        }
    };
    let companion = match companion::CompanionWindow::new({
        let cb = on_candidate_rc.clone();
        Box::new(move |action| (cb)(action))
    }) {
        Ok(companion) => Some(companion),
        Err(error) => {
            tracing::error!(%error, "建 English Assistant 窗口失败");
            None
        }
    };
    let drill = match drill::DrillWindow::new({
        let cb = on_candidate_rc.clone();
        Box::new(move |action| (cb)(action))
    }) {
        Ok(drill) => Some(drill),
        Err(error) => {
            tracing::error!(%error, "建 Drill 练习窗口失败");
            None
        }
    };
    let ai_assistant = match ai_assistant::AiAssistantWindow::new({
        let cb = on_candidate_rc.clone();
        Box::new(move |action| (cb)(action))
    }) {
        Ok(ai) => Some(ai),
        Err(error) => {
            tracing::error!(%error, "建 AI Assistant 窗口失败");
            None
        }
    };
    if ready.send(Some(thread_id)).is_err() {
        return;
    }
    let mut msg = MSG::default();
    loop {
        let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if got.0 <= 0 {
            break;
        }
        if msg.message == WM_WAKE {
            // 一次唤醒排空整个队列，保住 Hide→Show 的先后。
            while let Ok(command) = commands.try_recv() {
                apply(
                    &window,
                    status.as_ref(),
                    companion.as_ref().map(|c| c.as_ref()),
                    drill.as_ref().map(|d| d.as_ref()),
                    ai_assistant.as_ref().map(|a| a.as_ref()),
                    &painter,
                    command,
                );
            }
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn apply(
    window: &CandidateWindow,
    status: Option<&StatusBar>,
    companion: Option<&companion::CompanionWindow>,
    drill: Option<&drill::DrillWindow>,
    ai_assistant: Option<&ai_assistant::AiAssistantWindow>,
    painter: &SharedPainter,
    command: UiCommand,
) {
    match command {
        UiCommand::Show(payload) => {
            let (frame, rect) = *payload;
            window.set_content(&frame);
            window.show(to_win_rect(rect));
        }
        UiCommand::Hide => {
            window.hide();
            if let Some(comp) = companion {
                comp.hide();
            }
        }
        UiCommand::StatusShow(view) => {
            if let Some(status) = status {
                status.update(*view);
            }
        }
        UiCommand::StatusHide => {
            if let Some(status) = status {
                status.hide();
            }
        }
        UiCommand::Configure(settings) => Painter::configure(painter, &settings),
        UiCommand::CompanionShow(payload) => {
            let (data, rect) = *payload;
            if let Some(comp) = companion {
                comp.update_data(data);
                let _ = comp.show_at(to_win_rect(rect));
            }
        }
        UiCommand::CompanionHide => {
            if let Some(comp) = companion {
                comp.hide();
            }
        }
        UiCommand::DrillStart(payload) => {
            let (target_chinese, reference, rect) = *payload;
            if let Some(dr) = drill {
                dr.start_session(&target_chinese, &reference, to_win_rect(rect));
            }
        }
        UiCommand::DrillResult(payload) => {
            if let Some(dr) = drill {
                dr.set_result(*payload);
            }
        }
        UiCommand::DrillHide => {
            if let Some(dr) = drill {
                dr.hide();
            }
        }
        UiCommand::AiAssistantShow(payload) => {
            let (anchor, initial_prompt) = *payload;
            if let Some(ai) = ai_assistant {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    ai.show_or_focus(anchor, initial_prompt);
                }));
            }
        }
        UiCommand::AiAssistantHide => {
            if let Some(ai) = ai_assistant {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    ai.hide();
                }));
            }
        }
        UiCommand::AiAssistantChunk(chunk) => {
            if let Some(ai) = ai_assistant {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    ai.append_stream_chunk(&chunk);
                }));
            }
        }
        UiCommand::AiAssistantWebSources(sources) => {
            if let Some(ai) = ai_assistant {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    ai.set_web_sources(sources);
                }));
            }
        }
        UiCommand::AiAssistantFinish { ttft_ms, total_ms } => {
            if let Some(ai) = ai_assistant {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    ai.finish_stream(ttft_ms, total_ms);
                }));
            }
        }
        UiCommand::AiAssistantError(error) => {
            if let Some(ai) = ai_assistant {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    ai.set_stream_error(&error);
                }));
            }
        }
    }
}

fn to_win_rect(rect: ScreenRect) -> RECT {
    RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}
