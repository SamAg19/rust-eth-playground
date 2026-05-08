use crate::{Address, Transaction};

#[derive(Clone, Debug)]
pub struct TransactionBuilder {
    nonce: u64,
    gas_price: u128,
    gas_limit: u64,
    to: Option<Address>,
    value: u128,
    data: Vec<u8>,
}

impl Default for TransactionBuilder {
    fn default() -> Self {
        Self {
            nonce: 0,
            gas_price: 1,
            gas_limit: 21_000,
            to: Some(Address::zero()),
            value: 0,
            data: Vec::new(),
        }
    }
}

impl TransactionBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn nonce(mut self, nonce: u64) -> Self {
        self.nonce = nonce;
        self
    }

    pub fn gas_price(mut self, gas_price: u128) -> Self {
        self.gas_price = gas_price;
        self
    }

    pub fn gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit = gas_limit;
        self
    }

    pub fn to(mut self, to: Option<Address>) -> Self {
        self.to = to;
        self
    }

    pub fn value(mut self, value: u128) -> Self {
        self.value = value;
        self
    }

    pub fn data(mut self, data: Vec<u8>) -> Self {
        self.data = data;
        self
    }

    pub fn legacy(self) -> Transaction {
        Transaction::Legacy {
            nonce: self.nonce,
            gas_price: self.gas_price,
            gas_limit: self.gas_limit,
            to: self.to,
            value: self.value,
            data: self.data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_default_legacy_transaction() {
        assert_eq!(
            TransactionBuilder::new().legacy(),
            Transaction::Legacy {
                nonce: 0,
                gas_price: 1,
                gas_limit: 21_000,
                to: Some(Address::zero()),
                value: 0,
                data: Vec::new(),
            }
        );
    }

    #[test]
    fn overrides_legacy_transaction_fields() {
        let to = Address::from([1; 20]);

        assert_eq!(
            TransactionBuilder::new()
                .nonce(7)
                .gas_price(10)
                .gas_limit(50_000)
                .to(Some(to))
                .value(99)
                .data(vec![1, 2, 3])
                .legacy(),
            Transaction::Legacy {
                nonce: 7,
                gas_price: 10,
                gas_limit: 50_000,
                to: Some(to),
                value: 99,
                data: vec![1, 2, 3],
            }
        );
    }
}
