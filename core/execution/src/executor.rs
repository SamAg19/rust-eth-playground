use types::{Account, Address, B256, Block, Bloom, Transaction};

use crate::{
    error::ExecutionError, in_memory::InMemoryProvider, primitives::Receipt,
    providers::StateProvider,
};

use bytes::BytesMut;
use rlp_codec::{
    RlpEncodable, RlpItem, encode, hash_header, signed_transaction_hash,
    trie::{MerkleTrie, TrieError},
};

pub fn compute_state_root(provider: &InMemoryProvider) -> Result<B256, TrieError> {
    // B256::zero() is a placeholder storage root because storage tries are out of scope.
    let storage_root = B256::zero();

    let mut state_trie = MerkleTrie::new();
    for (address, info) in provider.state.iter() {
        let account_rlp = RlpItem::List(vec![
            info.nonce.to_rlp_item(),
            info.balance.to_rlp_item(),
            storage_root.to_rlp_item(),
            info.code_hash.to_rlp_item(),
        ]);
        let mut buffer = BytesMut::new();
        encode(&account_rlp, &mut buffer)?;
        let bytes = buffer.freeze();
        state_trie.insert(address.as_bytes(), bytes.to_vec())?;
    }
    state_trie.root_hash()
}

#[derive(Debug)]
pub struct ExecutionOutput {
    pub gas_used: u64,
    pub receipts: Vec<Receipt>,
    pub logs_bloom: Bloom,
    pub state_root: B256,
}

pub struct BlockWithSenders {
    pub block: Block,
    pub senders: Vec<Address>,
}

pub trait BlockExecutor {
    type Output;
    fn execute(
        &self,
        block_with_senders: &BlockWithSenders,
        state: &mut InMemoryProvider,
    ) -> Result<Self::Output, ExecutionError>;
}

pub struct ValueTransferExecutor;

impl BlockExecutor for ValueTransferExecutor {
    type Output = ExecutionOutput;
    fn execute(
        &self,
        block_with_senders: &BlockWithSenders,
        state: &mut InMemoryProvider,
    ) -> Result<Self::Output, ExecutionError> {
        let mut cumulative_gas_used: u64 = 0;
        let mut receipts = vec![];
        let mut logs_bloom: Bloom = Bloom::zero();
        let block_hash = hash_header(&block_with_senders.block.header)?;
        let base_fee_per_gas = block_with_senders.block.header.gas_limit as u128 / 2;
        for (i, (signed_tx, sender)) in block_with_senders
            .block
            .transactions
            .iter()
            .zip(block_with_senders.senders.iter())
            .enumerate()
        {
            let mut account = state.get_account(*sender)?;
            let sender_nonce = account.nonce;
            match &signed_tx.transaction {
                Transaction::Legacy {
                    nonce,
                    gas_limit,
                    value,
                    to,
                    ..
                }
                | Transaction::Eip1559 {
                    nonce,
                    gas_limit,
                    value,
                    to,
                    ..
                }
                | Transaction::Eip4844 {
                    nonce,
                    gas_limit,
                    value,
                    to,
                    ..
                } => {
                    if *nonce != sender_nonce {
                        return Err(ExecutionError::InvalidNonce {
                            address: *sender,
                            expected: sender_nonce,
                            actual: *nonce,
                        });
                    }

                    let max_cost = signed_tx.transaction.max_cost()?;
                    if max_cost > account.balance {
                        return Err(ExecutionError::InsufficientBalance {
                            address: *sender,
                            available: account.balance,
                            required: max_cost,
                        });
                    }
                    let effective_gas_price = signed_tx
                        .transaction
                        .effective_gas_price(base_fee_per_gas)?;

                    account.balance = account
                        .balance
                        .checked_sub(
                            effective_gas_price
                                .checked_mul(*gas_limit as u128)
                                .ok_or(ExecutionError::Overflow)?
                                .checked_add(*value)
                                .ok_or(ExecutionError::Overflow)?,
                        )
                        .ok_or(ExecutionError::Underflow)?;

                    account.nonce = account
                        .nonce
                        .checked_add(1)
                        .ok_or(ExecutionError::Overflow)?;
                    state.set_account(*sender, account);

                    if let Some(recipient_address) = to {
                        let mut recipient =
                            state.get_account(*recipient_address).or_else(|e| match e {
                                ExecutionError::AccountNotFound { .. } => Ok(Account::default()),
                                other => Err(other),
                            })?;
                        recipient.balance = recipient
                            .balance
                            .checked_add(*value)
                            .ok_or(ExecutionError::Overflow)?;
                        state.set_account(*recipient_address, recipient);
                    } else {
                        todo!()
                    }
                    let gas_used = *gas_limit;
                    cumulative_gas_used = cumulative_gas_used
                        .checked_add(gas_used)
                        .ok_or(ExecutionError::Overflow)?;
                    let receipt = Receipt {
                        transaction_hash: signed_transaction_hash(signed_tx)?,
                        transaction_index: i as u64,
                        block_hash,
                        block_number: block_with_senders.block.header.number,
                        from: *sender,
                        to: *to,
                        contract_address: None,
                        cumulative_gas_used,
                        effective_gas_price,
                        gas_used,
                        status: true,
                        logs: vec![],
                        logs_bloom: Bloom::zero(),
                    };

                    logs_bloom |= &receipt.logs_bloom;
                    receipts.push(receipt);
                }
                #[cfg(feature = "optimism")]
                Transaction::Deposit {
                    from,
                    to,
                    mint,
                    value,
                    ..
                } => {
                    let effective_gas_price = signed_tx
                        .transaction
                        .effective_gas_price(base_fee_per_gas)?;

                    if let Some(recipient_address) = to {
                        let mut recipient =
                            state.get_account(*recipient_address).or_else(|e| match e {
                                ExecutionError::AccountNotFound { .. } => Ok(Account::default()),
                                other => Err(other),
                            })?;
                        recipient.balance = recipient
                            .balance
                            .checked_add(mint.checked_add(*value).ok_or(ExecutionError::Overflow)?)
                            .ok_or(ExecutionError::Overflow)?;
                        state.set_account(*recipient_address, recipient);
                    } else {
                        todo!()
                    }
                    let receipt = Receipt {
                        transaction_hash: signed_transaction_hash(signed_tx)?,
                        transaction_index: i as u64,
                        block_hash,
                        block_number: block_with_senders.block.header.number,
                        from: *from,
                        to: *to,
                        contract_address: None,
                        cumulative_gas_used,
                        effective_gas_price,
                        gas_used: 0,
                        status: true,
                        logs: vec![],
                        logs_bloom: Bloom::zero(),
                    };

                    logs_bloom |= &receipt.logs_bloom;
                    receipts.push(receipt);
                }
            }
        }

        Ok(ExecutionOutput {
            gas_used: cumulative_gas_used,
            receipts,
            logs_bloom,
            state_root: compute_state_root(state)?,
        })
    }
}
