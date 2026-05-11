use execution::error::ExecutionError;
use rlp_codec::{RlpError, signing::SigningError, trie::TrieError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProcessorError {
    #[error(transparent)]
    Hashing(#[from] RlpError),
    #[error(transparent)]
    Execution(#[from] ExecutionError),
    #[error(transparent)]
    Trie(#[from] TrieError),
    #[error(transparent)]
    Signing(#[from] SigningError),
    #[error("Error due to inability to read shared head")]
    SharedHeadOperationFailed(),
    #[error("Parent hash for block number {block_number} not found")]
    ParentNotFound { block_number: u64 },
    #[error("Expected block number is {expected}, but found {actual}")]
    InvalidBlockNumber { actual: u64, expected: u64 },
    #[error("{reason}")]
    ValidationFailed { reason: String },
}
