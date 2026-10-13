//! 英文朗读：用系统自带的 SAPI 语音，只选美式英语（en-US）。
//!
//! 没装美音就报错让面板提示，不回退英音或别的语言，免得读出来口音不对。COM 对象只在 UI 线程里用。

use windows::Win32::Media::Speech::{
    ISpObjectToken, ISpObjectTokenCategory, ISpVoice, SPCAT_VOICES, SPF_ASYNC,
    SPF_PURGEBEFORESPEAK, SpObjectTokenCategory, SpVoice,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::core::{HSTRING, PCWSTR, w};

/// SAPI 语言属性里美式英语的 LCID（0x409）。
const EN_US_ATTRIBUTE: PCWSTR = w!("Language=409");

/// 朗读失败的原因，面板据此给提示。
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SpeakError {
    /// 系统里没有美式英语语音。
    NoAmericanVoice,

    /// SAPI 本身不可用或调用失败。
    Unavailable,
}

impl SpeakError {
    /// 给用户看的提示。
    pub(super) fn notice(&self) -> &'static str {
        match self {
            Self::NoAmericanVoice => "未安装美式英语语音",
            Self::Unavailable => "朗读不可用",
        }
    }
}

/// 一个已选好美音的 SAPI 朗读器。
pub(super) struct Speaker {
    voice: ISpVoice,
}

impl Speaker {
    pub(super) fn new() -> Result<Self, SpeakError> {
        // 已经初始化过（S_FALSE）或线程里是别的套间模式都不影响 SAPI 使用，不当作失败。
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let voice: ISpVoice = unsafe { CoCreateInstance(&SpVoice, None, CLSCTX_ALL) }
            .map_err(|_| SpeakError::Unavailable)?;
        let token = american_voice().ok_or(SpeakError::NoAmericanVoice)?;
        unsafe { voice.SetVoice(&token) }.map_err(|_| SpeakError::Unavailable)?;
        Ok(Self { voice })
    }

    /// 异步朗读，打断正在读的那句。
    pub(super) fn speak(&self, text: &str) -> Result<(), SpeakError> {
        let text = HSTRING::from(text);
        let flags = (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0) as u32;
        unsafe { self.voice.Speak(PCWSTR(text.as_ptr()), flags, None) }
            .map_err(|_| SpeakError::Unavailable)
    }
}

/// 枚举系统语音，取第一个美式英语的。
fn american_voice() -> Option<ISpObjectToken> {
    unsafe {
        let category: ISpObjectTokenCategory =
            CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_ALL).ok()?;
        category.SetId(SPCAT_VOICES, false).ok()?;
        let tokens = category.EnumTokens(EN_US_ATTRIBUTE, PCWSTR::null()).ok()?;
        let mut count = 0u32;
        tokens.GetCount(&mut count).ok()?;
        if count == 0 {
            return None;
        }
        tokens.Item(0).ok()
    }
}
