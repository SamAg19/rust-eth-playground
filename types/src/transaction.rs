use crate::address::Address;
use crate::b256::B256;
use crate::transaction_error::TransactionError;
use std::cmp::min;

#[derive(Clone, Debug, PartialEq)]
pub struct AccessListItem {
    pub address: Address,
    pub storage_keys: Vec<B256>,
}

// nonce
// gas-limit
// to
// value
// data
#[derive(Clone, Debug, PartialEq)]
pub enum Transaction {
    Legacy {
        nonce: u64,
        gas_price: u128,
        gas_limit: u64,
        to: Option<Address>,
        value: u128,
        data: Vec<u8>,
    },
    Eip1559 {
        nonce: u64,
        max_priority_fee_per_gas: u128,
        max_fee_per_gas: u128,
        gas_limit: u64,
        to: Option<Address>,
        value: u128,
        data: Vec<u8>,
        access_list: Vec<AccessListItem>,
    },
    Eip4844 {
        nonce: u64,
        max_priority_fee_per_gas: u128,
        max_fee_per_gas: u128,
        max_fee_per_blob_gas: u128,
        gas_limit: u64,
        to: Option<Address>,
        value: u128,
        data: Vec<u8>,
        access_list: Vec<AccessListItem>,
        blob_versioned_hashes: Vec<B256>,
    },
}

impl Transaction {
    pub fn tx_type(&self) -> Result<u8, TransactionError> {
        match self {
            Transaction::Legacy { .. } => Ok(0),
            Transaction::Eip1559 { .. } => Ok(1),
            Transaction::Eip4844 { .. } => Ok(2),
        }
    }

    pub fn is_create(&self) -> Result<bool, TransactionError> {
        match self {
            Transaction::Legacy { to, .. }
            | Transaction::Eip1559 { to, .. }
            | Transaction::Eip4844 { to, .. } => Ok(to.is_none()),
        }
    }

    pub fn effective_gas_price(&self, base_fee: u128) -> Result<u128, TransactionError> {
        match self {
            Transaction::Legacy { gas_price, .. } => Ok(*gas_price),
            Transaction::Eip1559 {
                max_fee_per_gas,
                max_priority_fee_per_gas,
                ..
            }
            | Transaction::Eip4844 {
                max_fee_per_gas,
                max_priority_fee_per_gas,
                ..
            } => {
                if base_fee > *max_fee_per_gas {
                    return Err(TransactionError::InsufficientMaxFee {
                        base_fee,
                        max_fee: *max_fee_per_gas,
                    });
                }
                let addition = base_fee
                    .checked_add(*max_priority_fee_per_gas)
                    .ok_or(TransactionError::Overflow)?;
                let effective_gas_price = min(*max_fee_per_gas, addition);
                Ok(effective_gas_price)
            }
        }
    }

    pub fn max_cost(&self) -> Result<u128, TransactionError> {
        match self {
            Transaction::Legacy {
                gas_price,
                gas_limit,
                value,
                ..
            } => {
                let multiplication = gas_price
                    .checked_mul((*gas_limit).into())
                    .ok_or(TransactionError::Overflow)?;
                let max_cost = multiplication
                    .checked_add(*value)
                    .ok_or(TransactionError::Overflow)?;
                Ok(max_cost)
            }
            Transaction::Eip1559 {
                gas_limit,
                value,
                max_fee_per_gas,
                ..
            }
            | Transaction::Eip4844 {
                gas_limit,
                value,
                max_fee_per_gas,
                ..
            } => {
                let multiplication = max_fee_per_gas
                    .checked_mul((*gas_limit).into())
                    .ok_or(TransactionError::Overflow)?;
                let max_cost = multiplication
                    .checked_add(*value)
                    .ok_or(TransactionError::Overflow)?;
                Ok(max_cost)
            }
        }
    }
}

pub struct TransactionSummary {
    pub total_value: u128,
    pub total_gas_limit: u64,
    pub create_count: usize,
    pub tx_count: usize,
}

pub fn summarise_transactions(txs: &[Transaction]) -> Result<TransactionSummary, TransactionError> {
    let mut total_value = 0;
    let mut total_gas_limit = 0;
    let mut create_count = 0;
    let tx_count = txs.len();

    for tx in txs {
        let (value, gas_limit) = match tx {
            Transaction::Legacy {
                value, gas_limit, ..
            }
            | Transaction::Eip1559 {
                value, gas_limit, ..
            }
            | Transaction::Eip4844 {
                value, gas_limit, ..
            } => (value, gas_limit),
        };

        total_value = value
            .checked_add(total_value)
            .ok_or(TransactionError::Overflow)?;
        total_gas_limit = gas_limit
            .checked_add(total_gas_limit)
            .ok_or(TransactionError::Overflow)?;
        if tx.is_create()? {
            create_count += 1;
        }
    }

    Ok(TransactionSummary {
        total_value,
        total_gas_limit,
        create_count,
        tx_count,
    })
}

#[cfg(test)]

mod tests {
    use super::*;

    #[test]
    fn test_tx_type() {
        let legacy_tx = Transaction::Legacy {
            nonce: 0,
            gas_price: 100,
            gas_limit: 21000,
            to: Some(Address::zero()),
            value: 0,
            data: vec![],
        };

        assert_eq!(legacy_tx.tx_type().unwrap(), 0);

        let eip1559_tx = Transaction::Eip1559 {
            nonce: 0,
            max_priority_fee_per_gas: 2,
            max_fee_per_gas: 100,
            gas_limit: 21000,
            to: Some(Address::zero()),
            value: 0,
            data: vec![],
            access_list: vec![],
        };

        assert_eq!(eip1559_tx.tx_type().unwrap(), 1);

        let eip4844_tx = Transaction::Eip4844 {
            nonce: 0,
            max_priority_fee_per_gas: 2,
            max_fee_per_gas: 100,
            max_fee_per_blob_gas: 50,
            gas_limit: 21000,
            to: Some(Address::zero()),
            value: 0,
            data: vec![],
            access_list: vec![],
            blob_versioned_hashes: vec![],
        };

        assert_eq!(eip4844_tx.tx_type().unwrap(), 2);
    }

    #[test]
    fn test_is_create() {
        let create_tx = Transaction::Eip4844 {
            nonce: 0,
            max_priority_fee_per_gas: 2,
            max_fee_per_gas: 100,
            max_fee_per_blob_gas: 50,
            gas_limit: 21000,
            to: None,
            value: 0,
            data: vec![],
            access_list: vec![],
            blob_versioned_hashes: vec![],
        };

        assert_eq!(create_tx.is_create().unwrap(), true);

        let non_create_tx = Transaction::Eip4844 {
            nonce: 0,
            max_priority_fee_per_gas: 2,
            max_fee_per_gas: 100,
            max_fee_per_blob_gas: 50,
            gas_limit: 21000,
            to: Some(Address::zero()),
            value: 0,
            data: vec![],
            access_list: vec![],
            blob_versioned_hashes: vec![],
        };

        assert_eq!(non_create_tx.is_create().unwrap(), false);
    }

    #[test]
    fn test_effective_gas_price_legacy() {
        let legacy_tx = Transaction::Legacy {
            nonce: 0,
            gas_price: 100,
            gas_limit: 21000,
            to: Some(Address::zero()),
            value: 0,
            data: vec![],
        };

        assert_eq!((legacy_tx.effective_gas_price(50).unwrap()), 100);
    }

    #[test]
    fn test_effective_gas_price_eip1559() {
        let eip1559_tx = Transaction::Eip1559 {
            nonce: 0,
            max_priority_fee_per_gas: 2,
            max_fee_per_gas: 100,
            gas_limit: 21000,
            to: Some(Address::zero()),
            value: 0,
            data: vec![],
            access_list: vec![],
        };

        // Case where base fee + priority fee is less than max fee
        assert_eq!((eip1559_tx.effective_gas_price(50).unwrap()), 52);
        assert_eq!((eip1559_tx.effective_gas_price(10).unwrap()), 12);
        // Case where base fee + priority fee exceeds max fee
        assert_eq!((eip1559_tx.effective_gas_price(99).unwrap()), 100);
        assert_eq!((eip1559_tx.effective_gas_price(100).unwrap()), 100);
        // Case where base fee exceeds max fee
        assert!(matches!(
            eip1559_tx.effective_gas_price(101),
            Err(TransactionError::InsufficientMaxFee {
                base_fee: 101,
                max_fee: 100
            })
        ));
    }

    #[test]
    fn test_effective_gas_price_eip4844() {
        let eip4844_tx = Transaction::Eip4844 {
            nonce: 0,
            max_priority_fee_per_gas: 2,
            max_fee_per_gas: 100,
            max_fee_per_blob_gas: 50,
            gas_limit: 21000,
            to: Some(Address::zero()),
            value: 0,
            data: vec![],
            access_list: vec![],
            blob_versioned_hashes: vec![],
        };

        // Case where base fee + priority fee is less than max fee
        assert_eq!((eip4844_tx.effective_gas_price(50).unwrap()), 52);
        assert_eq!((eip4844_tx.effective_gas_price(10).unwrap()), 12);
        // Case where base fee + priority fee exceeds max fee
        assert_eq!((eip4844_tx.effective_gas_price(99).unwrap()), 100);
        assert_eq!((eip4844_tx.effective_gas_price(100).unwrap()), 100);
        // Case where base fee exceeds max fee
        assert!(matches!(
            eip4844_tx.effective_gas_price(101),
            Err(TransactionError::InsufficientMaxFee {
                base_fee: 101,
                max_fee: 100
            })
        ));
    }

    #[test]
    fn test_effective_gas_price_overflow() {
        let eip1559_tx = Transaction::Eip1559 {
            nonce: 0,
            max_priority_fee_per_gas: u128::MAX,
            max_fee_per_gas: 100,
            gas_limit: 21000,
            to: Some(Address::zero()),
            value: 0,
            data: vec![],
            access_list: vec![],
        };

        assert!(matches!(
            eip1559_tx.effective_gas_price(1),
            Err(TransactionError::Overflow)
        ));
    }

    #[test]
    fn test_max_cost_legacy() {
        let legacy_tx_1 = Transaction::Legacy {
            nonce: 0,
            gas_price: 100,
            gas_limit: 21000,
            to: Some(Address::zero()),
            value: 0,
            data: vec![],
        };

        assert_eq!(legacy_tx_1.max_cost().unwrap(), 2100000);

        let legacy_tx_2 = Transaction::Legacy {
            nonce: 0,
            gas_price: 100,
            gas_limit: 21000,
            to: Some(Address::zero()),
            value: 50000,
            data: vec![],
        };

        assert_eq!(legacy_tx_2.max_cost().unwrap(), 2150000);
    }

    #[test]
    fn test_max_cost_eip1559() {
        let eip1559_tx_1 = Transaction::Eip1559 {
            nonce: 0,
            max_priority_fee_per_gas: 2,
            max_fee_per_gas: 100,
            gas_limit: 21000,
            to: Some(Address::zero()),
            value: 50000,
            data: vec![],
            access_list: vec![],
        };

        assert_eq!(eip1559_tx_1.max_cost().unwrap(), 2150000);

        let eip1559_tx_2 = Transaction::Eip1559 {
            nonce: 0,
            max_priority_fee_per_gas: 2,
            max_fee_per_gas: 100,
            gas_limit: 21000,
            to: Some(Address::zero()),
            value: 0,
            data: vec![],
            access_list: vec![],
        };

        assert_eq!(eip1559_tx_2.max_cost().unwrap(), 2100000);
    }

    #[test]
    fn test_max_cost_overflow() {
        let eip1559_tx = Transaction::Eip1559 {
            nonce: 0,
            max_priority_fee_per_gas: 2,
            max_fee_per_gas: 100,
            gas_limit: 21000,
            to: Some(Address::zero()),
            value: u128::MAX,
            data: vec![],
            access_list: vec![],
        };

        assert!(matches!(
            eip1559_tx.max_cost(),
            Err(TransactionError::Overflow)
        ));
    }

    #[test]
    fn test_empty_summarise_transactions() {
        let summary = summarise_transactions(&[]).unwrap();
        assert_eq!(summary.total_value, 0);
        assert_eq!(summary.total_gas_limit, 0);
        assert_eq!(summary.create_count, 0);
        assert_eq!(summary.tx_count, 0);
    }

    #[test]
    fn test_multiple_summarise_transactions() {
        let txs = vec![
            Transaction::Legacy {
                nonce: 0,
                gas_price: 100,
                gas_limit: 21000,
                to: Some(Address::zero()),
                value: 50000,
                data: vec![],
            },
            Transaction::Eip1559 {
                nonce: 0,
                max_priority_fee_per_gas: 2,
                max_fee_per_gas: 100,
                gas_limit: 21000,
                to: Some(Address::zero()),
                value: 100000,
                data: vec![],
                access_list: vec![],
            },
            Transaction::Eip4844 {
                nonce: 0,
                max_priority_fee_per_gas: 2,
                max_fee_per_gas: 100,
                max_fee_per_blob_gas: 50,
                gas_limit: 21000,
                to: None,
                value: 0,
                data: vec![],
                access_list: vec![],
                blob_versioned_hashes: vec![],
            },
        ];

        let summary = summarise_transactions(&txs).unwrap();
        assert_eq!(summary.total_value, 150000);
        assert_eq!(summary.total_gas_limit, 63000);
        assert_eq!(summary.create_count, 1);
        assert_eq!(summary.tx_count, 3);
    }

    #[test]
    fn test_summarise_transactions_overflow() {
        let txs = vec![
            Transaction::Legacy {
                nonce: 0,
                gas_price: 100,
                gas_limit: 21000,
                to: Some(Address::zero()),
                value: 50000,
                data: vec![],
            },
            Transaction::Eip1559 {
                nonce: 0,
                max_priority_fee_per_gas: 2,
                max_fee_per_gas: 100,
                gas_limit: 21000,
                to: Some(Address::zero()),
                value: 100000,
                data: vec![],
                access_list: vec![],
            },
            Transaction::Eip4844 {
                nonce: 0,
                max_priority_fee_per_gas: 2,
                max_fee_per_gas: 100,
                max_fee_per_blob_gas: 50,
                gas_limit: 21000,
                to: None,
                value: u128::MAX,
                data: vec![],
                access_list: vec![],
                blob_versioned_hashes: vec![],
            },
        ];

        assert!(matches!(
            summarise_transactions(&txs),
            Err(TransactionError::Overflow)
        ));
    }
}
