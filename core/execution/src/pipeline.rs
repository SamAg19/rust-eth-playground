use crate::{
    error::ExecutionError,
    executor::{BlockExecutor, BlockWithSenders},
    in_memory::InMemoryProvider,
    providers::HeaderProvider,
    validator::ConsensusValidator,
};

pub struct Pipeline<E: BlockExecutor, V: ConsensusValidator> {
    pub provider: InMemoryProvider,
    pub executor: E,
    pub validator: V,
}

impl<E: BlockExecutor, V: ConsensusValidator> Pipeline<E, V> {
    pub fn new(provider: InMemoryProvider, executor: E, validator: V) -> Self {
        Self {
            provider,
            executor,
            validator,
        }
    }

    pub fn execute(
        &mut self,
        block_with_senders: &BlockWithSenders,
    ) -> Result<E::Output, ExecutionError> {
        let parent_header = self
            .provider
            .get_header_by_hash(block_with_senders.block.header.parent_hash)?;
        self.validator
            .validate_header(&block_with_senders.block.header, &parent_header)?;
        self.validator.validate_body(&block_with_senders.block)?;
        self.executor
            .execute(block_with_senders, &mut self.provider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::ValueTransferExecutor;
    use crate::primitives::{AccountInfo, Block, Header};
    use crate::providers::StateProvider;
    use crate::test_helpers::{block_with_senders, signed_legacy_tx, test_sender};
    use crate::validator::{BasicValidator, StrictValidator};
    use types::{Address, B256, Bloom};

    const BASE_FEE: u128 = 1_000_000_000;
    const GAS_LIMIT_PER_BLOCK: u64 = 30_000_000;
    const INITIAL_SENDER_BALANCE: u128 = 1_000_000_000_000_000_000; // 1 ETH

    fn recipient_addr() -> Address {
        Address::new([0x22; 20])
    }

    fn second_recipient_addr() -> Address {
        Address::new([0x33; 20])
    }

    fn parent_header() -> Header {
        Header {
            block_number: 0,
            parent_hash: B256::new([0x00; 32]),
            state_root: B256::new([0x01; 32]),
            transactions_root: B256::new([0x02; 32]),
            receipts_root: B256::new([0x03; 32]),
            logs_bloom: Bloom::zero(),
            gas_limit: GAS_LIMIT_PER_BLOCK,
            gas_used: 0,
            base_fee_per_gas: BASE_FEE,
            hash: B256::new([0xaa; 32]),
        }
    }

    fn child_header(number: u64, parent_hash: B256, gas_used: u64) -> Header {
        Header {
            block_number: number,
            parent_hash,
            state_root: B256::new([0x10 + number as u8; 32]),
            transactions_root: B256::new([0x20 + number as u8; 32]),
            receipts_root: B256::new([0x30 + number as u8; 32]),
            logs_bloom: Bloom::zero(),
            gas_limit: GAS_LIMIT_PER_BLOCK,
            gas_used,
            base_fee_per_gas: BASE_FEE,
            hash: B256::new([0xa0 + number as u8; 32]),
        }
    }

    fn make_signed_tx(
        nonce: u64,
        value: u128,
        gas_limit: u64,
    ) -> rlp_codec::signing::SignedTransaction {
        signed_legacy_tx(nonce, value, gas_limit, BASE_FEE, Some(recipient_addr()))
    }

    fn fund(provider: &mut InMemoryProvider, address: Address, balance: u128, nonce: u64) {
        provider.set_account(
            address,
            AccountInfo {
                balance,
                nonce,
                code_hash: B256::default(),
                code: None,
            },
        );
    }

    fn setup_basic() -> Pipeline<ValueTransferExecutor, BasicValidator> {
        let mut provider = InMemoryProvider::default();
        provider
            .insert_block(Block {
                header: parent_header(),
                transactions: vec![],
            })
            .unwrap();
        fund(&mut provider, test_sender(), INITIAL_SENDER_BALANCE, 0);
        Pipeline::new(
            provider,
            ValueTransferExecutor,
            BasicValidator { max_txs: 10 },
        )
    }

    #[test]
    fn valid_block_executes_and_produces_receipt() {
        let mut pipeline = setup_basic();
        let signed = make_signed_tx(0, 1_000_000, 21_000);
        let expected_hash = signed.hash().unwrap();
        let bws = block_with_senders(child_header(1, parent_header().hash, 21_000), vec![signed]);

        let output = pipeline.execute(&bws).unwrap();

        assert_eq!(output.receipts.len(), 1);
        let r = &output.receipts[0];
        assert_eq!(r.transaction_hash, expected_hash);
        assert_eq!(r.transaction_index, 0);
        assert_eq!(r.block_number, 1);
        assert_eq!(r.from, test_sender());
        assert_eq!(r.to, Some(recipient_addr()));
        assert!(r.status);
        assert_eq!(r.gas_used, 21_000);
        assert_eq!(r.cumulative_gas_used, 21_000);
        assert_eq!(output.gas_used, 21_000);

        assert_eq!(
            pipeline
                .provider
                .get_account(recipient_addr())
                .unwrap()
                .balance,
            1_000_000
        );
        let sender = pipeline.provider.get_account(test_sender()).unwrap();
        assert_eq!(sender.nonce, 1);
        let expected_cost = 21_000u128 * BASE_FEE + 1_000_000;
        assert_eq!(sender.balance, INITIAL_SENDER_BALANCE - expected_cost);
    }

    #[test]
    fn gas_used_exceeds_limit_fails_validation() {
        let mut pipeline = setup_basic();
        let mut header = child_header(1, parent_header().hash, 0);
        header.gas_used = GAS_LIMIT_PER_BLOCK + 1;
        let bws = block_with_senders(header, vec![]);

        let err = pipeline.execute(&bws).unwrap_err();
        assert!(matches!(err, ExecutionError::GasLimitExceeded { .. }));
    }

    #[test]
    fn wrong_parent_hash_fails_validation() {
        // Seed an inconsistent provider state: `blocks_by_hash` points a hash at a
        // block number, but the block stored under that number carries a different
        // `header.hash`. This lets the pipeline's lookup-by-hash succeed but the
        // returned parent's self-hash mismatches the requested hash, tripping
        // `InvalidParentHash` in the validator.
        let mut provider = InMemoryProvider::default();
        let mut inconsistent_parent = parent_header();
        inconsistent_parent.hash = B256::new([0xbb; 32]); // stored hash differs from index key
        provider.blocks.insert(
            0,
            Block {
                header: inconsistent_parent,
                transactions: vec![],
            },
        );
        provider.blocks_by_hash.insert(B256::new([0xaa; 32]), 0); // index under parent_header().hash
        fund(&mut provider, test_sender(), INITIAL_SENDER_BALANCE, 0);

        let mut pipeline = Pipeline::new(
            provider,
            ValueTransferExecutor,
            BasicValidator { max_txs: 10 },
        );

        let bws = block_with_senders(child_header(1, B256::new([0xaa; 32]), 0), vec![]);
        let err = pipeline.execute(&bws).unwrap_err();
        assert!(matches!(err, ExecutionError::InvalidParentHash { .. }));
    }

    #[test]
    fn insufficient_balance_fails_execution() {
        let mut pipeline = setup_basic();
        let huge_value = INITIAL_SENDER_BALANCE; // leaves no room for gas
        let signed = make_signed_tx(0, huge_value, 21_000);
        let bws = block_with_senders(child_header(1, parent_header().hash, 21_000), vec![signed]);

        let err = pipeline.execute(&bws).unwrap_err();
        assert!(matches!(err, ExecutionError::InsufficientBalance { .. }));
    }

    #[test]
    fn wrong_nonce_fails_execution() {
        let mut pipeline = setup_basic();
        let signed = make_signed_tx(5, 1_000_000, 21_000); // account nonce is 0
        let bws = block_with_senders(child_header(1, parent_header().hash, 21_000), vec![signed]);

        let err = pipeline.execute(&bws).unwrap_err();
        assert!(matches!(err, ExecutionError::InvalidNonce { .. }));
    }

    #[test]
    fn chain_of_three_blocks_reflects_all_transfers() {
        let mut pipeline = setup_basic();
        let mut previous_hash = parent_header().hash;
        let mut expected_sender_balance = INITIAL_SENDER_BALANCE;
        let mut expected_recipient_balance = 0u128;

        for n in 1u64..=3 {
            let value = 1_000_000u128 * n as u128;
            let signed = make_signed_tx(n - 1, value, 21_000);
            let header = child_header(n, previous_hash, 21_000);
            let bws = block_with_senders(header.clone(), vec![signed]);

            pipeline.execute(&bws).unwrap();

            // Insert each executed block so the next iteration can look it up as parent.
            pipeline.provider.insert_block(bws.block).unwrap();

            expected_sender_balance -= 21_000u128 * BASE_FEE + value;
            expected_recipient_balance += value;
            previous_hash = header.hash;
        }

        let sender = pipeline.provider.get_account(test_sender()).unwrap();
        assert_eq!(sender.nonce, 3);
        assert_eq!(sender.balance, expected_sender_balance);
        assert_eq!(
            pipeline
                .provider
                .get_account(recipient_addr())
                .unwrap()
                .balance,
            expected_recipient_balance,
        );
    }

    #[test]
    fn repeated_execution_from_same_state_produces_same_state_root() {
        fn execute_once() -> B256 {
            let mut pipeline = setup_basic();
            fund(&mut pipeline.provider, recipient_addr(), 0, 0);
            fund(&mut pipeline.provider, second_recipient_addr(), 0, 0);

            let tx1 = make_signed_tx(0, 1_000_000, 21_000);
            let tx2 = signed_legacy_tx(
                1,
                2_000_000,
                21_000,
                BASE_FEE,
                Some(second_recipient_addr()),
            );
            let bws = block_with_senders(
                child_header(1, parent_header().hash, 42_000),
                vec![tx1, tx2],
            );

            pipeline.execute(&bws).unwrap().state_root
        }

        let first = execute_once();
        let second = execute_once();

        assert_eq!(first, second);
        assert_ne!(first, B256::default());
    }

    // §9.5 — swapping the validator without touching Pipeline code.
    #[test]
    fn strict_validator_rejects_non_contiguous_block_number() {
        let mut provider = InMemoryProvider::default();
        provider
            .insert_block(Block {
                header: parent_header(),
                transactions: vec![],
            })
            .unwrap();
        fund(&mut provider, test_sender(), INITIAL_SENDER_BALANCE, 0);
        let mut strict = Pipeline::new(
            provider,
            ValueTransferExecutor,
            StrictValidator { max_txs: 10 },
        );

        // Parent is block 0; child is block 5 — not contiguous.
        let bws = block_with_senders(child_header(5, parent_header().hash, 0), vec![]);
        let err = strict.execute(&bws).unwrap_err();
        assert!(matches!(err, ExecutionError::InvalidBlockNumber { .. }));
    }

    // Same block passes with `BasicValidator`, demonstrating the rule is only in `StrictValidator`.
    #[test]
    fn basic_validator_accepts_non_contiguous_block_number() {
        let mut pipeline = setup_basic();
        let bws = block_with_senders(child_header(5, parent_header().hash, 0), vec![]);
        assert!(pipeline.execute(&bws).is_ok());
    }
}
