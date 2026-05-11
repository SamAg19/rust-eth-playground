use execution::error::ExecutionError;
use rlp_codec::{RlpError, signing::SigningError, trie::TrieError};
use thiserror::Error;
use types::Address;

#[derive(Debug, Error)]
pub enum BuilderError {
    #[error("unknown sender index: {index}")]
    UnknownSenderIndex { index: usize },
    #[error("missing account snapshot for sender {address}")]
    MissingAccountSnapshot { address: Address },
    #[error(
        "insufficient balance for sender {address}: available {available}, required {required}"
    )]
    InsufficientBalance {
        address: Address,
        available: u128,
        required: u128,
    },
    #[error("overflow while building transaction")]
    Overflow,
    #[error(transparent)]
    Hashing(#[from] RlpError),
    #[error(transparent)]
    Execution(#[from] ExecutionError),
    #[error(transparent)]
    Trie(#[from] TrieError),
    #[error(transparent)]
    Signing(#[from] SigningError),
}
