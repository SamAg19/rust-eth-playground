use rlp_codec::{
    hash_header,
    signing::{recover_sender, sign},
};
use types::{Account, Address, B256, Block, GAS_LIMIT_PER_BLOCK, Header, Transaction};

use crate::{
    error::ExecutionError,
    executor::{BlockWithSenders, ExecutionOutput, ValueTransferExecutor, compute_state_root},
    in_memory::InMemoryProvider,
    pipeline::Pipeline,
    providers::{BlockProvider, StateProvider},
    validator::BasicValidator,
};

const TEST_PRIVATE_KEY: [u8; 32] = [
    0x4c, 0x0c, 0x4d, 0x14, 0x6c, 0x46, 0xed, 0x91, 0xf6, 0xa9, 0x35, 0x09, 0x46, 0xa3, 0x69, 0x9e,
    0xb1, 0xfb, 0xc1, 0x9c, 0x91, 0xc8, 0x10, 0xe6, 0xb6, 0xa7, 0x8b, 0x0c, 0xa6, 0x06, 0x65, 0x6f,
];

const TEST_CHAIN_ID: u64 = 1;
const BASE_FEE: u128 = 1_000_000_000;

pub struct TestHarness {
    pub pipeline: Pipeline<ValueTransferExecutor, BasicValidator>,
    pub current_block_number: u64,
    pub genesis_hash: B256,
}

impl TestHarness {
    pub fn new() -> Result<Self, ExecutionError> {
        Self::with_genesis_accounts(vec![])
    }

    pub fn with_genesis_accounts(
        accounts: Vec<(Address, u128, u64)>,
    ) -> Result<Self, ExecutionError> {
        let mut provider = InMemoryProvider::default();
        for (address, balance, nonce) in accounts {
            provider.set_account(
                address,
                Account {
                    balance,
                    nonce,
                    ..Account::default()
                },
            );
        }

        let genesis_state_root = compute_state_root(&provider)?;
        let genesis_header = Header {
            parent_hash: B256::zero(),
            beneficiary: Address::zero(),
            state_root: genesis_state_root,
            transactions_root: B256::zero(),
            gas_limit: GAS_LIMIT_PER_BLOCK,
            gas_used: 0,
            timestamp: 0,
            number: 0,
        };
        let genesis_hash = hash_header(&genesis_header)?;
        let genesis = Block {
            header: genesis_header,
            transactions: vec![],
        };
        provider.insert_block(genesis)?;

        Ok(Self {
            pipeline: Pipeline::new(
                provider,
                ValueTransferExecutor,
                BasicValidator { max_txs: 1_000 },
            ),
            current_block_number: 0,
            genesis_hash,
        })
    }

    pub fn funded(address: Address, balance: u128) -> Result<Self, ExecutionError> {
        Self::with_genesis_accounts(vec![(address, balance, 0)])
    }

    pub fn fund_account(&mut self, address: Address, balance: u128, nonce: u64) {
        self.pipeline.provider.set_account(
            address,
            Account {
                balance,
                nonce,
                ..Account::default()
            },
        );
    }

    pub fn execute_block(
        &mut self,
        transactions: Vec<Transaction>,
    ) -> Result<ExecutionOutput, ExecutionError> {
        let signed = transactions
            .iter()
            .map(|tx| sign(tx, &TEST_PRIVATE_KEY, TEST_CHAIN_ID))
            .collect::<Result<Vec<_>, _>>()?;
        let senders = signed
            .iter()
            .map(|tx| recover_sender(tx, TEST_CHAIN_ID))
            .collect::<Result<Vec<_>, _>>()?;
        let gas_used = transactions.iter().try_fold(0u64, |acc, tx| {
            let gas_limit = match tx {
                Transaction::Legacy { gas_limit, .. }
                | Transaction::Eip1559 { gas_limit, .. }
                | Transaction::Eip4844 { gas_limit, .. } => *gas_limit,
                #[cfg(feature = "optimism")]
                Transaction::Deposit { gas_limit, .. } => *gas_limit,
            };
            acc.checked_add(gas_limit).ok_or(ExecutionError::Overflow)
        })?;
        let parent = self
            .pipeline
            .provider
            .get_block_by_number(self.current_block_number)?;

        let mut block = Block {
            header: Header {
                parent_hash: hash_header(&parent.header)?,
                beneficiary: Address::zero(),
                state_root: B256::zero(),
                transactions_root: B256::zero(),
                gas_limit: GAS_LIMIT_PER_BLOCK,
                gas_used,
                timestamp: (self.current_block_number + 1) * 12,
                number: self.current_block_number + 1,
            },
            transactions: signed,
        };
        let block_with_senders = BlockWithSenders {
            block: block.clone(),
            senders,
        };

        let output = self.pipeline.execute(&block_with_senders)?;
        block.header.state_root = output.state_root;
        self.pipeline.provider.insert_block(block)?;
        self.current_block_number += 1;
        Ok(output)
    }

    pub fn assert_account_balance(&self, address: Address, expected: u128) {
        let actual = match self.pipeline.provider.get_balance(address) {
            Ok(balance) => balance,
            Err(err) => panic!("failed to get balance for {address}: {err}"),
        };
        assert_eq!(
            actual, expected,
            "balance mismatch for {address}: expected {expected}, actual {actual}"
        );
    }

    pub fn assert_block_exists(&self, block_number: u64) {
        if let Err(err) = self.pipeline.provider.get_block_by_number(block_number) {
            panic!("expected block {block_number} to exist: {err}");
        }
    }

    pub fn mine_empty_blocks(&mut self, count: u64) -> Result<(), ExecutionError> {
        for _ in 0..count {
            self.execute_block(vec![])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{error::ExecutionError, providers::StateProvider};

    fn sender() -> Address {
        let tx = Transaction::Legacy {
            nonce: 0,
            gas_price: BASE_FEE,
            gas_limit: 21_000,
            to: Some(Address::new([0x22; 20])),
            value: 0,
            data: vec![],
        };
        let signed = sign(&tx, &TEST_PRIVATE_KEY, TEST_CHAIN_ID).unwrap();
        recover_sender(&signed, TEST_CHAIN_ID).unwrap()
    }

    fn legacy(nonce: u64, to: Address, value: u128, gas_limit: u64) -> Transaction {
        Transaction::Legacy {
            nonce,
            gas_price: BASE_FEE,
            gas_limit,
            to: Some(to),
            value,
            data: vec![],
        }
    }

    #[test]
    fn valid_transfer_updates_balances() {
        let sender = sender();
        let recipient = Address::new([0x22; 20]);
        let mut harness = TestHarness::funded(sender, 1_000_000_000_000_000).unwrap();
        let pre_transfer_root = compute_state_root(&harness.pipeline.provider).unwrap();

        let output = harness
            .execute_block(vec![legacy(0, recipient, 1_000, 21_000)])
            .unwrap();

        harness.assert_account_balance(recipient, 1_000);
        harness.assert_account_balance(sender, 1_000_000_000_000_000 - 21_000 * BASE_FEE - 1_000);
        assert_ne!(output.state_root, pre_transfer_root);
    }

    #[test]
    fn funded_harness_records_genesis_state_root() {
        let sender = sender();
        let harness = TestHarness::funded(sender, 1_000_000_000_000_000).unwrap();

        let genesis = harness.pipeline.provider.get_block_by_number(0).unwrap();
        let expected_root = compute_state_root(&harness.pipeline.provider).unwrap();

        assert_eq!(genesis.header.state_root, expected_root);
        assert_ne!(genesis.header.state_root, B256::zero());
    }

    #[test]
    fn executed_block_records_post_transfer_state_root() {
        let sender = sender();
        let recipient = Address::new([0x22; 20]);
        let mut harness = TestHarness::funded(sender, 1_000_000_000_000_000).unwrap();
        let genesis_root = harness
            .pipeline
            .provider
            .get_block_by_number(0)
            .unwrap()
            .header
            .state_root;

        let output = harness
            .execute_block(vec![legacy(0, recipient, 1_000, 21_000)])
            .unwrap();
        let block = harness.pipeline.provider.get_block_by_number(1).unwrap();

        assert_eq!(block.header.state_root, output.state_root);
        assert_ne!(block.header.state_root, genesis_root);
        assert_ne!(output.state_root, genesis_root);
    }

    #[test]
    fn insufficient_balance_returns_error() {
        let sender = sender();
        let recipient = Address::new([0x22; 20]);
        let mut harness = TestHarness::funded(sender, 1_000).unwrap();

        let err = harness
            .execute_block(vec![legacy(0, recipient, 1_000, 21_000)])
            .unwrap_err();

        assert!(matches!(err, ExecutionError::InsufficientBalance { .. }));
    }

    #[test]
    fn gas_limit_exceeded_returns_error() {
        let sender = sender();
        let recipient = Address::new([0x22; 20]);
        let mut harness = TestHarness::funded(sender, u128::MAX / 2).unwrap();

        let err = harness
            .execute_block(vec![legacy(0, recipient, 1_000, GAS_LIMIT_PER_BLOCK + 1)])
            .unwrap_err();

        assert!(matches!(err, ExecutionError::GasLimitExceeded { .. }));
    }

    #[test]
    fn invalid_parent_hash_returns_error() {
        let mut harness = TestHarness::new().unwrap();
        harness
            .pipeline
            .provider
            .blocks
            .get_mut(&0)
            .unwrap()
            .header
            .state_root = B256::new([0xee; 32]);
        let block = Block {
            header: Header {
                parent_hash: harness.genesis_hash,
                beneficiary: Address::zero(),
                state_root: B256::zero(),
                transactions_root: B256::zero(),
                gas_limit: GAS_LIMIT_PER_BLOCK,
                gas_used: 0,
                timestamp: 12,
                number: 1,
            },
            transactions: vec![],
        };
        let block_with_senders = BlockWithSenders {
            block,
            senders: vec![],
        };

        let err = harness.pipeline.execute(&block_with_senders).unwrap_err();

        assert!(matches!(err, ExecutionError::InvalidParentHash { .. }));
    }

    #[test]
    fn chain_of_50_blocks_advances_head_and_executes_transfer() {
        let sender = sender();
        let recipient = Address::new([0x22; 20]);
        let mut harness = TestHarness::funded(sender, 1_000_000_000_000_000).unwrap();

        harness.mine_empty_blocks(49).unwrap();
        let pre_transfer_root = compute_state_root(&harness.pipeline.provider).unwrap();
        let output = harness
            .execute_block(vec![legacy(0, recipient, 1_000, 21_000)])
            .unwrap();

        assert_eq!(harness.current_block_number, 50);
        harness.assert_block_exists(50);
        harness.assert_account_balance(recipient, 1_000);
        assert_ne!(output.state_root, pre_transfer_root);
    }

    #[test]
    fn final_state_after_multiple_blocks_is_exact() {
        let sender = sender();
        let recipient_a = Address::new([0x22; 20]);
        let recipient_b = Address::new([0x33; 20]);
        let initial = 1_000_000_000_000_000u128;
        let mut harness = TestHarness::funded(sender, initial).unwrap();

        for block_idx in 0..5u64 {
            let pre_transfer_root = compute_state_root(&harness.pipeline.provider).unwrap();
            let output = harness
                .execute_block(vec![
                    legacy(block_idx * 2, recipient_a, 100, 21_000),
                    legacy(block_idx * 2 + 1, recipient_b, 200, 21_000),
                ])
                .unwrap();
            assert_ne!(output.state_root, pre_transfer_root);
        }

        let gas = 10 * 21_000u128 * BASE_FEE;
        harness.assert_account_balance(sender, initial - gas - 1_500);
        harness.assert_account_balance(recipient_a, 500);
        harness.assert_account_balance(recipient_b, 1_000);

        let sender_account = harness.pipeline.provider.get_account(sender).unwrap();
        assert_eq!(sender_account.nonce, 10);
    }
}
