use types::{Address, B256, Bloom, Transaction};

use crate::{
    error::ExecutionError,
    in_memory::InMemoryProvider,
    primitives::{AccountInfo, Block, Receipt},
    providers::StateProvider,
};

#[derive(Debug)]
pub struct ExecutionOutput {
    pub gas_used: u64,
    pub receipts: Vec<Receipt>,
    pub logs_bloom: Bloom,
}

pub struct TransactionWithSender {
    pub transaction: Transaction,
    pub sender: Address,
    pub hash: B256,
}

pub trait BlockExecutor {
    type Output;
    fn execute(
        &self,
        block: &Block,
        txs_with_senders: &[TransactionWithSender],
        state: &mut InMemoryProvider,
    ) -> Result<Self::Output, ExecutionError>;
}

pub struct ValueTransferExecutor;

impl BlockExecutor for ValueTransferExecutor {
    type Output = ExecutionOutput;
    fn execute(
        &self,
        block: &Block,
        txs_with_senders: &[TransactionWithSender],
        state: &mut InMemoryProvider,
    ) -> Result<Self::Output, ExecutionError> {
        let mut cumulative_gas_used: u64 = 0;
        let mut receipts = vec![];
        let mut logs_bloom: Bloom = Bloom::zero();
        for (i, tx_with_sender) in txs_with_senders.iter().enumerate() {
            let mut account = state.get_account(tx_with_sender.sender)?;
            let sender_nonce = account.nonce;
            match &tx_with_sender.transaction {
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
                            address: tx_with_sender.sender,
                            expected: sender_nonce,
                            actual: *nonce,
                        });
                    }

                    let max_cost = tx_with_sender.transaction.max_cost()?;
                    if max_cost > account.balance {
                        return Err(ExecutionError::InsufficientBalance {
                            address: tx_with_sender.sender,
                            available: account.balance,
                            required: max_cost,
                        });
                    }
                    let effective_gas_price = tx_with_sender
                        .transaction
                        .effective_gas_price(block.header.base_fee_per_gas)?;

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
                    state.set_account(tx_with_sender.sender, account);

                    if let Some(recipient_address) = to {
                        let mut recipient =
                            state.get_account(*recipient_address).or_else(|e| match e {
                                ExecutionError::AccountNotFound { .. } => {
                                    Ok(AccountInfo::default())
                                }
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
                        transaction_hash: tx_with_sender.hash,
                        transaction_index: i as u64,
                        block_hash: block.header.hash,
                        block_number: block.header.block_number,
                        from: tx_with_sender.sender,
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
            }
        }

        Ok(ExecutionOutput {
            gas_used: cumulative_gas_used,
            receipts,
            logs_bloom,
        })
    }
}
