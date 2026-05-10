#[macro_use]
mod macros;

pub mod account;
pub mod address;
pub mod b256;
pub mod block;
pub mod bloom;
pub mod chain_head;
pub mod constant;
pub mod error;
pub mod genesis;
pub mod header;
pub mod opcode;
#[cfg(any(feature = "test-utils", test))]
pub mod test_helpers;
pub mod transaction;
pub mod transaction_error;

pub use account::Account;
pub use address::Address;
pub use b256::B256;
pub use block::{Block, SignedTransaction};
pub use bloom::Bloom;
pub use chain_head::ChainHead;
pub use constant::GAS_LIMIT_PER_BLOCK;
pub use error::ParseError;
pub use genesis::{GenesisAccount, GenesisConfig};
pub use header::Header;
pub use opcode::{Opcode, OpcodeError};
pub use transaction::{AccessListItem, Transaction, TransactionSummary};
pub use transaction_error::{DecodeError, TransactionError};

pub trait TypeName {
    fn type_name() -> &'static str;
}

impl_type_name! {
    Address => "Address",
    B256    => "B256",
    Bloom   => "Bloom",
    Transaction => "Transaction"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_name_returns_address_name() {
        assert_eq!(Address::type_name(), "Address");
    }

    #[test]
    fn type_name_returns_b256_name() {
        assert_eq!(B256::type_name(), "B256");
    }

    #[test]
    fn type_name_returns_bloom_name() {
        assert_eq!(Bloom::type_name(), "Bloom");
    }

    #[test]
    fn type_name_returns_transaction_name() {
        assert_eq!(Transaction::type_name(), "Transaction");
    }
}
