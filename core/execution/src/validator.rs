use crate::{
    error::ExecutionError,
    primitives::{Block, Header},
};

pub trait ConsensusValidator {
    fn validate_header(&self, header: &Header, parent: &Header) -> Result<(), ExecutionError>;
    fn validate_body(&self, block: &Block) -> Result<(), ExecutionError>;
}

pub struct BasicValidator {
    pub max_txs: usize,
}

impl ConsensusValidator for BasicValidator {
    fn validate_header(&self, header: &Header, parent: &Header) -> Result<(), ExecutionError> {
        if header.parent_hash != parent.hash {
            return Err(ExecutionError::InvalidParentHash {
                expected: parent.hash,
                actual: header.parent_hash,
            });
        }

        if header.gas_used > header.gas_limit {
            return Err(ExecutionError::GasLimitExceeded {
                limit: header.gas_limit,
                used: header.gas_used,
            });
        }

        Ok(())
    }
    fn validate_body(&self, block: &Block) -> Result<(), ExecutionError> {
        if block.transactions.len() > self.max_txs {
            return Err(ExecutionError::TooManyTransactions {
                limit: self.max_txs,
                actual: block.transactions.len(),
            });
        }
        Ok(())
    }
}

pub struct StrictValidator {
    pub max_txs: usize,
}

impl ConsensusValidator for StrictValidator {
    fn validate_header(&self, header: &Header, parent: &Header) -> Result<(), ExecutionError> {
        if header.parent_hash != parent.hash {
            return Err(ExecutionError::InvalidParentHash {
                expected: parent.hash,
                actual: header.parent_hash,
            });
        }

        let actual_block_number = parent
            .block_number
            .checked_add(1)
            .ok_or(ExecutionError::Overflow)?;
        if header.block_number != actual_block_number {
            return Err(ExecutionError::InvalidBlockNumber {
                expected: actual_block_number,
                actual: header.block_number,
            });
        }

        if header.gas_used > header.gas_limit {
            return Err(ExecutionError::GasLimitExceeded {
                limit: header.gas_limit,
                used: header.gas_used,
            });
        }

        Ok(())
    }
    fn validate_body(&self, block: &Block) -> Result<(), ExecutionError> {
        if block.transactions.len() > self.max_txs {
            return Err(ExecutionError::TooManyTransactions {
                limit: self.max_txs,
                actual: block.transactions.len(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;
    use types::{B256, Bloom};

    fn hash_from_number(number: u64) -> B256 {
        let mut bytes = [0u8; 32];
        bytes[24..].copy_from_slice(&number.to_be_bytes());
        B256::new(bytes)
    }

    fn header(number: u64, parent_hash: B256, gas_limit: u64, gas_used: u64) -> Header {
        Header {
            block_number: number,
            parent_hash,
            state_root: B256::zero(),
            transactions_root: B256::zero(),
            receipts_root: B256::zero(),
            logs_bloom: Bloom::zero(),
            gas_limit,
            gas_used,
            base_fee_per_gas: 1_000_000_000,
            hash: hash_from_number(number),
        }
    }

    prop_compose! {
        fn arb_header_chain()
            (gas_pairs in prop::collection::vec((1_000_000u64..=30_000_000, 0u64..=30_000_000), 1..=50))
            -> Vec<Header>
        {
            let genesis_gas_limit = gas_pairs[0].0;
            let genesis_gas_used = gas_pairs[0].1.min(genesis_gas_limit);
            let mut headers = vec![header(0, B256::zero(), genesis_gas_limit, genesis_gas_used)];

            for (idx, (gas_limit, gas_used)) in gas_pairs.into_iter().enumerate().skip(1) {
                let number = idx as u64;
                let parent_hash = headers[idx - 1].hash;
                headers.push(header(number, parent_hash, gas_limit, gas_used.min(gas_limit)));
            }

            headers
        }
    }

    proptest! {
        #[test]
        fn strict_validator_accepts_valid_header_chains(headers in arb_header_chain()) {
            let validator = StrictValidator { max_txs: 100 };

            for pair in headers.windows(2) {
                validator.validate_header(&pair[1], &pair[0])?;
            }
        }
    }
}
