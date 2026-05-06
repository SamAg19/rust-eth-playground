use rlp_codec::signing::SignedTransaction;
use types::{Address, B256, Bloom};

pub type BlockNumber = u64;

#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub block_number: BlockNumber,
    pub parent_hash: B256,
    pub state_root: B256,
    pub transactions_root: B256,
    pub receipts_root: B256,
    pub logs_bloom: Bloom,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub base_fee_per_gas: u128,
    pub hash: B256,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub header: Header,
    pub transactions: Vec<SignedTransaction>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Log {
    pub address: Address,
    pub topics: Vec<B256>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct AccountInfo {
    pub balance: u128,
    pub nonce: u64,
    pub code_hash: B256,
    pub code: Option<Vec<u8>>,
}

impl Default for AccountInfo {
    fn default() -> Self {
        let byte_arr: [u8; 32] = [
            0xc5, 0xd2, 0x46, 0x01, 0x86, 0xf7, 0x23, 0x3c, 0x92, 0x7e, 0x7d, 0xb2, 0xdc, 0xc7,
            0x03, 0xc0, 0xe5, 0x00, 0xb6, 0x53, 0xca, 0x82, 0x27, 0x3b, 0x7b, 0xfa, 0xd8, 0x04,
            0x5d, 0x85, 0xa4, 0x70,
        ];
        let default_hash = B256::from(byte_arr);
        Self {
            balance: 0,
            nonce: 0,
            code_hash: default_hash,
            code: None,
        }
    }
}
