use crate::B256;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub balance: u128,
    pub nonce: u64,
    pub code_hash: B256,
}

impl Account {
    pub fn is_empty(&self) -> bool {
        self.balance == 0 && self.nonce == 0 && self.code_hash == B256::zero()
    }
}

impl Default for Account {
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
        }
    }
}
