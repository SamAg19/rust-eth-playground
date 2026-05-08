use bytes::{Bytes, BytesMut};
use rlp_codec::{RlpEncodable, RlpItem, encode};
use types::{Address, B256, Bloom, Transaction};

use crate::primitives::{Block, Header, Log, Receipt};

fn address(byte: u8) -> Address {
    Address::new([byte; 20])
}

fn hash(byte: u8) -> B256 {
    B256::new([byte; 32])
}

fn header() -> Header {
    Header {
        block_number: 42,
        parent_hash: hash(0x11),
        state_root: hash(0x22),
        transactions_root: hash(0x33),
        receipts_root: hash(0x44),
        logs_bloom: Bloom::zero(),
        gas_limit: 30_000_000,
        gas_used: 42_000,
        base_fee_per_gas: 1_000_000_000,
        hash: hash(0xaa),
    }
}

fn transaction() -> Transaction {
    Transaction::Legacy {
        nonce: 7,
        gas_price: 1_000_000_000,
        gas_limit: 21_000,
        to: Some(address(0x22)),
        value: 12_345,
        data: vec![0xde, 0xad, 0xbe, 0xef],
    }
}

fn receipt() -> Receipt {
    Receipt {
        transaction_hash: hash(0x55),
        transaction_index: 2,
        block_hash: hash(0xaa),
        block_number: 42,
        from: address(0x11),
        to: Some(address(0x22)),
        contract_address: Some(address(0x33)),
        cumulative_gas_used: 42_000,
        effective_gas_price: 1_000_000_000,
        gas_used: 21_000,
        status: true,
        logs: vec![Log {
            address: address(0x44),
            topics: vec![hash(0x66), hash(0x77)],
            data: vec![1, 2, 3, 4],
        }],
        logs_bloom: Bloom::zero(),
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn encode_item_to_hex(item: &RlpItem) -> String {
    let mut buffer = BytesMut::new();
    encode(item, &mut buffer).unwrap();
    hex(&buffer.freeze())
}

fn header_rlp_item(header: &Header) -> RlpItem {
    RlpItem::List(vec![
        header.block_number.to_rlp_item(),
        header.parent_hash.to_rlp_item(),
        header.state_root.to_rlp_item(),
        header.transactions_root.to_rlp_item(),
        header.receipts_root.to_rlp_item(),
        RlpItem::Bytes(Bytes::copy_from_slice(header.logs_bloom.as_ref())),
        header.gas_limit.to_rlp_item(),
        header.gas_used.to_rlp_item(),
        header.base_fee_per_gas.to_rlp_item(),
        header.hash.to_rlp_item(),
    ])
}

#[test]
fn snapshots_header_json() {
    insta::assert_json_snapshot!("header_json", header());
}

#[test]
fn snapshots_block_json() {
    insta::assert_json_snapshot!(
        "block_json",
        Block {
            header: header(),
            transactions: vec![],
        }
    );
}

#[test]
fn snapshots_transaction_json() {
    insta::assert_json_snapshot!("transaction_json", transaction());
}

#[test]
fn snapshots_receipt_json() {
    insta::assert_json_snapshot!("receipt_json", receipt());
}

#[test]
fn snapshots_header_rlp_hex() {
    insta::assert_snapshot!(
        "header_rlp_hex",
        encode_item_to_hex(&header_rlp_item(&header()))
    );
}

#[test]
fn snapshots_transaction_rlp_hex() {
    insta::assert_snapshot!(
        "transaction_rlp_hex",
        encode_item_to_hex(&transaction().to_rlp_item())
    );
}
