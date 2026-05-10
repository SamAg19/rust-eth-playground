use crate::B256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainHead {
    pub number: u64,
    pub hash: B256,
    pub total_difficulty: u128,
}

impl Default for ChainHead {
    fn default() -> Self {
        Self {
            number: 0,
            hash: B256::zero(),
            total_difficulty: 0,
        }
    }
}
