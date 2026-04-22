use std::collections::HashMap;

use types::{Address, B256, Transaction};

use crate::{
    error::ExecutionError,
    primitives::{AccountInfo, Block, BlockNumber, Header, Receipt},
    providers::{
        BlockProvider, HeaderProvider, ReceiptProvider, StateProvider, TransactionProvider,
    },
};

#[derive(Debug, Default)]
pub struct InMemoryProvider {
    pub blocks: HashMap<BlockNumber, Block>,
    pub blocks_by_hash: HashMap<B256, BlockNumber>,
    pub transactions: HashMap<B256, (Transaction, BlockNumber)>,
    pub receipts: HashMap<B256, Receipt>,
    pub state: HashMap<Address, AccountInfo>,
    pub storage: HashMap<(Address, B256), B256>,
}

impl InMemoryProvider {
    pub fn insert_block(&mut self, block: Block) {
        self.blocks_by_hash
            .insert(block.header.hash, block.header.block_number);
        self.blocks.insert(block.header.block_number, block);
    }

    pub fn insert_transaction(
        &mut self,
        hash: B256,
        transaction: Transaction,
        block_number: BlockNumber,
    ) {
        self.transactions.insert(hash, (transaction, block_number));
    }

    pub fn insert_receipt(&mut self, receipt: Receipt) {
        self.receipts.insert(receipt.transaction_hash, receipt);
    }

    pub fn set_account(&mut self, address: Address, info: AccountInfo) {
        self.state.insert(address, info);
    }

    pub fn set_storage(&mut self, address: Address, slot: B256, value: B256) {
        self.storage.insert((address, slot), value);
    }
}

impl BlockProvider for InMemoryProvider {
    fn get_block_by_number(&self, number: BlockNumber) -> Result<Block, ExecutionError> {
        self.blocks
            .get(&number)
            .cloned()
            .ok_or(ExecutionError::BlockNotFound { number })
    }

    fn get_block_by_hash(&self, hash: B256) -> Result<Block, ExecutionError> {
        let number = self
            .blocks_by_hash
            .get(&hash)
            .ok_or(ExecutionError::HeaderNotFound { hash })?;
        self.get_block_by_number(*number)
    }
}

impl HeaderProvider for InMemoryProvider {
    fn get_header_by_hash(&self, hash: B256) -> Result<Header, ExecutionError> {
        let number = self
            .blocks_by_hash
            .get(&hash)
            .ok_or(ExecutionError::HeaderNotFound { hash })?;
        self.get_header_by_number(*number)
    }
    fn get_header_by_number(&self, number: BlockNumber) -> Result<Header, ExecutionError> {
        self.blocks
            .get(&number)
            .map(|b| b.header.clone())
            .ok_or(ExecutionError::BlockNotFound { number })
    }
}

impl StateProvider for InMemoryProvider {
    fn get_account(&self, address: Address) -> Result<AccountInfo, ExecutionError> {
        self.state
            .get(&address)
            .cloned()
            .ok_or(ExecutionError::AccountNotFound { address })
    }
    fn get_storage(&self, address: Address, slot: B256) -> Result<B256, ExecutionError> {
        Ok(self
            .storage
            .get(&(address, slot))
            .copied()
            .unwrap_or_default())
    }
}

impl TransactionProvider for InMemoryProvider {
    fn get_transaction(&self, hash: B256) -> Result<Transaction, ExecutionError> {
        self.transactions
            .get(&hash)
            .map(|(tx, _)| tx.clone())
            .ok_or(ExecutionError::TransactionNotFound { hash })
    }

    fn get_block_transactions(
        &self,
        block_number: BlockNumber,
    ) -> Result<Vec<Transaction>, ExecutionError> {
        self.blocks
            .get(&block_number)
            .map(|b| b.transactions.clone())
            .ok_or(ExecutionError::BlockNotFound {
                number: block_number,
            })
    }
}

impl ReceiptProvider for InMemoryProvider {
    fn get_receipt(&self, transaction_hash: B256) -> Result<Receipt, ExecutionError> {
        self.receipts
            .get(&transaction_hash)
            .cloned()
            .ok_or(ExecutionError::ReceiptNotFound {
                hash: transaction_hash,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::FullProvider;
    use types::Bloom;

    const BLOCK_NUMBER: BlockNumber = 7;
    const MISSING_NUMBER: BlockNumber = 9999;

    fn block_hash() -> B256 {
        B256::new([0xaa; 32])
    }
    fn parent_hash() -> B256 {
        B256::new([0x99; 32])
    }
    fn tx_hash_1() -> B256 {
        B256::new([0x01; 32])
    }
    fn tx_hash_2() -> B256 {
        B256::new([0x02; 32])
    }
    fn missing_hash() -> B256 {
        B256::new([0xff; 32])
    }
    fn slot() -> B256 {
        B256::new([0x33; 32])
    }
    fn slot_value() -> B256 {
        B256::new([0x44; 32])
    }
    fn account_addr() -> Address {
        Address::new([0x11; 20])
    }
    fn missing_addr() -> Address {
        Address::new([0xfe; 20])
    }

    fn make_tx(nonce: u64) -> Transaction {
        Transaction::Legacy {
            nonce,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: Some(Address::new([0x22; 20])),
            value: 1_000,
            data: vec![],
        }
    }

    fn make_header() -> Header {
        Header {
            block_number: BLOCK_NUMBER,
            parent_hash: parent_hash(),
            state_root: B256::new([0x55; 32]),
            transactions_root: B256::new([0x66; 32]),
            receipts_root: B256::new([0x77; 32]),
            logs_bloom: Bloom::zero(),
            gas_limit: 30_000_000,
            gas_used: 42_000,
            base_fee_per_gas: 1_000_000_000,
            hash: block_hash(),
        }
    }

    fn make_receipt(tx_hash: B256, tx_index: u64) -> Receipt {
        Receipt {
            transaction_hash: tx_hash,
            transaction_index: tx_index,
            block_hash: block_hash(),
            block_number: BLOCK_NUMBER,
            from: account_addr(),
            to: Some(Address::new([0x22; 20])),
            contract_address: None,
            cumulative_gas_used: 21_000 * (tx_index + 1),
            effective_gas_price: 1_000_000_000,
            gas_used: 21_000,
            status: true,
            logs: vec![],
            logs_bloom: Bloom::zero(),
        }
    }

    fn make_account() -> AccountInfo {
        AccountInfo {
            balance: 1_000_000,
            nonce: 5,
            code_hash: B256::new([0xcc; 32]),
            code: Some(vec![0xde, 0xad, 0xbe, 0xef]),
        }
    }

    fn populated() -> InMemoryProvider {
        let mut provider = InMemoryProvider::default();
        let tx1 = make_tx(0);
        let tx2 = make_tx(1);
        let block = Block {
            header: make_header(),
            transactions: vec![tx1.clone(), tx2.clone()],
        };
        provider.insert_block(block);
        provider.insert_transaction(tx_hash_1(), tx1, BLOCK_NUMBER);
        provider.insert_transaction(tx_hash_2(), tx2, BLOCK_NUMBER);
        provider.insert_receipt(make_receipt(tx_hash_1(), 0));
        provider.insert_receipt(make_receipt(tx_hash_2(), 1));
        provider.set_account(account_addr(), make_account());
        provider.set_storage(account_addr(), slot(), slot_value());
        provider
    }

    fn fetch_block<P: BlockProvider>(p: &P, n: BlockNumber) -> Result<Block, ExecutionError> {
        p.get_block_by_number(n)
    }

    fn fetch_block_by_hash<P: BlockProvider>(p: &P, h: B256) -> Result<Block, ExecutionError> {
        p.get_block_by_hash(h)
    }

    fn fetch_header_by_number<P: HeaderProvider>(
        p: &P,
        n: BlockNumber,
    ) -> Result<Header, ExecutionError> {
        p.get_header_by_number(n)
    }

    fn fetch_header_by_hash<P: HeaderProvider>(p: &P, h: B256) -> Result<Header, ExecutionError> {
        p.get_header_by_hash(h)
    }

    fn fetch_account<P: StateProvider>(p: &P, a: Address) -> Result<AccountInfo, ExecutionError> {
        p.get_account(a)
    }

    fn fetch_balance<P: StateProvider>(p: &P, a: Address) -> Result<u128, ExecutionError> {
        p.get_balance(a)
    }

    fn fetch_nonce<P: StateProvider>(p: &P, a: Address) -> Result<u64, ExecutionError> {
        p.get_nonce(a)
    }

    fn fetch_code<P: StateProvider>(p: &P, a: Address) -> Result<Option<Vec<u8>>, ExecutionError> {
        p.get_code(a)
    }

    fn fetch_storage<P: StateProvider>(
        p: &P,
        a: Address,
        slot: B256,
    ) -> Result<B256, ExecutionError> {
        p.get_storage(a, slot)
    }

    fn fetch_transaction<P: TransactionProvider>(
        p: &P,
        h: B256,
    ) -> Result<Transaction, ExecutionError> {
        p.get_transaction(h)
    }

    fn fetch_block_transactions<P: TransactionProvider>(
        p: &P,
        n: BlockNumber,
    ) -> Result<Vec<Transaction>, ExecutionError> {
        p.get_block_transactions(n)
    }

    fn fetch_receipt<P: ReceiptProvider>(p: &P, h: B256) -> Result<Receipt, ExecutionError> {
        p.get_receipt(h)
    }

    #[test]
    fn block_by_number_success() {
        let p = populated();
        let block = fetch_block(&p, BLOCK_NUMBER).unwrap();
        assert_eq!(block.header.hash, block_hash());
        assert_eq!(block.transactions.len(), 2);
    }

    #[test]
    fn block_by_hash_success() {
        let p = populated();
        let block = fetch_block_by_hash(&p, block_hash()).unwrap();
        assert_eq!(block.header.block_number, BLOCK_NUMBER);
    }

    #[test]
    fn header_by_number_success() {
        let p = populated();
        let header = fetch_header_by_number(&p, BLOCK_NUMBER).unwrap();
        assert_eq!(header.hash, block_hash());
        assert_eq!(header.parent_hash, parent_hash());
    }

    #[test]
    fn header_by_hash_success() {
        let p = populated();
        let header = fetch_header_by_hash(&p, block_hash()).unwrap();
        assert_eq!(header.block_number, BLOCK_NUMBER);
    }

    #[test]
    fn account_success() {
        let p = populated();
        let account = fetch_account(&p, account_addr()).unwrap();
        assert_eq!(account.balance, 1_000_000);
        assert_eq!(account.nonce, 5);
    }

    #[test]
    fn state_default_impls_delegate_to_get_account() {
        let p = populated();
        assert_eq!(fetch_balance(&p, account_addr()).unwrap(), 1_000_000);
        assert_eq!(fetch_nonce(&p, account_addr()).unwrap(), 5);
        assert_eq!(
            fetch_code(&p, account_addr()).unwrap(),
            Some(vec![0xde, 0xad, 0xbe, 0xef])
        );
    }

    #[test]
    fn storage_hit_returns_value() {
        let p = populated();
        assert_eq!(
            fetch_storage(&p, account_addr(), slot()).unwrap(),
            slot_value()
        );
    }

    #[test]
    fn storage_miss_returns_zero() {
        let p = populated();
        let unset_slot = B256::new([0xee; 32]);
        assert_eq!(
            fetch_storage(&p, account_addr(), unset_slot).unwrap(),
            B256::default()
        );
    }

    #[test]
    fn transaction_success() {
        let p = populated();
        let tx = fetch_transaction(&p, tx_hash_1()).unwrap();
        match tx {
            Transaction::Legacy { nonce, .. } => assert_eq!(nonce, 0),
            _ => panic!("expected Legacy"),
        }
    }

    #[test]
    fn block_transactions_success() {
        let p = populated();
        let txs = fetch_block_transactions(&p, BLOCK_NUMBER).unwrap();
        assert_eq!(txs.len(), 2);
    }

    #[test]
    fn receipt_success() {
        let p = populated();
        let r = fetch_receipt(&p, tx_hash_1()).unwrap();
        assert_eq!(r.transaction_hash, tx_hash_1());
        assert_eq!(r.transaction_index, 0);
    }

    #[test]
    fn block_not_found() {
        let p = populated();
        assert!(matches!(
            fetch_block(&p, MISSING_NUMBER),
            Err(ExecutionError::BlockNotFound { .. })
        ));
    }

    #[test]
    fn header_not_found_by_hash() {
        let p = populated();
        assert!(matches!(
            fetch_header_by_hash(&p, missing_hash()),
            Err(ExecutionError::HeaderNotFound { .. })
        ));
    }

    #[test]
    fn account_not_found() {
        let p = populated();
        assert!(matches!(
            fetch_account(&p, missing_addr()),
            Err(ExecutionError::AccountNotFound { .. })
        ));
    }

    #[test]
    fn transaction_not_found() {
        let p = populated();
        assert!(matches!(
            fetch_transaction(&p, missing_hash()),
            Err(ExecutionError::TransactionNotFound { .. })
        ));
    }

    #[test]
    fn receipt_not_found() {
        let p = populated();
        assert!(matches!(
            fetch_receipt(&p, missing_hash()),
            Err(ExecutionError::ReceiptNotFound { .. })
        ));
    }

    fn exercise_all<P: FullProvider>(p: &P) {
        assert!(p.get_block_by_number(BLOCK_NUMBER).is_ok());
        assert!(p.get_header_by_hash(block_hash()).is_ok());
        assert!(p.get_account(account_addr()).is_ok());
        assert!(p.get_transaction(tx_hash_1()).is_ok());
        assert!(p.get_receipt(tx_hash_2()).is_ok());
    }

    #[test]
    fn full_provider_blanket_impl() {
        let p = populated();
        exercise_all(&p);
    }
}
