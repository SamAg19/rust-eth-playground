use std::{collections::{BTreeMap, HashMap}, sync::{Arc, atomic::{AtomicU64, Ordering}}};
use tokio::{
    sync::{RwLock, mpsc},
};
use bytes::BytesMut;
use execution::{InMemoryProvider, executor::{BlockWithSenders, ValueTransferExecutor}, pipeline::Pipeline, providers::BlockProvider, validator::StrictValidator};
use networking::PeerId;
use rlp_codec::{RlpEncodable, encode, hash_header, signing::recover_sender};
use tracing::{debug, info, trace, warn, Instrument};
use types::{Account, Address, B256, Block, ChainHead};

use crate::errors::ProcessorError;

#[derive(Debug)]
pub enum ProcessorMessage {
    NewBlock{
        block: Block,
        peer_id: PeerId
    },
    Shutdown
}

#[derive(Debug)]
struct PendingBlock {
    block: Block,
    peer_id: PeerId,
}

pub struct Metrics {
    pub blocks_received: AtomicU64,
    pub blocks_committed: AtomicU64,
    pub blocks_rejected_validation: AtomicU64,
    pub blocks_rejected_execution: AtomicU64,
    pub transactions_committed: AtomicU64,
    pub total_gas_committed: AtomicU64
}

//blocks_received: 5,
//blocks_committed: 3,
//blocks_rejected_validation: 1,
//blocks_rejected_execution: 1,
//transactions_committed: 9,


pub struct BlockProcessor {
    pending_map: BTreeMap<u64, PendingBlock>,
    pub chain_id: u64,
    pub chain_head: ChainHead,
    pub pipeline: Pipeline<ValueTransferExecutor, StrictValidator>,
    pub accounts: HashMap<Address, Account>,
    pub shared_head: Arc<RwLock<ChainHead>>,
    pub metrics: Arc<Metrics>
}

impl BlockProcessor {
    pub async fn new(genesis_block: Block, initial_account: HashMap<Address, Account>, chain_id: u64, shared_head: Arc<RwLock<ChainHead>>) -> Result<Self, ProcessorError> {
        let chain_head = shared_head.read().await.clone();
        let metrics = Arc::new(
            Metrics{
                blocks_received: AtomicU64::new(0),
                blocks_committed: AtomicU64::new(0),
                blocks_rejected_validation: AtomicU64::new(0),
                blocks_rejected_execution: AtomicU64::new(0),
                transactions_committed: AtomicU64::new(0),
                total_gas_committed: AtomicU64::new(0)
            }
        );

        let mut initial_block_processor = Self {
            pending_map: BTreeMap::new(),
            chain_id,
            chain_head,
            pipeline: Pipeline::new(InMemoryProvider::default(), ValueTransferExecutor, StrictValidator { max_txs: 100 }),
            accounts: initial_account,
            shared_head,
            metrics
        };
        initial_block_processor.pipeline.provider.insert_block(genesis_block)?;
        for (address, account) in &initial_block_processor.accounts {
            initial_block_processor.pipeline.provider.set_account(*address,account.clone());
            let mut buffer = BytesMut::new();
            encode(&account.to_rlp_item(), &mut buffer)?;
        }

        Ok(initial_block_processor)
    }
    pub async fn run(mut self, mut rx: mpsc::Receiver<ProcessorMessage>) {
        loop {
            match rx.recv().await {
                Some(ProcessorMessage::Shutdown) => {
                    debug!("processor shutdown message received");
                    match self.try_drain().await {
                        Ok(_) => debug!("All the remaining blocks in queue processed successfully"),
                        Err(e) => {
                            info!("Block processing failed due to {}", e);
                        }
                    }
                    break;
                }
                Some(ProcessorMessage::NewBlock { block, peer_id }) => {
                    info!("Block received {:?} from peerId {:?} ", block, peer_id);
                    match self.handle_new_block(block, peer_id).await {
                        Ok(_) => info!("Block processed successfully"),
                        Err(e) => {
                            info!("Block processing failed due to {}", e);
                        }
                    }
                }
                None => {
                    debug!("processor channel closed");
                    match self.try_drain().await {
                        Ok(_) => debug!("All the remaining blocks in queue processed successfully"),
                        Err(e) => {
                            info!("Block processing failed due to {}", e);
                        }
                    }
                    break;
                }
            }
        }

        while let Some((number, pending_block)) = self.pending_map.pop_first() {
            debug!(
                number,
                ?pending_block,
                "dropping non-consecutive pending block during shutdown cleanup"
            );
        }
    }

    async fn handle_new_block(&mut self, block: Block, peer_id: PeerId) -> Result<(), ProcessorError>  {
        self.pending_map.insert(block.header.number, PendingBlock {
            block,
            peer_id,
        });
        self.metrics.blocks_received.fetch_add(1, Ordering::Relaxed);

        trace!(?self.pending_map, "pending map after block insert");
        self.try_drain().await?;
        Ok(())
    }

    async fn try_drain(&mut self) -> Result<(), ProcessorError> {
        let mut next_block_number = self.chain_head.number + 1;
        while let Some(pending_block) = self.pending_map.remove(&next_block_number) {
            let block = pending_block.block;
            let peer_id = pending_block.peer_id;
            let block_number = block.header.number;
            trace!(
                block_number = block_number,
                ?self.pending_map,
                "pending map after block drain"
            );
            let span = tracing::info_span!(
                "process_block",
                block_number,
                peer_id = %peer_id,
            );
            if let Err(e) = self.process_block(block).instrument(span).await {
                warn!(
                    block_number,
                    peer_id = %peer_id,
                    error = %e,
                    "block rejected"
                );
            }

            next_block_number += 1;
        }
        Ok(())
    }
    async fn process_block(&mut self, block: Block) -> Result<(), ProcessorError> {
        let number = block.header.number;

        info!("Block number {} received.", number);
        let parent_block = self
            .pipeline
            .provider
            .get_block_by_hash(block.header.parent_hash)
            .map_err(|_| {
                self.metrics.blocks_rejected_validation.fetch_add(1, Ordering::Relaxed);
                ProcessorError::ParentNotFound { block_number: number }
            })?;

        if parent_block.header.number + 1 != number {
            self.metrics.blocks_rejected_validation.fetch_add(1, Ordering::Relaxed);
            return Err(ProcessorError::InvalidBlockNumber { actual: number, expected: parent_block.header.number + 1 });
        }

        debug!(
            block_number = number,
            parent_hash = %block.header.parent_hash,
            "parent validation succeeded"
        );

        if parent_block.header.timestamp >= block.header.timestamp {
            self.metrics.blocks_rejected_validation.fetch_add(1, Ordering::Relaxed);
            
            let parent_timestamp = parent_block.header.timestamp;
            let block_timestamp = block.header.timestamp;

            let reason = format!(
                "Parent block timestamp {parent_timestamp} must be less than incoming block timestamp {block_timestamp}"
            );
            return Err(ProcessorError::ValidationFailed { reason });
        }

        if self.chain_head.number + 1 != number {
            self.metrics.blocks_rejected_validation.fetch_add(1, Ordering::Relaxed);
            return Err(ProcessorError::InvalidBlockNumber { actual: number, expected: self.chain_head.number + 1 });
        }

        info!("Processing Block Number {}", number);

        let block_with_senders = self.build_block_with_senders(&block)?;

        let output = self.pipeline.execute(&block_with_senders).map_err(|e| {
            self.metrics.blocks_rejected_execution.fetch_add(1, Ordering::Relaxed);
            e
        })?;

        let hash = hash_header(&block.header)?;

        let gas_used = block.header.gas_used as u128;
        let gas_limit = block.header.gas_limit;

        self.pipeline.provider.insert_block(block)?;

        self.chain_head.number = number;
        self.chain_head.hash = hash;
        self.chain_head.total_difficulty += gas_used;

        self.write_to_shared_head(
            number, 
            hash, 
            self.chain_head.total_difficulty
        ).await;
        let tx_count = output.receipts.len() as u64;

        info!(
            block_number = self.chain_head.number,
            block_hash = format!("{}", self.chain_head.hash),
            tx_count = tx_count,
            gas_used = output.gas_used,
            gas_limit,
            state_root = format!("{}", output.state_root),
            "block committed"
        );
        
        self.metrics.blocks_committed.fetch_add(1, Ordering::Relaxed);
        self.metrics.transactions_committed.fetch_add(tx_count, Ordering::Relaxed);
        self.metrics.total_gas_committed.fetch_add(output.gas_used, Ordering::Relaxed);

        Ok(())
    }

    async fn write_to_shared_head(&mut self, number: u64, hash: B256, total_difficulty: u128) {
        let mut state = self.shared_head.write().await;
        state.number = number;
        state.hash = hash;
        state.total_difficulty = total_difficulty;
    }

    fn build_block_with_senders(&self, block: &Block) -> Result<BlockWithSenders, ProcessorError> {
        let mut senders = vec![];
        for tx in block.transactions() {
            senders.push(recover_sender(tx, self.chain_id)?);
        }
        Ok(BlockWithSenders { block: block.clone(), senders })
    }

}



#[cfg(test)]
mod tests {
    use super::*;
    use execution::{
        error::ExecutionError, executor::compute_state_root, providers::StateProvider,
        InMemoryProvider,
    };
    use rlp_codec::{
        hash_header,
        signing::{recover_sender, sign},
    };
    use std::{
        io::{self, Write},
        sync::Mutex,
    };
    use tokio::time::{timeout, Duration};
    use tracing::{Instrument, Level};
    use tracing_subscriber::fmt::MakeWriter;
    use types::{Header, Transaction};

    const TEST_PRIVATE_KEY: [u8; 32] = [
        0x4c, 0x0c, 0x4d, 0x14, 0x6c, 0x46, 0xed, 0x91, 0xf6, 0xa9, 0x35, 0x09, 0x46, 0xa3,
        0x69, 0x9e, 0xb1, 0xfb, 0xc1, 0x9c, 0x91, 0xc8, 0x10, 0xe6, 0xb6, 0xa7, 0x8b, 0x0c,
        0xa6, 0x06, 0x65, 0x6f,
    ];

    #[derive(Clone)]
    struct SharedLogWriter {
        output: Arc<Mutex<Vec<u8>>>,
    }

    struct SharedLogGuard {
        output: Arc<Mutex<Vec<u8>>>,
    }

    impl<'a> MakeWriter<'a> for SharedLogWriter {
        type Writer = SharedLogGuard;

        fn make_writer(&'a self) -> Self::Writer {
            SharedLogGuard {
                output: Arc::clone(&self.output),
            }
        }
    }

    impl Write for SharedLogGuard {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.output.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn test_block(number: u64, parent_hash: B256, state_root: B256) -> Block {
        Block {
            header: Header {
                parent_hash,
                beneficiary: Address::zero(),
                state_root,
                transactions_root: B256::zero(),
                gas_limit: 30_000_000,
                gas_used: 0,
                timestamp: number,
                number,
            },
            transactions: vec![],
        }
    }

    fn legacy_tx(nonce: u64, to: Address, value: u128) -> Transaction {
        Transaction::Legacy {
            nonce,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: Some(to),
            value,
            data: vec![],
        }
    }

    fn block_with_three_signed_transfers(
        number: u64,
        parent_hash: B256,
        state_root: B256,
        starting_nonce: u64,
        chain_id: u64,
    ) -> Block {
        let mut block = test_block(number, parent_hash, state_root);
        block.header.gas_used = 63_000;
        block.transactions = (0..3)
            .map(|offset| {
                let recipient = Address::from([0x20 + number as u8 + offset as u8; 20]);
                let tx = legacy_tx(starting_nonce + offset, recipient, 100);
                sign(&tx, &TEST_PRIVATE_KEY, chain_id).unwrap()
            })
            .collect();
        block
    }

    async fn test_processor_with_genesis_at_timestamp_and_chain_id(
        timestamp: u64,
        chain_id: u64,
    ) -> (BlockProcessor, B256, B256) {
        let mut initial_accounts = HashMap::new();
        initial_accounts.insert(
            Address::from([0x01; 20]),
            Account {
                balance: 1_000,
                nonce: 0,
                code_hash: B256::zero(),
            },
        );

        let mut genesis_provider = InMemoryProvider::default();
        for (address, account) in &initial_accounts {
            genesis_provider.set_account(*address, account.clone());
        }
        let genesis_state_root = compute_state_root(&genesis_provider).unwrap();
        let mut genesis_block = test_block(0, B256::zero(), genesis_state_root);
        genesis_block.header.timestamp = timestamp;
        let genesis_hash = hash_header(&genesis_block.header).unwrap();

        let shared_head = Arc::new(RwLock::new(ChainHead {
            number: 0,
            hash: genesis_hash,
            total_difficulty: 0,
        }));

        let processor = BlockProcessor::new(genesis_block, initial_accounts, chain_id, shared_head)
            .await
            .unwrap();

        (processor, genesis_hash, genesis_state_root)
    }

    async fn test_processor_with_genesis_at_timestamp(timestamp: u64) -> (BlockProcessor, B256, B256) {
        test_processor_with_genesis_at_timestamp_and_chain_id(timestamp, 1).await
    }

    async fn test_processor_with_genesis() -> (BlockProcessor, B256, B256) {
        test_processor_with_genesis_at_timestamp(0).await
    }

    async fn test_processor_with_accounts(
        initial_accounts: HashMap<Address, Account>,
        chain_id: u64,
    ) -> (BlockProcessor, B256, B256) {
        let mut genesis_provider = InMemoryProvider::default();
        for (address, account) in &initial_accounts {
            genesis_provider.set_account(*address, account.clone());
        }
        let genesis_state_root = compute_state_root(&genesis_provider).unwrap();
        let genesis_block = test_block(0, B256::zero(), genesis_state_root);
        let genesis_hash = hash_header(&genesis_block.header).unwrap();

        let shared_head = Arc::new(RwLock::new(ChainHead {
            number: 0,
            hash: genesis_hash,
            total_difficulty: 0,
        }));

        let processor = BlockProcessor::new(genesis_block, initial_accounts, chain_id, shared_head)
            .await
            .unwrap();

        (processor, genesis_hash, genesis_state_root)
    }

    fn assert_validation_failed_contains(error: ProcessorError, expected_parts: &[&str]) {
        match error {
            ProcessorError::ValidationFailed { reason } => {
                for expected_part in expected_parts {
                    assert!(
                        reason.contains(expected_part),
                        "expected validation error `{reason}` to contain `{expected_part}`"
                    );
                }
            }
            other => panic!("expected ValidationFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn constructor_stores_chain_id_for_sender_recovery() {
        let chain_id = 1337;
        let (processor, _, _) = test_processor_with_genesis_at_timestamp_and_chain_id(0, chain_id).await;

        assert_eq!(processor.chain_id, chain_id);
    }

    #[tokio::test]
    async fn build_block_with_senders_recovers_two_senders_in_transaction_order() {
        let chain_id = 1;
        let (processor, genesis_hash, genesis_state_root) =
            test_processor_with_genesis_at_timestamp_and_chain_id(0, chain_id).await;
        let tx_1 = legacy_tx(0, Address::from([0x02; 20]), 100);
        let tx_2 = legacy_tx(1, Address::from([0x03; 20]), 200);
        let signed_tx_1 = sign(&tx_1, &TEST_PRIVATE_KEY, chain_id).unwrap();
        let signed_tx_2 = sign(&tx_2, &TEST_PRIVATE_KEY, chain_id).unwrap();
        let expected_sender_1 = recover_sender(&signed_tx_1, chain_id).unwrap();
        let expected_sender_2 = recover_sender(&signed_tx_2, chain_id).unwrap();
        let mut block = test_block(1, genesis_hash, genesis_state_root);
        block.transactions = vec![signed_tx_1, signed_tx_2];

        let block_with_senders = processor.build_block_with_senders(&block).unwrap();

        assert_eq!(block_with_senders.block, block);
        assert_eq!(block_with_senders.senders, vec![expected_sender_1, expected_sender_2]);
    }

    #[tokio::test]
    async fn build_block_with_senders_wrong_chain_id_does_not_recover_expected_sender() {
        let signing_chain_id = 1;
        let recovery_chain_id = 137;
        let (processor, genesis_hash, genesis_state_root) =
            test_processor_with_genesis_at_timestamp_and_chain_id(0, recovery_chain_id).await;
        let tx = legacy_tx(0, Address::from([0x02; 20]), 100);
        let signed_tx = sign(&tx, &TEST_PRIVATE_KEY, signing_chain_id).unwrap();
        let expected_sender = recover_sender(&signed_tx, signing_chain_id).unwrap();
        let mut block = test_block(1, genesis_hash, genesis_state_root);
        block.transactions = vec![signed_tx];

        match processor.build_block_with_senders(&block) {
            Ok(block_with_senders) => {
                assert_ne!(block_with_senders.senders, vec![expected_sender]);
            }
            Err(ProcessorError::Signing(_)) => {}
            Err(other) => panic!("expected signing error or non-matching sender, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn process_block_surfaces_execution_errors_from_pipeline() {
        let chain_id = 1;
        let (mut processor, genesis_hash, genesis_state_root) =
            test_processor_with_genesis_at_timestamp_and_chain_id(0, chain_id).await;
        let tx = legacy_tx(5, Address::from([0x02; 20]), 100);
        let signed_tx = sign(&tx, &TEST_PRIVATE_KEY, chain_id).unwrap();
        let mut block = test_block(1, genesis_hash, genesis_state_root);
        block.header.gas_used = 21_000;
        block.transactions = vec![signed_tx];

        let error = processor.process_block(block).await.unwrap_err();

        match error {
            ProcessorError::Execution(_) => {}
            other => panic!("expected ProcessorError::Execution(_), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn process_block_does_not_commit_block_on_execution_error() {
        let chain_id = 1;
        let (mut processor, genesis_hash, genesis_state_root) =
            test_processor_with_genesis_at_timestamp_and_chain_id(0, chain_id).await;
        let tx = legacy_tx(0, Address::from([0x02; 20]), 100);
        let signed_tx = sign(&tx, &TEST_PRIVATE_KEY, chain_id).unwrap();
        let mut block = test_block(1, genesis_hash, genesis_state_root);
        block.header.gas_used = 21_000;
        block.transactions = vec![signed_tx];

        let error = processor.process_block(block).await.unwrap_err();

        match error {
            ProcessorError::Execution(_) => {}
            other => panic!("expected ProcessorError::Execution(_), got {other:?}"),
        }
        assert_eq!(processor.chain_head.number, 0);
        assert!(matches!(
            processor.pipeline.provider.get_block_by_number(1),
            Err(ExecutionError::BlockNotFound { .. })
        ));
    }

    #[tokio::test]
    async fn process_block_imports_signed_value_transfer_and_updates_state() {
        let chain_id = 1;
        let recipient = Address::from([0x02; 20]);
        let value = 1_000;
        let gas_limit = 21_000;
        let gas_price = 1_000_000_000;
        let sender_start_balance = 100_000_000_000_000_000;
        let tx = legacy_tx(0, recipient, value);
        let signed_tx = sign(&tx, &TEST_PRIVATE_KEY, chain_id).unwrap();
        let sender = recover_sender(&signed_tx, chain_id).unwrap();
        let mut initial_accounts = HashMap::new();
        initial_accounts.insert(
            sender,
            Account {
                balance: sender_start_balance,
                nonce: 0,
                code_hash: B256::zero(),
            },
        );
        let (mut processor, genesis_hash, genesis_state_root) =
            test_processor_with_accounts(initial_accounts, chain_id).await;
        let mut block = test_block(1, genesis_hash, genesis_state_root);
        block.header.gas_used = gas_limit;
        block.transactions = vec![signed_tx];
        let expected_block_hash = hash_header(&block.header).unwrap();

        processor.process_block(block.clone()).await.unwrap();

        assert_eq!(
            processor.pipeline.provider.get_block_by_number(1).unwrap(),
            block
        );
        assert_eq!(
            processor
                .pipeline
                .provider
                .get_block_by_hash(expected_block_hash)
                .unwrap(),
            block
        );
        assert_eq!(processor.chain_head.number, 1);
        assert_eq!(processor.chain_head.hash, expected_block_hash);

        let shared_head = processor.shared_head.read().await.clone();
        assert_eq!(shared_head.number, 1);
        assert_eq!(shared_head.hash, expected_block_hash);

        assert_eq!(
            processor
                .metrics
                .blocks_committed
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            processor
                .metrics
                .transactions_committed
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            processor
                .metrics
                .total_gas_committed
                .load(std::sync::atomic::Ordering::Relaxed),
            gas_limit
        );

        let sender_account = processor.pipeline.provider.get_account(sender).unwrap();
        let recipient_account = processor.pipeline.provider.get_account(recipient).unwrap();
        assert_eq!(sender_account.nonce, 1);
        assert_eq!(
            sender_account.balance,
            sender_start_balance - (gas_limit as u128 * gas_price) - value
        );
        assert_eq!(recipient_account.balance, value);
    }

    #[tokio::test]
    async fn process_block_execution_error_does_not_commit_block_or_advance_head() {
        let chain_id = 1;
        let tx = legacy_tx(1, Address::from([0x02; 20]), 100);
        let signed_tx = sign(&tx, &TEST_PRIVATE_KEY, chain_id).unwrap();
        let sender = recover_sender(&signed_tx, chain_id).unwrap();
        let mut initial_accounts = HashMap::new();
        initial_accounts.insert(
            sender,
            Account {
                balance: 100_000_000_000_000_000,
                nonce: 0,
                code_hash: B256::zero(),
            },
        );
        let (mut processor, genesis_hash, genesis_state_root) =
            test_processor_with_accounts(initial_accounts, chain_id).await;
        let original_head = processor.chain_head.clone();
        let mut block = test_block(1, genesis_hash, genesis_state_root);
        block.header.gas_used = 21_000;
        block.transactions = vec![signed_tx];

        let error = processor.process_block(block).await.unwrap_err();

        assert!(matches!(
            error,
            ProcessorError::Execution(ExecutionError::InvalidNonce { .. })
        ));
        assert!(matches!(
            processor.pipeline.provider.get_block_by_number(1),
            Err(ExecutionError::BlockNotFound { .. })
        ));
        assert_eq!(processor.chain_head.number, original_head.number);
        assert_eq!(processor.chain_head.hash, original_head.hash);
        assert_eq!(
            processor.chain_head.total_difficulty,
            original_head.total_difficulty
        );
        assert_eq!(
            processor
                .metrics
                .blocks_rejected_execution
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
    }

    #[tokio::test]
    async fn parent_validation_returns_parent_not_found_for_unknown_parent_hash() {
        let (mut processor, genesis_hash, genesis_state_root) = test_processor_with_genesis().await;
        let original_head = processor.chain_head.clone();
        let mut bad_parent_hash_bytes = *genesis_hash.as_bytes();
        bad_parent_hash_bytes[0] ^= 0x01;
        let block = test_block(1, B256::from(bad_parent_hash_bytes), genesis_state_root);

        let error = processor.process_block(block).await.unwrap_err();

        match error {
            ProcessorError::ParentNotFound { block_number } => {
                assert_eq!(block_number, 1);
            }
            other => panic!("expected ParentNotFound, got {other:?}"),
        }
        assert_eq!(processor.chain_head.number, original_head.number);
        assert_eq!(processor.chain_head.hash, original_head.hash);
        assert!(matches!(
            processor.pipeline.provider.get_block_by_number(1),
            Err(ExecutionError::BlockNotFound { .. })
        ));
    }

    #[tokio::test]
    async fn process_block_accepts_sequential_blocks_with_matching_parent_hashes() {
        let (mut processor, genesis_hash, genesis_state_root) = test_processor_with_genesis().await;
        let block_1 = test_block(1, genesis_hash, genesis_state_root);
        let block_1_hash = hash_header(&block_1.header).unwrap();
        let block_2 = test_block(2, block_1_hash, genesis_state_root);
        let block_2_hash = hash_header(&block_2.header).unwrap();

        processor.process_block(block_1).await.unwrap();
        processor.process_block(block_2).await.unwrap();

        assert_eq!(processor.chain_head.number, 2);
        assert_eq!(processor.chain_head.hash, block_2_hash);
        let shared_head = processor.shared_head.read().await.clone();
        assert_eq!(shared_head.number, 2);
        assert_eq!(shared_head.hash, block_2_hash);
    }

    #[tokio::test]
    async fn parent_validation_rejects_wrong_block_number_relationship() {
        let (mut processor, genesis_hash, genesis_state_root) = test_processor_with_genesis().await;
        let block = test_block(2, genesis_hash, genesis_state_root);

        let error = processor.process_block(block).await.unwrap_err();

        match error {
            ProcessorError::InvalidBlockNumber { expected, actual } => {
                assert_eq!(expected, 1);
                assert_eq!(actual, 2);
            }
            other => panic!("expected InvalidBlockNumber, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn header_validation_allows_gas_used_equal_to_gas_limit() {
        let (mut processor, genesis_hash, genesis_state_root) = test_processor_with_genesis().await;
        let mut block = test_block(1, genesis_hash, genesis_state_root);
        block.header.gas_limit = 42;
        block.header.gas_used = 42;

        processor.process_block(block).await.unwrap();
    }

    #[tokio::test]
    async fn header_validation_rejects_gas_used_exceeding_gas_limit() {
        let (mut processor, genesis_hash, genesis_state_root) = test_processor_with_genesis().await;
        let mut block = test_block(1, genesis_hash, genesis_state_root);
        block.header.gas_limit = 42;
        block.header.gas_used = 43;

        let error = processor.process_block(block).await.unwrap_err();

        assert!(matches!(error, ProcessorError::Execution(ExecutionError::GasLimitExceeded { limit: 42, used: 43 })));
    }

    #[tokio::test]
    async fn header_validation_rejects_timestamp_equal_to_parent_timestamp() {
        let (mut processor, genesis_hash, genesis_state_root) = test_processor_with_genesis_at_timestamp(10).await;
        let mut block = test_block(1, genesis_hash, genesis_state_root);
        block.header.timestamp = 10;

        let error = processor.process_block(block).await.unwrap_err();

        assert_validation_failed_contains(error, &["timestamp", "10"]);
    }

    #[tokio::test]
    async fn header_validation_rejects_timestamp_less_than_parent_timestamp() {
        let (mut processor, genesis_hash, genesis_state_root) = test_processor_with_genesis_at_timestamp(10).await;
        let mut block = test_block(1, genesis_hash, genesis_state_root);
        block.header.timestamp = 9;

        let error = processor.process_block(block).await.unwrap_err();

        assert_validation_failed_contains(error, &["timestamp", "10", "9"]);
    }

    #[tokio::test]
    async fn run_receives_new_blocks_and_exits_on_shutdown() {
        let mut initial_accounts = HashMap::new();
        initial_accounts.insert(
            Address::from([0x01; 20]),
            Account {
                balance: 1_000,
                nonce: 0,
                code_hash: B256::zero(),
            },
        );

        let mut genesis_provider = InMemoryProvider::default();
        for (address, account) in &initial_accounts {
            genesis_provider.set_account(*address, account.clone());
        }
        let genesis_state_root = compute_state_root(&genesis_provider).unwrap();
        let genesis_block = test_block(0, B256::zero(), genesis_state_root);
        let genesis_hash = hash_header(&genesis_block.header).unwrap();

        let shared_head = Arc::new(RwLock::new(ChainHead {
            number: 0,
            hash: genesis_hash,
            total_difficulty: 0,
        }));

        let processor = BlockProcessor::new(genesis_block, initial_accounts, 1, shared_head)
            .await
            .unwrap();
        let (tx, rx) = mpsc::channel(1);
        let handle = tokio::spawn(processor.run(rx));

        let send_messages = async {
            let mut parent_hash = genesis_hash;
            for number in 1..=3 {
                let block = test_block(number, parent_hash, genesis_state_root);
                parent_hash = hash_header(&block.header).unwrap();

                tx.send(ProcessorMessage::NewBlock {
                    block,
                    peer_id: PeerId(number),
                })
                .await
                .unwrap();
            }
            tx.send(ProcessorMessage::Shutdown).await.unwrap();
        };

        timeout(Duration::from_secs(1), send_messages).await.unwrap();
        timeout(Duration::from_secs(1), handle).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn run_drains_out_of_order_blocks_in_chain_order() {
        let log_output = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(Level::INFO)
            .with_ansi(false)
            .without_time()
            .with_target(false)
            .with_writer(SharedLogWriter {
                output: Arc::clone(&log_output),
            })
            .finish();
        let _ = tracing::subscriber::set_global_default(subscriber);
        
        let mut initial_accounts = HashMap::new();
        initial_accounts.insert(
            Address::from([0x01; 20]),
            Account {
                balance: 1_000,
                nonce: 0,
                code_hash: B256::zero(),
            },
        );

        let mut genesis_provider = InMemoryProvider::default();
        for (address, account) in &initial_accounts {
            genesis_provider.set_account(*address, account.clone());
        }
        let genesis_state_root = compute_state_root(&genesis_provider).unwrap();
        let genesis_block = test_block(0, B256::zero(), genesis_state_root);
        let genesis_hash = hash_header(&genesis_block.header).unwrap();

        let block_1 = test_block(1, genesis_hash, genesis_state_root);
        let block_1_hash = hash_header(&block_1.header).unwrap();
        let block_2 = test_block(2, block_1_hash, genesis_state_root);
        let block_2_hash = hash_header(&block_2.header).unwrap();
        let block_3 = test_block(3, block_2_hash, genesis_state_root);

        let shared_head = Arc::new(RwLock::new(ChainHead {
            number: 0,
            hash: genesis_hash,
            total_difficulty: 0,
        }));

        let processor = BlockProcessor::new(genesis_block, initial_accounts, 1, shared_head)
            .await
            .unwrap();
        let (tx, rx) = mpsc::channel(1);
        let span = tracing::info_span!("out_of_order_drain_test");
        let handle = tokio::spawn(processor.run(rx).instrument(span));

        let send_messages = async {
            for (block, peer_id) in [(block_1, 1), (block_3, 3), (block_2, 2)] {
                tx.send(ProcessorMessage::NewBlock {
                    block,
                    peer_id: PeerId(peer_id),
                })
                .await
                .unwrap();
            }
            tx.send(ProcessorMessage::Shutdown).await.unwrap();
        };

        timeout(Duration::from_secs(1), send_messages).await.unwrap();
        timeout(Duration::from_secs(1), handle).await.unwrap().unwrap();

        let logs = String::from_utf8(log_output.lock().unwrap().clone()).unwrap();
        let test_logs = logs
            .lines()
            .filter(|line| line.contains("out_of_order_drain_test"))
            .collect::<Vec<_>>()
            .join("\n");
        let block_1_position = test_logs.find("Block number 1 received.").unwrap();
        let block_2_position = test_logs.find("Block number 2 received.").unwrap();
        let block_3_position = test_logs.find("Block number 3 received.").unwrap();

        assert!(block_1_position < block_2_position);
        assert!(block_2_position < block_3_position);
    }

    #[tokio::test]
    async fn run_allows_corrected_block_to_fill_empty_pending_slot_after_rejection() {
        let mut initial_accounts = HashMap::new();
        initial_accounts.insert(
            Address::from([0x01; 20]),
            Account {
                balance: 1_000,
                nonce: 0,
                code_hash: B256::zero(),
            },
        );

        let mut genesis_provider = InMemoryProvider::default();
        for (address, account) in &initial_accounts {
            genesis_provider.set_account(*address, account.clone());
        }
        let genesis_state_root = compute_state_root(&genesis_provider).unwrap();
        let genesis_block = test_block(0, B256::zero(), genesis_state_root);
        let genesis_hash = hash_header(&genesis_block.header).unwrap();
        let corrected_block = test_block(1, genesis_hash, genesis_state_root);
        let corrected_block_hash = hash_header(&corrected_block.header).unwrap();
        let mut bad_parent_hash_bytes = *genesis_hash.as_bytes();
        bad_parent_hash_bytes[0] ^= 0x01;
        let invalid_block = test_block(1, B256::from(bad_parent_hash_bytes), genesis_state_root);

        let shared_head = Arc::new(RwLock::new(ChainHead {
            number: 0,
            hash: genesis_hash,
            total_difficulty: 0,
        }));

        let processor = BlockProcessor::new(
            genesis_block,
            initial_accounts,
            1,
            Arc::clone(&shared_head),
        )
        .await
        .unwrap();
        let metrics = Arc::clone(&processor.metrics);
        let (tx, rx) = mpsc::channel(1);
        let handle = tokio::spawn(processor.run(rx));

        let send_messages = async {
            tx.send(ProcessorMessage::NewBlock {
                block: invalid_block,
                peer_id: PeerId(1),
            })
            .await
            .unwrap();
            tx.send(ProcessorMessage::NewBlock {
                block: corrected_block,
                peer_id: PeerId(1),
            })
            .await
            .unwrap();
            tx.send(ProcessorMessage::Shutdown).await.unwrap();
        };

        timeout(Duration::from_secs(1), send_messages).await.unwrap();
        timeout(Duration::from_secs(1), handle).await.unwrap().unwrap();

        let head = shared_head.read().await.clone();
        assert_eq!(head.number, 1);
        assert_eq!(head.hash, corrected_block_hash);
        assert_eq!(
            metrics
                .blocks_received
                .load(std::sync::atomic::Ordering::Relaxed),
            2
        );
        assert_eq!(
            metrics
                .blocks_rejected_validation
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            metrics
                .blocks_committed
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
    }

    #[tokio::test]
    async fn run_processes_five_blocks_with_mixed_commit_validation_and_execution_outcomes() {
        let chain_id = 1;
        let sender_probe_tx = legacy_tx(0, Address::from([0x02; 20]), 100);
        let sender_probe_signed_tx = sign(&sender_probe_tx, &TEST_PRIVATE_KEY, chain_id).unwrap();
        let sender = recover_sender(&sender_probe_signed_tx, chain_id).unwrap();
        let mut initial_accounts = HashMap::new();
        initial_accounts.insert(
            sender,
            Account {
                balance: 100_000_000_000_000_000,
                nonce: 0,
                code_hash: B256::zero(),
            },
        );

        let (processor, genesis_hash, genesis_state_root) =
            test_processor_with_accounts(initial_accounts, chain_id).await;
        let shared_head = Arc::clone(&processor.shared_head);
        let metrics = Arc::clone(&processor.metrics);

        let valid_block_1 =
            block_with_three_signed_transfers(1, genesis_hash, genesis_state_root, 0, chain_id);
        let valid_block_1_hash = hash_header(&valid_block_1.header).unwrap();
        let valid_block_2 =
            block_with_three_signed_transfers(2, valid_block_1_hash, genesis_state_root, 3, chain_id);
        let valid_block_2_hash = hash_header(&valid_block_2.header).unwrap();
        let valid_block_3 =
            block_with_three_signed_transfers(3, valid_block_2_hash, genesis_state_root, 6, chain_id);
        let valid_block_3_hash = hash_header(&valid_block_3.header).unwrap();

        let mut bad_parent_hash_bytes = *genesis_hash.as_bytes();
        bad_parent_hash_bytes[0] ^= 0x01;
        let invalid_parent_block = block_with_three_signed_transfers(
            1,
            B256::from(bad_parent_hash_bytes),
            genesis_state_root,
            0,
            chain_id,
        );

        let mut execution_rejected_block =
            block_with_three_signed_transfers(4, valid_block_3_hash, genesis_state_root, 9, chain_id);
        let invalid_nonce_tx = legacy_tx(99, Address::from([0x44; 20]), 100);
        execution_rejected_block.transactions[0] =
            sign(&invalid_nonce_tx, &TEST_PRIVATE_KEY, chain_id).unwrap();

        let (tx, rx) = mpsc::channel(1);
        let handle = tokio::spawn(processor.run(rx));

        let send_messages = async {
            for (block, peer_id) in [
                (invalid_parent_block, 1),
                (valid_block_1, 2),
                (valid_block_2, 3),
                (valid_block_3, 4),
                (execution_rejected_block, 5),
            ] {
                tx.send(ProcessorMessage::NewBlock {
                    block,
                    peer_id: PeerId(peer_id),
                })
                .await
                .unwrap();
            }
            tx.send(ProcessorMessage::Shutdown).await.unwrap();
        };

        timeout(Duration::from_secs(1), send_messages).await.unwrap();
        timeout(Duration::from_secs(1), handle).await.unwrap().unwrap();

        let head = shared_head.read().await.clone();
        assert_eq!(head.number, 3);
        assert_eq!(head.hash, valid_block_3_hash);
        assert_eq!(
            metrics
                .blocks_received
                .load(std::sync::atomic::Ordering::Relaxed),
            5
        );
        assert_eq!(
            metrics
                .blocks_committed
                .load(std::sync::atomic::Ordering::Relaxed),
            3
        );
        assert_eq!(
            metrics
                .blocks_rejected_validation
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            metrics
                .blocks_rejected_execution
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            metrics
                .transactions_committed
                .load(std::sync::atomic::Ordering::Relaxed),
            9
        );
    }
}
