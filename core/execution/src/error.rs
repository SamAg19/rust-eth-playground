use rlp_codec::{RlpError, signing::SigningError, trie::TrieError};
use thiserror::Error;
use types::{Address, B256, TransactionError};

use crate::primitives::BlockNumber;

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("block {number} not found")]
    BlockNotFound { number: BlockNumber },
    #[error("header with hash {hash} not found")]
    HeaderNotFound { hash: B256 },
    #[error("transaction with hash {hash} not found")]
    TransactionNotFound { hash: B256 },
    #[error("receipt for transaction {hash} not found")]
    ReceiptNotFound { hash: B256 },
    #[error("account {address} not found")]
    AccountNotFound { address: Address },
    #[error("invalid parent hash: expected {expected}, got {actual}")]
    InvalidParentHash { expected: B256, actual: B256 },
    #[error("gas limit exceeded: used {used}, limit {limit}")]
    GasLimitExceeded { limit: u64, used: u64 },
    #[error("insufficient balance for {address}: available {available}, required {required}")]
    InsufficientBalance {
        address: Address,
        available: u128,
        required: u128,
    },
    #[error("invalid nonce for {address}: expected {expected}, got {actual}")]
    InvalidNonce {
        address: Address,
        expected: u64,
        actual: u64,
    },
    #[error("block body has too many transactions: limit {limit}, got {actual}")]
    TooManyTransactions { limit: usize, actual: usize },
    #[error("invalid block number: expected {expected}, got {actual}")]
    InvalidBlockNumber {
        expected: BlockNumber,
        actual: BlockNumber,
    },
    #[error("arithmetic overflow")]
    Overflow,
    #[error("arithmetic underflow")]
    Underflow,
    #[error(transparent)]
    Transaction(#[from] TransactionError),
    #[error(transparent)]
    Signing(#[from] SigningError),
    #[error(transparent)]
    RootCompute(#[from] TrieError),
    #[error(transparent)]
    Rlp(#[from] RlpError),
}
