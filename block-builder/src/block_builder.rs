use std::collections::HashMap;

use rlp_codec::{
    hash_header,
    signing::{recover_sender, sign},
};
use serde::{Deserialize, Serialize};
use types::{Address, B256, Block, Header, SignedTransaction, Transaction, TransactionError};

use crate::error::BuilderError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionSpec {
    pub sender_index: usize,
    pub recipient: Address,
    pub value: u128,
    pub gas_limit: u64,
}

#[derive(Debug, Clone)]
pub struct BlockSpec {
    pub transactions: Vec<TransactionSpec>,
}

#[derive(Debug, Clone)]
pub struct AccountSnapshot {
    pub address: Address,
    pub nonce: u64,
    pub balance: u128,
}

#[derive(Debug)]
pub struct BuilderState {
    signing_keys: Vec<[u8; 32]>,
    current_number: u64,
    current_hash: B256,
    genesis_timestamp: u64,
    current_timestamp: u64,
    chain_id: u64,
}
pub struct BlockBuilder {
    pub state: BuilderState,
    pub genesis_gas_limit: u64,
}

impl BlockBuilder {
    const GAS_PRICE: u128 = 1_000_000_000;

    pub fn new(
        chain_id: u64,
        genesis_block_hash: B256,
        genesis_timestamp: u64,
        genesis_gas_limit: u64,
        signing_keys: Vec<[u8; 32]>,
    ) -> Result<Self, BuilderError> {
        let state = BuilderState {
            signing_keys,
            current_number: 0,
            current_hash: genesis_block_hash,
            genesis_timestamp,
            current_timestamp: genesis_timestamp,
            chain_id,
        };

        Ok(Self {
            state,
            genesis_gas_limit,
        })
    }

    pub fn chain_id(&self) -> u64 {
        self.state.chain_id
    }

    pub fn current_head(&self) -> (u64, B256) {
        (self.state.current_number, self.state.current_hash)
    }

    pub fn set_current_head(&mut self, number: u64, hash: B256) -> Result<(), BuilderError> {
        let timestamp_offset = number.checked_mul(12).ok_or(BuilderError::Overflow)?;
        let current_timestamp = self
            .state
            .genesis_timestamp
            .checked_add(timestamp_offset)
            .ok_or(BuilderError::Overflow)?;

        self.state.current_number = number;
        self.state.current_hash = hash;
        self.state.current_timestamp = current_timestamp;

        Ok(())
    }

    pub fn generate_block(
        &mut self,
        spec: BlockSpec,
        account_snapshots: HashMap<Address, AccountSnapshot>,
    ) -> Result<Block, BuilderError> {
        let mut transactions = Vec::with_capacity(spec.transactions.len());
        let mut gas_used = 0u64;

        for transaction_spec in &spec.transactions {
            gas_used = gas_used
                .checked_add(transaction_spec.gas_limit)
                .ok_or(BuilderError::Overflow)?;
            transactions.push(self.build_signed_transaction(transaction_spec, &account_snapshots)?);
        }

        let number = self
            .state
            .current_number
            .checked_add(1)
            .ok_or(BuilderError::Overflow)?;
        let timestamp_offset = number.checked_mul(12).ok_or(BuilderError::Overflow)?;
        let timestamp = self
            .state
            .genesis_timestamp
            .checked_add(timestamp_offset)
            .ok_or(BuilderError::Overflow)?;
        let header = Header {
            parent_hash: self.state.current_hash,
            beneficiary: Address::zero(),
            state_root: B256::zero(),
            transactions_root: B256::zero(),
            gas_limit: self.genesis_gas_limit,
            gas_used,
            timestamp,
            number,
        };
        let block_hash = hash_header(&header)?;
        let block = Block {
            header,
            transactions,
        };

        self.state.current_number = number;
        self.state.current_hash = block_hash;
        self.state.current_timestamp = timestamp;

        Ok(block)
    }

    fn sender_address(&self, private_key: &[u8; 32]) -> Result<Address, BuilderError> {
        let transaction = Transaction::Legacy {
            nonce: 0,
            gas_price: Self::GAS_PRICE,
            gas_limit: 21_000,
            to: None,
            value: 0,
            data: vec![],
        };
        let signed_transaction = sign(&transaction, private_key, self.state.chain_id)?;
        Ok(recover_sender(&signed_transaction, self.state.chain_id)?)
    }

    fn build_signed_transaction(
        &self,
        spec: &TransactionSpec,
        account_snapshots: &HashMap<Address, AccountSnapshot>,
    ) -> Result<SignedTransaction, BuilderError> {
        let private_key = self.state.signing_keys.get(spec.sender_index).ok_or(
            BuilderError::UnknownSenderIndex {
                index: spec.sender_index,
            },
        )?;
        let sender = self.sender_address(private_key)?;
        let snapshot = account_snapshots
            .get(&sender)
            .ok_or(BuilderError::MissingAccountSnapshot { address: sender })?;
        let transaction = Transaction::Legacy {
            nonce: snapshot.nonce,
            gas_price: Self::GAS_PRICE,
            gas_limit: spec.gas_limit,
            to: Some(spec.recipient),
            value: spec.value,
            data: vec![],
        };
        let max_cost = transaction.max_cost().map_err(|error| match error {
            TransactionError::Overflow => BuilderError::Overflow,
            _ => BuilderError::Overflow,
        })?;

        if max_cost > snapshot.balance {
            return Err(BuilderError::InsufficientBalance {
                address: sender,
                available: snapshot.balance,
                required: max_cost,
            });
        }

        Ok(sign(&transaction, private_key, self.state.chain_id)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshots(accounts: Vec<(Address, u64, u128)>) -> HashMap<Address, AccountSnapshot> {
        accounts
            .into_iter()
            .map(|(address, nonce, balance)| {
                (
                    address,
                    AccountSnapshot {
                        address,
                        nonce,
                        balance,
                    },
                )
            })
            .collect()
    }

    fn transaction_nonce(transaction: &SignedTransaction) -> u64 {
        match &transaction.transaction {
            Transaction::Legacy { nonce, .. } => *nonce,
            other => panic!("expected legacy transaction, got {other:?}"),
        }
    }

    fn block_nonces(block: &Block) -> Vec<u64> {
        block.transactions.iter().map(transaction_nonce).collect()
    }

    #[test]
    fn generate_block_links_three_blocks_and_assigns_nonces() {
        let chain_id = 1337;
        let genesis_hash = B256::from([0x11; 32]);
        let genesis_timestamp = 100;
        let sender_key_1 = [0x01; 32];
        let sender_key_2 = [0x02; 32];
        let mut builder = BlockBuilder::new(
            chain_id,
            genesis_hash,
            genesis_timestamp,
            30_000_000,
            vec![sender_key_1, sender_key_2],
        )
        .expect("builder should be created");
        let sender_1 = builder
            .sender_address(&sender_key_1)
            .expect("sender 1 should recover");
        let sender_2 = builder
            .sender_address(&sender_key_2)
            .expect("sender 2 should recover");
        let recipient = Address::from([0x44; 20]);
        let balance = 1_000_000_000_000_000_000;

        let block_1 = builder
            .generate_block(
                BlockSpec {
                    transactions: vec![TransactionSpec {
                        sender_index: 0,
                        recipient,
                        value: 100,
                        gas_limit: 21_000,
                    }],
                },
                snapshots(vec![(sender_1, 0, balance)]),
            )
            .expect("block 1 should generate");
        let block_1_hash = hash_header(&block_1.header).expect("block 1 hash should compute");

        let block_2 = builder
            .generate_block(
                BlockSpec {
                    transactions: vec![TransactionSpec {
                        sender_index: 0,
                        recipient,
                        value: 300,
                        gas_limit: 21_000,
                    }],
                },
                snapshots(vec![(sender_1, 1, balance)]),
            )
            .expect("block 2 should generate");
        let block_2_hash = hash_header(&block_2.header).expect("block 2 hash should compute");

        let block_3 = builder
            .generate_block(
                BlockSpec {
                    transactions: vec![TransactionSpec {
                        sender_index: 1,
                        recipient,
                        value: 400,
                        gas_limit: 21_000,
                    }],
                },
                snapshots(vec![(sender_2, 5, balance)]),
            )
            .expect("block 3 should generate");

        assert_eq!(block_1.header.parent_hash, genesis_hash);
        assert_eq!(block_2.header.parent_hash, block_1_hash);
        assert_eq!(block_3.header.parent_hash, block_2_hash);
        assert_eq!(block_1.header.number, 1);
        assert_eq!(block_2.header.number, 2);
        assert_eq!(block_3.header.number, 3);
        assert_eq!(block_1.header.timestamp, 112);
        assert_eq!(block_2.header.timestamp, 124);
        assert_eq!(block_3.header.timestamp, 136);
        assert_eq!(transaction_nonce(&block_1.transactions[0]), 0);
        assert_eq!(transaction_nonce(&block_2.transactions[0]), 1);
        assert_eq!(transaction_nonce(&block_3.transactions[0]), 5);
    }

    #[test]
    fn repeated_generate_block_calls_build_consistent_chain_from_supplied_snapshots() {
        let chain_id = 1337;
        let genesis_hash = B256::from([0x22; 32]);
        let genesis_timestamp = 1_000;
        let sender_key_1 = [0x01; 32];
        let sender_key_2 = [0x02; 32];
        let mut builder = BlockBuilder::new(
            chain_id,
            genesis_hash,
            genesis_timestamp,
            30_000_000,
            vec![sender_key_1, sender_key_2],
        )
        .expect("builder should be created");
        let sender_1 = builder
            .sender_address(&sender_key_1)
            .expect("sender 1 should recover");
        let sender_2 = builder
            .sender_address(&sender_key_2)
            .expect("sender 2 should recover");
        let recipient = Address::from([0x55; 20]);
        let balance = 1_000_000_000_000_000_000;
        let block_inputs = [
            (
                BlockSpec {
                    transactions: vec![
                        TransactionSpec {
                            sender_index: 0,
                            recipient,
                            value: 100,
                            gas_limit: 21_000,
                        },
                        TransactionSpec {
                            sender_index: 1,
                            recipient,
                            value: 200,
                            gas_limit: 21_000,
                        },
                    ],
                },
                snapshots(vec![(sender_1, 0, balance), (sender_2, 10, balance)]),
                vec![0, 10],
            ),
            (
                BlockSpec {
                    transactions: vec![
                        TransactionSpec {
                            sender_index: 1,
                            recipient,
                            value: 300,
                            gas_limit: 21_000,
                        },
                        TransactionSpec {
                            sender_index: 0,
                            recipient,
                            value: 400,
                            gas_limit: 21_000,
                        },
                    ],
                },
                snapshots(vec![(sender_1, 1, balance), (sender_2, 11, balance)]),
                vec![11, 1],
            ),
            (
                BlockSpec {
                    transactions: vec![
                        TransactionSpec {
                            sender_index: 0,
                            recipient,
                            value: 500,
                            gas_limit: 21_000,
                        },
                        TransactionSpec {
                            sender_index: 1,
                            recipient,
                            value: 600,
                            gas_limit: 21_000,
                        },
                    ],
                },
                snapshots(vec![(sender_1, 2, balance), (sender_2, 12, balance)]),
                vec![2, 12],
            ),
        ];
        let mut expected_parent_hash = genesis_hash;

        for (index, (spec, account_snapshots, expected_nonces)) in
            block_inputs.into_iter().enumerate()
        {
            let block = builder
                .generate_block(spec, account_snapshots)
                .expect("block should generate from supplied snapshots");

            assert_eq!(block.header.parent_hash, expected_parent_hash);
            assert_eq!(block.header.number, index as u64 + 1);
            assert_eq!(
                block.header.timestamp,
                genesis_timestamp + 12 * (index as u64 + 1)
            );
            assert_eq!(block_nonces(&block), expected_nonces);

            expected_parent_hash =
                hash_header(&block.header).expect("generated block hash should compute");
        }
    }

    #[test]
    fn set_current_head_resumes_next_block_number_parent_hash_and_timestamp() {
        let chain_id = 1337;
        let genesis_hash = B256::from([0x33; 32]);
        let resumed_hash = B256::from([0xaa; 32]);
        let genesis_timestamp = 1_000;
        let sender_key = [0x01; 32];
        let mut builder = BlockBuilder::new(
            chain_id,
            genesis_hash,
            genesis_timestamp,
            30_000_000,
            vec![sender_key],
        )
        .expect("builder should be created");
        let sender = builder
            .sender_address(&sender_key)
            .expect("sender should recover");
        let recipient = Address::from([0x66; 20]);

        builder
            .set_current_head(10, resumed_hash)
            .expect("builder should resume from known head");
        let block = builder
            .generate_block(
                BlockSpec {
                    transactions: vec![TransactionSpec {
                        sender_index: 0,
                        recipient,
                        value: 100,
                        gas_limit: 21_000,
                    }],
                },
                snapshots(vec![(sender, 10, 1_000_000_000_000_000_000)]),
            )
            .expect("block should generate from resumed head");

        assert_eq!(block.header.number, 11);
        assert_eq!(block.header.parent_hash, resumed_hash);
        assert_eq!(block.header.timestamp, genesis_timestamp + 11 * 12);
    }
}
