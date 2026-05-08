use serde::{Deserialize, Serialize};
use types::{Address, B256, Bloom};

pub type BlockNumber = u64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Log {
    pub address: Address,
    pub topics: Vec<B256>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub transaction_hash: B256,
    pub transaction_index: u64,
    pub block_hash: B256,
    pub block_number: BlockNumber,
    pub from: Address,
    pub to: Option<Address>,
    pub contract_address: Option<Address>,
    pub cumulative_gas_used: u64,
    pub effective_gas_price: u128,
    pub gas_used: u64,
    pub status: bool,
    pub logs: Vec<Log>,
    pub logs_bloom: Bloom,
}
