//! 测速工具的错误。

use qingjian_predict::PredictError;

#[derive(Debug, thiserror::Error)]
pub enum BenchError {
    #[error("--ai-bench needs at least one model in --models")]
    NoModels,

    #[error("sample file line {line}: {reason}")]
    Sample { line: usize, reason: &'static str },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Predict(#[from] PredictError),
}
