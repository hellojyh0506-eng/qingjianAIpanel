//! AI 问答气泡里的 Markdown：标题、引用、列表、行内代码、粗体、代码块。
//!
//! 只做模型回答里常见的那几种；解析是纯函数，每次只重新解析正在流式追加的最后一个气泡。

mod block;
mod inline;
mod parse;

pub use block::Block;
pub use inline::Inline;
pub use parse::parse_blocks;
