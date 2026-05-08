use std::{cell::RefCell, collections::HashMap};

use rlp_codec::signing::SignedTransaction;
use types::{Address, B256};

use crate::{
    error::ExecutionError,
    primitives::{AccountInfo, Block, BlockNumber, Header, Receipt},
    providers::{
        BlockProvider, HeaderProvider, ReceiptProvider, StateProvider, TransactionProvider,
    },
};

// Interior mutability: provider trait methods take `&self`, but caching requires
// writing to the cache on a read. The three options are:
//   - `RefCell<HashMap<..>>`   — single-threaded, zero overhead, panics on bad borrows.
//   - `Mutex<HashMap<..>>`     — multi-threaded, per-access lock overhead.
//   - change methods to `&mut self` — forces callers to hold a mutable reference,
//     which is inconvenient when multiple components share one provider.
// `RefCell` is used here because this crate is synchronous and single-threaded.
// A production implementation would swap this for `Mutex`, `RwLock`, or a
// concurrent hash map (e.g. `dashmap`) to support sharing across tasks/threads.
pub struct CachedProvider<
    T: BlockProvider + HeaderProvider + StateProvider + TransactionProvider + ReceiptProvider,
> {
    inner: T,
    block_cache: RefCell<HashMap<BlockNumber, Block>>,
    header_cache: RefCell<HashMap<B256, Header>>,
    account_cache: RefCell<HashMap<Address, AccountInfo>>,
    capacity: usize,
}

impl<T: BlockProvider + HeaderProvider + StateProvider + TransactionProvider + ReceiptProvider>
    CachedProvider<T>
{
    pub fn new(inner: T, capacity: usize) -> Self {
        Self {
            inner,
            block_cache: RefCell::new(HashMap::new()),
            header_cache: RefCell::new(HashMap::new()),
            account_cache: RefCell::new(HashMap::new()),
            capacity,
        }
    }
}

impl<T: BlockProvider + HeaderProvider + StateProvider + TransactionProvider + ReceiptProvider>
    BlockProvider for CachedProvider<T>
{
    fn get_block_by_hash(&self, hash: B256) -> Result<Block, ExecutionError> {
        let block = self.inner.get_block_by_hash(hash)?;
        {
            let cache = self.block_cache.borrow();

            if let Some(value) = cache.get(&block.header.block_number) {
                return Ok(value.clone());
            }
        }

        let evicted = {
            let cache = self.block_cache.borrow();
            if cache.len() >= self.capacity {
                cache.keys().next().copied()
            } else {
                None
            }
        };
        let mut cache = self.block_cache.borrow_mut();
        if let Some(key) = evicted {
            cache.remove(&key);
        }

        cache.insert(block.header.block_number, block.clone());
        Ok(block)
    }

    fn get_block_by_number(&self, number: BlockNumber) -> Result<Block, ExecutionError> {
        {
            let cache = self.block_cache.borrow();

            if let Some(value) = cache.get(&number) {
                return Ok(value.clone());
            }
        }
        let block = self.inner.get_block_by_number(number)?;
        let evicted = {
            let cache = self.block_cache.borrow();
            if cache.len() >= self.capacity {
                cache.keys().next().copied()
            } else {
                None
            }
        };
        let mut cache = self.block_cache.borrow_mut();
        if let Some(key) = evicted {
            cache.remove(&key);
        }

        cache.insert(number, block.clone());
        Ok(block)
    }
}

impl<T: BlockProvider + HeaderProvider + StateProvider + TransactionProvider + ReceiptProvider>
    HeaderProvider for CachedProvider<T>
{
    fn get_header_by_hash(&self, hash: B256) -> Result<Header, ExecutionError> {
        {
            let cache = self.header_cache.borrow();

            if let Some(value) = cache.get(&hash) {
                return Ok(value.clone());
            }
        }

        let header = self.inner.get_header_by_hash(hash)?;

        let evicted = {
            let cache = self.header_cache.borrow();
            if cache.len() >= self.capacity {
                cache.keys().next().copied()
            } else {
                None
            }
        };

        let mut cache = self.header_cache.borrow_mut();
        if let Some(key) = evicted {
            cache.remove(&key);
        }
        cache.insert(hash, header.clone());
        Ok(header)
    }

    fn get_header_by_number(&self, number: BlockNumber) -> Result<Header, ExecutionError> {
        let header = self.inner.get_header_by_number(number)?;
        {
            let cache = self.header_cache.borrow();

            if let Some(value) = cache.get(&header.hash) {
                return Ok(value.clone());
            }
        }

        let evicted = {
            let cache = self.header_cache.borrow();
            if cache.len() >= self.capacity {
                cache.keys().next().copied()
            } else {
                None
            }
        };

        let mut cache = self.header_cache.borrow_mut();

        if let Some(key) = evicted {
            cache.remove(&key);
        }

        cache.insert(header.hash, header.clone());
        Ok(header)
    }
}

impl<T: BlockProvider + HeaderProvider + StateProvider + TransactionProvider + ReceiptProvider>
    StateProvider for CachedProvider<T>
{
    fn get_account(&self, address: Address) -> Result<AccountInfo, ExecutionError> {
        {
            let cache = self.account_cache.borrow();

            if let Some(value) = cache.get(&address) {
                return Ok(value.clone());
            }
        }

        let info = self.inner.get_account(address)?;
        let evicted = {
            let cache = self.account_cache.borrow();
            if cache.len() >= self.capacity {
                cache.keys().next().copied()
            } else {
                None
            }
        };

        let mut cache = self.account_cache.borrow_mut();
        if let Some(key) = evicted {
            cache.remove(&key);
        }

        cache.insert(address, info.clone());
        Ok(info)
    }

    fn get_storage(&self, address: Address, slot: B256) -> Result<B256, ExecutionError> {
        self.inner.get_storage(address, slot)
    }
}

impl<T: BlockProvider + HeaderProvider + StateProvider + TransactionProvider + ReceiptProvider>
    TransactionProvider for CachedProvider<T>
{
    fn get_block_transactions(
        &self,
        block_number: BlockNumber,
    ) -> Result<Vec<SignedTransaction>, ExecutionError> {
        self.inner.get_block_transactions(block_number)
    }

    fn get_transaction(&self, hash: B256) -> Result<SignedTransaction, ExecutionError> {
        self.inner.get_transaction(hash)
    }
}

impl<T: BlockProvider + HeaderProvider + StateProvider + TransactionProvider + ReceiptProvider>
    ReceiptProvider for CachedProvider<T>
{
    fn get_receipt(&self, transaction_hash: B256) -> Result<Receipt, ExecutionError> {
        self.inner.get_receipt(transaction_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::in_memory::InMemoryProvider;
    use crate::providers::FullProvider;
    use types::Bloom;

    // Wraps an `InMemoryProvider` and counts calls into the three methods that
    // `CachedProvider` is supposed to short-circuit on a cache hit.
    struct CountingProvider {
        inner: InMemoryProvider,
        block_by_number_calls: RefCell<usize>,
        header_by_hash_calls: RefCell<usize>,
        account_calls: RefCell<usize>,
    }

    impl CountingProvider {
        fn new(inner: InMemoryProvider) -> Self {
            Self {
                inner,
                block_by_number_calls: RefCell::new(0),
                header_by_hash_calls: RefCell::new(0),
                account_calls: RefCell::new(0),
            }
        }
    }

    impl BlockProvider for CountingProvider {
        fn get_block_by_number(&self, number: BlockNumber) -> Result<Block, ExecutionError> {
            *self.block_by_number_calls.borrow_mut() += 1;
            self.inner.get_block_by_number(number)
        }
        fn get_block_by_hash(&self, hash: B256) -> Result<Block, ExecutionError> {
            self.inner.get_block_by_hash(hash)
        }
    }

    impl HeaderProvider for CountingProvider {
        fn get_header_by_hash(&self, hash: B256) -> Result<Header, ExecutionError> {
            *self.header_by_hash_calls.borrow_mut() += 1;
            self.inner.get_header_by_hash(hash)
        }
        fn get_header_by_number(&self, number: BlockNumber) -> Result<Header, ExecutionError> {
            self.inner.get_header_by_number(number)
        }
    }

    impl StateProvider for CountingProvider {
        fn get_account(&self, address: Address) -> Result<AccountInfo, ExecutionError> {
            *self.account_calls.borrow_mut() += 1;
            self.inner.get_account(address)
        }
        fn get_storage(&self, address: Address, slot: B256) -> Result<B256, ExecutionError> {
            self.inner.get_storage(address, slot)
        }
    }

    impl TransactionProvider for CountingProvider {
        fn get_transaction(&self, hash: B256) -> Result<SignedTransaction, ExecutionError> {
            self.inner.get_transaction(hash)
        }
        fn get_block_transactions(
            &self,
            block_number: BlockNumber,
        ) -> Result<Vec<SignedTransaction>, ExecutionError> {
            self.inner.get_block_transactions(block_number)
        }
    }

    impl ReceiptProvider for CountingProvider {
        fn get_receipt(&self, transaction_hash: B256) -> Result<Receipt, ExecutionError> {
            self.inner.get_receipt(transaction_hash)
        }
    }

    fn make_header(number: BlockNumber) -> Header {
        Header {
            block_number: number,
            parent_hash: B256::new([0x99; 32]),
            state_root: B256::new([0x55; 32]),
            transactions_root: B256::new([0x66; 32]),
            receipts_root: B256::new([0x77; 32]),
            logs_bloom: Bloom::zero(),
            gas_limit: 30_000_000,
            gas_used: 0,
            base_fee_per_gas: 1_000_000_000,
            hash: B256::new([number as u8; 32]),
        }
    }

    fn make_block(number: BlockNumber) -> Block {
        Block {
            header: make_header(number),
            transactions: vec![],
        }
    }

    fn populated_inner() -> InMemoryProvider {
        let mut p = InMemoryProvider::default();
        for n in 0u64..5 {
            p.insert_block(make_block(n)).unwrap();
        }
        p.set_account(Address::new([0x11; 20]), AccountInfo::default());
        p
    }

    #[test]
    fn first_read_calls_inner_and_populates_cache() {
        let counting = CountingProvider::new(populated_inner());
        let cached = CachedProvider::new(counting, 10);

        let block = cached.get_block_by_number(1).unwrap();
        assert_eq!(block.header.block_number, 1);
        assert_eq!(*cached.inner.block_by_number_calls.borrow(), 1);
        assert!(cached.block_cache.borrow().contains_key(&1));
    }

    #[test]
    fn second_read_does_not_call_inner() {
        let counting = CountingProvider::new(populated_inner());
        let cached = CachedProvider::new(counting, 10);

        let _ = cached.get_block_by_number(2).unwrap();
        let _ = cached.get_block_by_number(2).unwrap();
        let _ = cached.get_block_by_number(2).unwrap();

        assert_eq!(*cached.inner.block_by_number_calls.borrow(), 1);
    }

    #[test]
    fn header_cache_hit_skips_inner() {
        let counting = CountingProvider::new(populated_inner());
        let cached = CachedProvider::new(counting, 10);

        let hash = make_header(3).hash;
        let _ = cached.get_header_by_hash(hash).unwrap();
        let _ = cached.get_header_by_hash(hash).unwrap();

        assert_eq!(*cached.inner.header_by_hash_calls.borrow(), 1);
    }

    #[test]
    fn account_cache_hit_skips_inner() {
        let counting = CountingProvider::new(populated_inner());
        let cached = CachedProvider::new(counting, 10);

        let addr = Address::new([0x11; 20]);
        let _ = cached.get_account(addr).unwrap();
        let _ = cached.get_account(addr).unwrap();

        assert_eq!(*cached.inner.account_calls.borrow(), 1);
    }

    #[test]
    fn default_impls_benefit_from_account_cache() {
        let counting = CountingProvider::new(populated_inner());
        let cached = CachedProvider::new(counting, 10);

        let addr = Address::new([0x11; 20]);
        let _ = cached.get_balance(addr).unwrap();
        let _ = cached.get_nonce(addr).unwrap();
        let _ = cached.get_code(addr).unwrap();

        // All three delegate to `get_account`; only the first should hit the inner.
        assert_eq!(*cached.inner.account_calls.borrow(), 1);
    }

    #[test]
    fn eviction_bounds_cache_size() {
        let counting = CountingProvider::new(populated_inner());
        let cached = CachedProvider::new(counting, 2);

        for n in 0u64..5 {
            let _ = cached.get_block_by_number(n).unwrap();
        }

        assert!(cached.block_cache.borrow().len() <= 2);
    }

    fn exercise_full_provider<P: FullProvider>(p: &P, tx_hash: B256, addr: Address) {
        assert!(p.get_block_by_number(0).is_ok());
        assert!(p.get_header_by_hash(make_header(0).hash).is_ok());
        assert!(p.get_account(addr).is_ok());
        assert!(p.get_storage(addr, B256::default()).is_ok());
        assert!(p.get_transaction(tx_hash).is_err()); // no tx inserted — just checks the method is reachable
        assert!(p.get_receipt(tx_hash).is_err());
    }

    #[test]
    fn cached_provider_satisfies_full_provider() {
        let counting = CountingProvider::new(populated_inner());
        let cached = CachedProvider::new(counting, 10);
        exercise_full_provider(&cached, B256::new([0xaa; 32]), Address::new([0x11; 20]));
    }
}
