use crate::encoder::encode;
use crate::decoder::decode;
use crate::error::RlpError;
use crate::item::RlpItem;
use bytes::{BytesMut, Bytes};
use crate::traits::{RlpDecodable, RlpEncodable};
use types::{AccessListItem, Address, B256, Transaction};

fn rlp_roundtrip(item: &RlpItem) -> Result<RlpItem, RlpError> {
    let mut buffer = BytesMut::new();
    encode(item, &mut buffer)?;
    let buffer_bytes = buffer.freeze();

    let (item_decoded, rem) = decode(&buffer_bytes)?;
    assert_eq!(rem.len(), 0);

    Ok(item_decoded)
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_roundtrip() {
        assert_eq!(rlp_roundtrip(&RlpItem::Bytes(Bytes::from(vec![]))).unwrap(), RlpItem::Bytes(Bytes::from(vec![])));
        assert_eq!(rlp_roundtrip(&RlpItem::Bytes(Bytes::from(vec![0x00]))).unwrap(), RlpItem::Bytes(Bytes::from(vec![0x00])));
        assert_eq!(rlp_roundtrip(&RlpItem::Bytes(Bytes::from(vec![0x7f]))).unwrap(), RlpItem::Bytes(Bytes::from(vec![0x7f])));
        assert_eq!(rlp_roundtrip(&RlpItem::Bytes(Bytes::from(vec![0x80]))).unwrap(), RlpItem::Bytes(Bytes::from(vec![0x80])));
        assert_eq!(rlp_roundtrip(&RlpItem::Bytes(Bytes::from(vec![0xff]))).unwrap(), RlpItem::Bytes(Bytes::from(vec![0xff])));
        assert_eq!(rlp_roundtrip(&RlpItem::Bytes(Bytes::from(vec![0x00; 55]))).unwrap(), RlpItem::Bytes(Bytes::from(vec![0x00; 55])));
        assert_eq!(rlp_roundtrip(&RlpItem::Bytes(Bytes::from(vec![0x00; 56]))).unwrap(), RlpItem::Bytes(Bytes::from(vec![0x00; 56])));
        
        assert_eq!(rlp_roundtrip(&RlpItem::List(vec![])).unwrap(), RlpItem::List(vec![]));
        assert_eq!(rlp_roundtrip(&RlpItem::List(vec![RlpItem::Bytes(Bytes::from(""))])).unwrap(), RlpItem::List(vec![RlpItem::Bytes(Bytes::from(""))]));
        assert_eq!(rlp_roundtrip(&RlpItem::List(vec![RlpItem::List(vec![])])).unwrap(), RlpItem::List(vec![RlpItem::List(vec![])]));

        let item_deeply_nested = RlpItem::List(vec![RlpItem::List(vec![RlpItem::List(vec![
            RlpItem::List(vec![RlpItem::List(vec![RlpItem::List(vec![])])]),
        ])])]);

        assert_eq!(rlp_roundtrip(&item_deeply_nested).unwrap(), item_deeply_nested);

        let item_mixed = RlpItem::List(vec![
            RlpItem::Bytes(Bytes::from("cat")),
            RlpItem::Bytes(Bytes::from(
                "This is a longer string that exceeds 55 bytes and should be encoded with a length prefix.",
            )),
            RlpItem::List(vec![
                RlpItem::Bytes(Bytes::from("dog")),
                RlpItem::Bytes(Bytes::from("fish")),
            ]),
            RlpItem::Bytes(Bytes::from("cow")),
        ]);

        assert_eq!(rlp_roundtrip(&item_mixed).unwrap(), item_mixed);
    }

    #[test]
    fn test_roundtrip_u64() {
        let zero = 0u64;
        assert_eq!(u64::from_rlp_item(&rlp_roundtrip(&zero.to_rlp_item()).unwrap()).unwrap(), zero);

        let one = 1u64;
        assert_eq!(u64::from_rlp_item(&rlp_roundtrip(&one.to_rlp_item()).unwrap()).unwrap(), one);

        let one_two_seven = 127u64;
        assert_eq!(u64::from_rlp_item(&rlp_roundtrip(&one_two_seven.to_rlp_item()).unwrap()).unwrap(), one_two_seven);

        let one_two_eight = 128u64;
        assert_eq!(u64::from_rlp_item(&rlp_roundtrip(&one_two_eight.to_rlp_item()).unwrap()).unwrap(), one_two_eight);

        let max = u64::MAX;
        assert_eq!(u64::from_rlp_item(&rlp_roundtrip(&max.to_rlp_item()).unwrap()).unwrap(), max);

        let max_u128 = u128::MAX;
        let err = u64::from_rlp_item(&rlp_roundtrip(&max_u128.to_rlp_item()).unwrap()).unwrap_err();
        assert!(matches!(err, RlpError::InvalidLength(16)));
    }

    #[test]
    fn test_roundtrip_u128() {
        let zero = 0u128;
        assert_eq!(u128::from_rlp_item(&rlp_roundtrip(&zero.to_rlp_item()).unwrap()).unwrap(), zero);

        let one = 1u128;
        assert_eq!(u128::from_rlp_item(&rlp_roundtrip(&one.to_rlp_item()).unwrap()).unwrap(), one);

        let one_two_seven = 127u128;
        assert_eq!(u128::from_rlp_item(&rlp_roundtrip(&one_two_seven.to_rlp_item()).unwrap()).unwrap(), one_two_seven);

        let max = u128::MAX;
        assert_eq!(u128::from_rlp_item(&rlp_roundtrip(&max.to_rlp_item()).unwrap()).unwrap(), max);
    }

    #[test]
    fn test_roundtrip_bool() {
        let f = false;
        assert_eq!(bool::from_rlp_item(&rlp_roundtrip(&f.to_rlp_item()).unwrap()).unwrap(), f);

        let t = true;
        assert_eq!(bool::from_rlp_item(&rlp_roundtrip(&t.to_rlp_item()).unwrap()).unwrap(), t);

        let max = u128::MAX;
        let err = bool::from_rlp_item(&rlp_roundtrip(&max.to_rlp_item()).unwrap()).unwrap_err();
        assert!(matches!(err, RlpError::InvalidLength(16)));
    }

    #[test]
    fn test_roundtrip_vecu8() {
        let v1: Vec<u8> = vec![123];
        assert_eq!(Vec::from_rlp_item(&rlp_roundtrip(&v1.to_rlp_item()).unwrap()).unwrap(), v1);

        let v2: Vec<u8> = vec![123, 1, 2, 3, 10];
        assert_eq!(Vec::from_rlp_item(&rlp_roundtrip(&v2.to_rlp_item()).unwrap()).unwrap(), v2);

        let err = Vec::from_rlp_item(&RlpItem::List(vec![])).unwrap_err();
        assert!(matches!(err, RlpError::UnexpectedType(0xc0)));
    }

    #[test]
    fn test_roundtrip_address() {
        let zero = Address::from([0x00;20]);
        assert_eq!(Address::from_rlp_item(&rlp_roundtrip(&zero.to_rlp_item()).unwrap()).unwrap(), zero);

        let arbitrary = Address::from([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x00, 0x11, 0x22, 0x33,     
  0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb]);
        assert_eq!(Address::from_rlp_item(&rlp_roundtrip(&arbitrary.to_rlp_item()).unwrap()).unwrap(), arbitrary);

        let max = Address::from([0xff;20]);
        assert_eq!(Address::from_rlp_item(&rlp_roundtrip(&max.to_rlp_item()).unwrap()).unwrap(), max);

        let item_too_short = RlpItem::Bytes(Bytes::from(vec![0x00; 19]));
        let err_too_short = Address::from_rlp_item(&item_too_short).unwrap_err();
        assert!(matches!(err_too_short, RlpError::InvalidLength(19)));

        let item_too_long = RlpItem::Bytes(Bytes::from(vec![0x00; 21]));
        let err_too_long = Address::from_rlp_item(&item_too_long).unwrap_err();
        assert!(matches!(err_too_long, RlpError::InvalidLength(21)));

        let err = Address::from_rlp_item(&RlpItem::List(vec![])).unwrap_err();
        assert!(matches!(err, RlpError::UnexpectedType(0xc0)));
    }

    #[test]
    fn test_roundtrip_b256() {
        let zero = B256::from([0x00;32]);
        assert_eq!(B256::from_rlp_item(&rlp_roundtrip(&zero.to_rlp_item()).unwrap()).unwrap(), zero);

        let arbitrary = B256::from([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x00, 0x11, 0x22, 0x33,     
  0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x00, 0x11, 0x22, 0x33]);
        assert_eq!(B256::from_rlp_item(&rlp_roundtrip(&arbitrary.to_rlp_item()).unwrap()).unwrap(), arbitrary);

        let max = B256::from([0xff;32]);
        assert_eq!(B256::from_rlp_item(&rlp_roundtrip(&max.to_rlp_item()).unwrap()).unwrap(), max);
        
        let item_too_short = RlpItem::Bytes(Bytes::from(vec![0x00; 31]));
        let err_too_short = B256::from_rlp_item(&item_too_short).unwrap_err();
        assert!(matches!(err_too_short, RlpError::InvalidLength(31)));

        let item_too_long = RlpItem::Bytes(Bytes::from(vec![0x00; 33]));
        let err_too_long = B256::from_rlp_item(&item_too_long).unwrap_err();
        assert!(matches!(err_too_long, RlpError::InvalidLength(33)));

        let err = B256::from_rlp_item(&RlpItem::List(vec![])).unwrap_err();
        assert!(matches!(err, RlpError::UnexpectedType(0xc0)));
    }

    fn roundtrip_tx(tx: &Transaction) -> Transaction {
        Transaction::from_rlp_item(&rlp_roundtrip(&tx.to_rlp_item()).unwrap()).unwrap()
    }

    fn assert_tx_eq(a: &Transaction, b: &Transaction) {
        match (a, b) {
            (
                Transaction::Legacy { nonce: n1, gas_price: gp1, gas_limit: gl1, to: t1, value: v1, data: d1 },
                Transaction::Legacy { nonce: n2, gas_price: gp2, gas_limit: gl2, to: t2, value: v2, data: d2 },
            ) => {
                assert_eq!(n1, n2);
                assert_eq!(gp1, gp2);
                assert_eq!(gl1, gl2);
                assert_eq!(t1, t2);
                assert_eq!(v1, v2);
                assert_eq!(d1, d2);
            }
            (
                Transaction::Eip1559 { nonce: n1, max_priority_fee_per_gas: mp1, max_fee_per_gas: mf1, gas_limit: gl1, to: t1, value: v1, data: d1, access_list: al1 },
                Transaction::Eip1559 { nonce: n2, max_priority_fee_per_gas: mp2, max_fee_per_gas: mf2, gas_limit: gl2, to: t2, value: v2, data: d2, access_list: al2 },
            ) => {
                assert_eq!(n1, n2);
                assert_eq!(mp1, mp2);
                assert_eq!(mf1, mf2);
                assert_eq!(gl1, gl2);
                assert_eq!(t1, t2);
                assert_eq!(v1, v2);
                assert_eq!(d1, d2);
                assert_eq!(al1.len(), al2.len());
                for (x, y) in al1.iter().zip(al2.iter()) {
                    assert_eq!(x.address, y.address);
                    assert_eq!(x.storage_keys, y.storage_keys);
                }
            }
            (
                Transaction::Eip4844 { nonce: n1, max_priority_fee_per_gas: mp1, max_fee_per_gas: mf1, max_fee_per_blob_gas: mb1, gas_limit: gl1, to: t1, value: v1, data: d1, access_list: al1, blob_versioned_hashes: bh1 },
                Transaction::Eip4844 { nonce: n2, max_priority_fee_per_gas: mp2, max_fee_per_gas: mf2, max_fee_per_blob_gas: mb2, gas_limit: gl2, to: t2, value: v2, data: d2, access_list: al2, blob_versioned_hashes: bh2 },
            ) => {
                assert_eq!(n1, n2);
                assert_eq!(mp1, mp2);
                assert_eq!(mf1, mf2);
                assert_eq!(mb1, mb2);
                assert_eq!(gl1, gl2);
                assert_eq!(t1, t2);
                assert_eq!(v1, v2);
                assert_eq!(d1, d2);
                assert_eq!(bh1, bh2);
                assert_eq!(al1.len(), al2.len());
                for (x, y) in al1.iter().zip(al2.iter()) {
                    assert_eq!(x.address, y.address);
                    assert_eq!(x.storage_keys, y.storage_keys);
                }
            }
            _ => panic!("variant mismatch"),
        }
    }

    #[test]
    fn test_roundtrip_access_list_item() {
        let item = AccessListItem {
            address: Address::from([0x11; 20]),
            storage_keys: vec![B256::from([0x22; 32]), B256::from([0x33; 32])],
        };
        let decoded = AccessListItem::from_rlp_item(&rlp_roundtrip(&item.to_rlp_item()).unwrap()).unwrap();
        assert_eq!(decoded.address, item.address);
        assert_eq!(decoded.storage_keys, item.storage_keys);

        let empty = AccessListItem {
            address: Address::from([0x00; 20]),
            storage_keys: vec![],
        };
        let decoded_empty = AccessListItem::from_rlp_item(&rlp_roundtrip(&empty.to_rlp_item()).unwrap()).unwrap();
        assert_eq!(decoded_empty.address, empty.address);
        assert_eq!(decoded_empty.storage_keys, empty.storage_keys);
    }

    #[test]
    fn test_roundtrip_transaction_legacy() {
        let tx = Transaction::Legacy {
            nonce: 42,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: Some(Address::from([0xaa; 20])),
            value: 1_000_000_000_000_000_000,
            data: vec![0xde, 0xad, 0xbe, 0xef],
        };
        assert_tx_eq(&roundtrip_tx(&tx), &tx);
    }

    #[test]
    fn test_roundtrip_transaction_eip1559() {
        let tx = Transaction::Eip1559 {
            nonce: 7,
            max_priority_fee_per_gas: 2_000_000_000,
            max_fee_per_gas: 50_000_000_000,
            gas_limit: 100_000,
            to: Some(Address::from([0xbb; 20])),
            value: 500_000,
            data: vec![0x01, 0x02, 0x03],
            access_list: vec![
                AccessListItem {
                    address: Address::from([0xcc; 20]),
                    storage_keys: vec![B256::from([0x01; 32]), B256::from([0x02; 32])],
                },
                AccessListItem {
                    address: Address::from([0xdd; 20]),
                    storage_keys: vec![],
                },
            ],
        };
        assert_tx_eq(&roundtrip_tx(&tx), &tx);
    }

    #[test]
    fn test_roundtrip_transaction_eip4844() {
        let tx = Transaction::Eip4844 {
            nonce: 99,
            max_priority_fee_per_gas: 3_000_000_000,
            max_fee_per_gas: 60_000_000_000,
            max_fee_per_blob_gas: 1_000_000,
            gas_limit: 250_000,
            to: Some(Address::from([0xee; 20])),
            value: 0,
            data: vec![0xff; 10],
            access_list: vec![AccessListItem {
                address: Address::from([0x12; 20]),
                storage_keys: vec![B256::from([0x34; 32])],
            }],
            blob_versioned_hashes: vec![
                B256::from([0xa1; 32]),
                B256::from([0xa2; 32]),
                B256::from([0xa3; 32]),
            ],
        };
        assert_tx_eq(&roundtrip_tx(&tx), &tx);
    }

    #[test]
    fn test_roundtrip_transaction_contract_creation() {
        let legacy = Transaction::Legacy {
            nonce: 0,
            gas_price: 100,
            gas_limit: 21_000,
            to: None,
            value: 0,
            data: vec![0x60, 0x80, 0x60, 0x40],
        };
        assert_tx_eq(&roundtrip_tx(&legacy), &legacy);

        let eip1559 = Transaction::Eip1559 {
            nonce: 1,
            max_priority_fee_per_gas: 1,
            max_fee_per_gas: 100,
            gas_limit: 21_000,
            to: None,
            value: 0,
            data: vec![0x60, 0x80],
            access_list: vec![],
        };
        assert_tx_eq(&roundtrip_tx(&eip1559), &eip1559);

        let eip4844 = Transaction::Eip4844 {
            nonce: 2,
            max_priority_fee_per_gas: 1,
            max_fee_per_gas: 100,
            max_fee_per_blob_gas: 50,
            gas_limit: 21_000,
            to: None,
            value: 0,
            data: vec![],
            access_list: vec![],
            blob_versioned_hashes: vec![B256::from([0xbb; 32])],
        };
        assert_tx_eq(&roundtrip_tx(&eip4844), &eip4844);
    }
}