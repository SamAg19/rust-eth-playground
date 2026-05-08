use rlp_codec::signing::SignedTransaction;
use types::{Address, B256};

use crate::{
    error::ExecutionError,
    primitives::{AccountInfo, Block, BlockNumber, Header, Receipt},
};

// a component that only needs headers should not be forced to depend on receipt logic;
// testing a component that uses headers is simpler if it only needs to mock `HeaderProvider`;
// composing subsets is easy with supertrait bounds;
// splitting is the approach Reth uses in production.

pub trait BlockProvider {
    fn get_block_by_number(&self, number: BlockNumber) -> Result<Block, ExecutionError>;
    fn get_block_by_hash(&self, hash: B256) -> Result<Block, ExecutionError>;
}

pub trait HeaderProvider {
    fn get_header_by_number(&self, number: BlockNumber) -> Result<Header, ExecutionError>;
    fn get_header_by_hash(&self, hash: B256) -> Result<Header, ExecutionError>;
}

pub trait StateProvider {
    fn get_account(&self, address: Address) -> Result<AccountInfo, ExecutionError>;
    fn get_balance(&self, address: Address) -> Result<u128, ExecutionError> {
        let account_info = self.get_account(address)?;
        Ok(account_info.balance)
    }
    fn get_nonce(&self, address: Address) -> Result<u64, ExecutionError> {
        let account_info = self.get_account(address)?;
        Ok(account_info.nonce)
    }
    fn get_code(&self, address: Address) -> Result<Option<Vec<u8>>, ExecutionError> {
        let account_info = self.get_account(address)?;
        Ok(account_info.code)
    }
    fn get_storage(&self, address: Address, slot: B256) -> Result<B256, ExecutionError>;
}

pub trait TransactionProvider {
    fn get_transaction(&self, hash: B256) -> Result<SignedTransaction, ExecutionError>;
    fn get_block_transactions(
        &self,
        block_number: BlockNumber,
    ) -> Result<Vec<SignedTransaction>, ExecutionError>;
}

pub trait ReceiptProvider {
    fn get_receipt(&self, transaction_hash: B256) -> Result<Receipt, ExecutionError>;
}

// a function that requires `T: FullProvider` is equivalent to requiring all five individual bounds, but more concise.
// The blanket impl means implementors never need to write `impl FullProvider for MyType {}` explicitly —
// they just implement the five individual traits and get `FullProvider` for free.
pub trait FullProvider:
    BlockProvider + HeaderProvider + StateProvider + TransactionProvider + ReceiptProvider
{
}

impl<T> FullProvider for T where
    T: BlockProvider + HeaderProvider + StateProvider + TransactionProvider + ReceiptProvider
{
}
