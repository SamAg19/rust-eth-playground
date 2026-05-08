use serde::{Deserialize, Serialize};

use crate::{Address, B256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub parent_hash: B256,
    pub beneficiary: Address,
    pub state_root: B256,
    pub transactions_root: B256,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    pub number: u64,
}
