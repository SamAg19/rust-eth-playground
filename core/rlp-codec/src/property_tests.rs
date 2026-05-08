use bytes::{Bytes, BytesMut};
use proptest::prelude::*;

use crate::{RlpItem, decode, encode};

fn arb_rlp_item() -> impl Strategy<Value = RlpItem> {
    prop::collection::vec(any::<u8>(), 0..=100)
        .prop_map(|bytes| RlpItem::Bytes(Bytes::from(bytes)))
        .prop_recursive(4, 64, 5, |inner| {
            prop::collection::vec(inner, 0..=5).prop_map(RlpItem::List)
        })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        ..ProptestConfig::default()
    })]

    #[test]
    fn arbitrary_rlp_items_roundtrip(item in arb_rlp_item()) {
        let mut buffer = BytesMut::new();
        encode(&item, &mut buffer)?;

        let bytes = buffer.freeze();
        let (decoded, remaining) = decode(&bytes)?;

        prop_assert_eq!(decoded, item);
        prop_assert!(remaining.is_empty());
    }
}
