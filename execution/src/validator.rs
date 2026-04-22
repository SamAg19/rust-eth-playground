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
