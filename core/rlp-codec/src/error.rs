use thiserror::Error;
use types::TransactionError;

#[derive(Debug, Error)]
pub enum RlpError {
    #[error("Bytes length expected end at {expected} but got {actual}")]
    InputTooShort { expected: usize, actual: usize },
    #[error("Invalid RLP length prefix: {0}")]
    InvalidLength(usize),
    #[error("Unexpected trailing bytes: {count}")]
    TrailingBytes { count: usize },
    #[error("Unexpected RLP type: {0}")]
    UnexpectedType(u8),
    #[error("RLP encoding overflow")]
    Overflow,
    #[error(transparent)]
    Transaction(#[from] TransactionError),
}
