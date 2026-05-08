use crate::{B256, Header, Transaction};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub header: Header,
    pub transactions: Vec<SignedTransaction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignedTransaction {
    pub transaction: Transaction,
    pub v: u64,
    pub r: B256,
    pub s: B256,
}

impl Block {
    pub fn header(&self) -> &Header {
        &self.header
    }
    pub fn transactions(&self) -> &[SignedTransaction] {
        &self.transactions
    }
}
