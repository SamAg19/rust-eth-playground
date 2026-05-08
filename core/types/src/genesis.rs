use serde::{Deserialize, Serialize};

use crate::{Address, B256, GAS_LIMIT_PER_BLOCK, Header};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenesisAccount {
    pub address: Address,
    pub balance: u128,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenesisConfig {
    pub chain_id: u64,
    pub accounts: Vec<GenesisAccount>,
    pub genesis_timestamp: u64,
}

impl GenesisConfig {
    pub fn genesis_header(&self) -> Header {
        Header {
            parent_hash: B256::zero(),
            beneficiary: Address::zero(),
            state_root: B256::zero(),
            transactions_root: B256::zero(),
            gas_limit: GAS_LIMIT_PER_BLOCK,
            gas_used: 0,
            timestamp: self.genesis_timestamp,
            number: 0,
        }
    }

    pub fn genesis_header_with_state_root(&self, state_root: B256) -> Header {
        Header {
            state_root,
            ..self.genesis_header()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn genesis_config() -> GenesisConfig {
        GenesisConfig {
            chain_id: 1337,
            accounts: vec![
                GenesisAccount {
                    address: Address::from([0x01; 20]),
                    balance: 1_000,
                },
                GenesisAccount {
                    address: Address::from([0x02; 20]),
                    balance: 2_000,
                },
            ],
            genesis_timestamp: 1_700_000_000,
        }
    }

    #[test]
    fn genesis_header_uses_expected_zero_and_constant_values() {
        let config = genesis_config();
        let header = config.genesis_header();

        assert_eq!(header.parent_hash, B256::zero());
        assert_eq!(header.beneficiary, Address::zero());
        assert_eq!(header.state_root, B256::zero());
        assert_eq!(header.transactions_root, B256::zero());
        assert_eq!(header.gas_limit, GAS_LIMIT_PER_BLOCK);
        assert_eq!(header.gas_used, 0);
        assert_eq!(header.timestamp, config.genesis_timestamp);
        assert_eq!(header.number, 0);
    }

    #[test]
    fn genesis_header_with_state_root_only_changes_state_root() {
        let config = genesis_config();
        let base_header = config.genesis_header();
        let state_root = B256::from([0xab; 32]);
        let header = config.genesis_header_with_state_root(state_root);

        assert_eq!(header.state_root, state_root);
        assert_eq!(header.parent_hash, base_header.parent_hash);
        assert_eq!(header.beneficiary, base_header.beneficiary);
        assert_eq!(header.transactions_root, base_header.transactions_root);
        assert_eq!(header.gas_limit, base_header.gas_limit);
        assert_eq!(header.gas_used, base_header.gas_used);
        assert_eq!(header.timestamp, base_header.timestamp);
        assert_eq!(header.number, base_header.number);
    }
}
