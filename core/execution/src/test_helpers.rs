use rlp_codec::signing::{SignedTransaction, recover_sender, sign};
use types::{Address, B256, Transaction};

use crate::{
    executor::BlockWithSenders,
    primitives::{Block, Header},
};

pub const TEST_PRIVATE_KEY: [u8; 32] = [
    0x4c, 0x0c, 0x4d, 0x14, 0x6c, 0x46, 0xed, 0x91, 0xf6, 0xa9, 0x35, 0x09, 0x46, 0xa3, 0x69, 0x9e,
    0xb1, 0xfb, 0xc1, 0x9c, 0x91, 0xc8, 0x10, 0xe6, 0xb6, 0xa7, 0x8b, 0x0c, 0xa6, 0x06, 0x65, 0x6f,
];

pub const TEST_CHAIN_ID: u64 = 1;

// Signature placeholder for tests that exercise the provider layer (storage,
// indexing, caching) but don't care about ECDSA validity. Uses zero v/r/s so
// no key material leaks into tests that aren't about signing.
pub fn dummy_signed(tx: Transaction) -> SignedTransaction {
    SignedTransaction {
        transaction: tx,
        v: 0,
        r: B256::default(),
        s: B256::default(),
    }
}

pub fn signed_legacy_tx(
    nonce: u64,
    value: u128,
    gas_limit: u64,
    gas_price: u128,
    to: Option<Address>,
) -> SignedTransaction {
    let tx = Transaction::Legacy {
        nonce,
        gas_price,
        gas_limit,
        to,
        value,
        data: vec![],
    };
    sign(&tx, &TEST_PRIVATE_KEY, TEST_CHAIN_ID).unwrap()
}

pub fn block_with_senders(header: Header, txs: Vec<SignedTransaction>) -> BlockWithSenders {
    let senders: Vec<Address> = txs
        .iter()
        .map(|tx| recover_sender(tx, TEST_CHAIN_ID).unwrap())
        .collect();
    BlockWithSenders {
        block: Block {
            header,
            transactions: txs,
        },
        senders,
    }
}

pub fn test_sender() -> Address {
    // Round-trip the test key through sign/recover once to get the canonical
    // address, instead of duplicating the public-key-to-address derivation.
    let dummy = signed_legacy_tx(0, 0, 21_000, 1_000_000_000, None);
    recover_sender(&dummy, TEST_CHAIN_ID).unwrap()
}
