use crate::error::RlpError;
use crate::item::RlpItem;
use bytes::{BufMut, BytesMut};

pub fn encoded_len(item: &RlpItem) -> usize {
    match item {
        RlpItem::Bytes(data) => {
            if data.len() == 1 && data[0] < 0x80 {
                1
            } else {
                length_prefix_len(data.len()) + data.len()
            }
        }
        RlpItem::List(items) => {
            let payload_len = items.iter().map(encoded_len).sum::<usize>();
            list_prefix_len(payload_len) + payload_len
        }
    }
}

pub fn encode(item: &RlpItem, buffer: &mut BytesMut) -> Result<(), RlpError> {
    match item {
        RlpItem::Bytes(data) => {
            if data.len() == 1 && data[0] < 0x80 {
                buffer.put_slice(data);
                Ok(())
            } else {
                if data.len() <= 55 {
                    buffer.put_u8(0x80 + data.len() as u8)
                } else {
                    let length_bytes = encode_length(data.len());
                    buffer.put_u8(0xb7 + length_bytes.len() as u8);
                    buffer.put_slice(&length_bytes);
                }

                buffer.put_slice(data);
                Ok(())
            }
        }
        RlpItem::List(items) => {
            let payload_len = items.iter().map(encoded_len).sum();
            let mut encoded_items = BytesMut::with_capacity(payload_len);

            for item in items {
                encode(item, &mut encoded_items)?;
            }

            add_rlp_list_prefix(buffer, encoded_items.len());

            buffer.put_slice(&encoded_items);
            Ok(())
        }
    }
}

pub fn add_rlp_list_prefix(buffer: &mut BytesMut, encoded_items_len: usize) {
    if encoded_items_len <= 55 {
        buffer.put_u8(0xc0 + encoded_items_len as u8);
    } else {
        let length_bytes = encode_length(encoded_items_len);
        buffer.put_u8(0xf7 + length_bytes.len() as u8);
        buffer.put_slice(&length_bytes);
    }
}

// integer to big endian bytes
fn encode_length(value: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut val = value;
    loop {
        bytes.push((val & 0xff) as u8);
        if val <= 0xff {
            break;
        }
        val >>= 8;
    }
    bytes.reverse();
    bytes
}

fn length_prefix_len(payload_len: usize) -> usize {
    if payload_len <= 55 {
        1
    } else {
        1 + encoded_length_len(payload_len)
    }
}

fn list_prefix_len(payload_len: usize) -> usize {
    length_prefix_len(payload_len)
}

fn encoded_length_len(value: usize) -> usize {
    let bits = usize::BITS - value.leading_zeros();
    bits.div_ceil(8) as usize
}

#[cfg(test)]
mod tests {

    use super::*;
    use bytes::Bytes;

    fn encode_to_bytes(item: &RlpItem) -> Bytes {
        let mut buffer = BytesMut::with_capacity(encoded_len(item));
        encode(item, &mut buffer).unwrap();
        buffer.freeze()
    }

    #[test]
    fn encoded_len_matches_encoded_output_len() {
        let item = RlpItem::List(vec![
            RlpItem::Bytes(Bytes::from("cat")),
            RlpItem::Bytes(Bytes::from(vec![0x00; 56])),
            RlpItem::List(vec![RlpItem::Bytes(Bytes::from("dog"))]),
        ]);

        assert_eq!(encoded_len(&item), encode_to_bytes(&item).len());
    }

    #[test]
    fn test_encode_single_byte_below_0x80() {
        assert_eq!(
            encode_to_bytes(&RlpItem::Bytes(Bytes::from(vec![0x00]))),
            Bytes::from(vec![0x00])
        );
        assert_eq!(
            encode_to_bytes(&RlpItem::Bytes(Bytes::from(vec![0x01]))),
            Bytes::from(vec![0x01])
        );
        assert_eq!(
            encode_to_bytes(&RlpItem::Bytes(Bytes::from(vec![0x7f]))),
            Bytes::from(vec![0x7f])
        );
    }

    #[test]
    fn test_encode_short_string() {
        assert_eq!(
            encode_to_bytes(&RlpItem::Bytes(Bytes::from(vec![]))),
            Bytes::from(vec![0x80])
        );
        assert_eq!(
            encode_to_bytes(&RlpItem::Bytes(Bytes::from(vec![0x80]))),
            Bytes::from(vec![0x81, 0x80])
        );
        assert_eq!(
            encode_to_bytes(&RlpItem::Bytes(Bytes::from(vec![0x80, 0x80]))),
            Bytes::from(vec![0x82, 0x80, 0x80])
        );
        assert_eq!(
            encode_to_bytes(&RlpItem::Bytes(Bytes::from("dog"))),
            Bytes::from(vec![0x83, 0x64, 0x6f, 0x67])
        );

        let mut expected_max_55 = vec![0x80 + 55];
        expected_max_55.extend(vec![0x00; 55]);
        assert_eq!(
            encode_to_bytes(&RlpItem::Bytes(Bytes::from(vec![0x00; 55]))),
            Bytes::from(expected_max_55)
        );
    }

    #[test]
    fn test_encode_length() {
        assert_eq!(encode_length(0), vec![0x00]);
        assert_eq!(encode_length(1), vec![0x01]);
        assert_eq!(encode_length(127), vec![0x7f]);
        assert_eq!(encode_length(128), vec![0x80]);
        assert_eq!(encode_length(255), vec![0xff]);
        assert_eq!(encode_length(256), vec![0x01, 0x00]);
        assert_eq!(encode_length(65535), vec![0xff, 0xff]);
        assert_eq!(encode_length(65536), vec![0x01, 0x00, 0x00]);
    }

    #[test]
    fn test_encode_greater_than_55() {
        let mut expected_56 = vec![0xb7 + 1, 0x38]; // 56 in hex is 0x38
        expected_56.extend(vec![0x00; 56]);
        assert_eq!(
            encode_to_bytes(&RlpItem::Bytes(Bytes::from(vec![0x00; 56]))),
            Bytes::from(expected_56)
        );

        let mut expected_1024 = vec![0xb7 + 2, 0x04, 0x00]; // 1024 in hex is 0x0400
        expected_1024.extend(vec![0x00; 1024]);
        assert_eq!(
            encode_to_bytes(&RlpItem::Bytes(Bytes::from(vec![0x00; 1024]))),
            Bytes::from(expected_1024)
        );
    }

    #[test]
    fn test_encode_list() {
        assert_eq!(
            encode_to_bytes(&RlpItem::List(vec![])),
            Bytes::from(vec![0xc0])
        );

        assert_eq!(
            encode_to_bytes(&RlpItem::List(vec![RlpItem::Bytes(Bytes::from(vec![]))])),
            Bytes::from(vec![0xc1, 0x80])
        );

        let item_strings_list = RlpItem::List(vec![
            RlpItem::Bytes(Bytes::from("cat")),
            RlpItem::Bytes(Bytes::from("dog")),
        ]);
        assert_eq!(
            encode_to_bytes(&item_strings_list),
            Bytes::from(vec![0xc8, 0x83, 0x63, 0x61, 0x74, 0x83, 0x64, 0x6f, 0x67])
        );

        assert_eq!(
            encode_to_bytes(&RlpItem::List(vec![RlpItem::List(vec![])])),
            Bytes::from(vec![0xc1, 0xc0])
        );
    }

    #[test]
    fn test_encode_list_greater_than_55() {
        let item_56_empty_lists = RlpItem::List(vec![RlpItem::List(vec![]); 56]);
        let mut expected_56_empty_lists = vec![0xf7 + 1, 0x38]; // 56 in hex is 0x38
        expected_56_empty_lists.extend(vec![0xc0; 56]);
        assert_eq!(
            encode_to_bytes(&item_56_empty_lists),
            Bytes::from(expected_56_empty_lists)
        );

        // A deeply nested list: five levels of nesting with an empty list at the centre — verify the output is correct by decoding it by hand from the outside in
        let item_deeply_nested = RlpItem::List(vec![RlpItem::List(vec![RlpItem::List(vec![
            RlpItem::List(vec![RlpItem::List(vec![])]),
        ])])]);
        assert_eq!(
            encode_to_bytes(&item_deeply_nested),
            Bytes::from(vec![0xc4, 0xc3, 0xc2, 0xc1, 0xc0])
        );
    }

    #[test]
    fn test_encode_mixed_list() {
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

        assert_eq!(
            encode_to_bytes(&item_mixed),
            Bytes::from(vec![
                0xf8,
                0x6d, // long-list prefix, payload is 109 bytes
                0x83,
                0x63,
                0x61,
                0x74, // "cat"
                0xb7 + 1,
                0x59, // length prefix for long string (89 bytes)
                0x54,
                0x68,
                0x69,
                0x73,
                0x20,
                0x69,
                0x73,
                0x20,
                0x61,
                0x20,
                0x6c,
                0x6f,
                0x6e,
                0x67,
                0x65,
                0x72,
                0x20,
                0x73,
                0x74,
                0x72,
                0x69,
                0x6e,
                0x67,
                0x20,
                0x74,
                0x68,
                0x61,
                0x74,
                0x20,
                0x65,
                0x78,
                0x63,
                0x65,
                0x65,
                0x64,
                0x73,
                0x20,
                0x35,
                0x35,
                0x20,
                0x62,
                0x79,
                0x74,
                0x65,
                0x73,
                0x20,
                0x61,
                0x6e,
                0x64,
                0x20,
                0x73,
                0x68,
                0x6f,
                0x75,
                0x6c,
                0x64,
                0x20,
                0x62,
                0x65,
                0x20,
                0x65,
                0x6e,
                0x63,
                0x6f,
                0x64,
                0x65,
                0x64,
                0x20,
                0x77,
                0x69,
                0x74,
                0x68,
                0x20,
                0x61,
                0x20,
                0x6c,
                0x65,
                0x6e,
                0x67,
                0x74,
                0x68,
                0x20,
                0x70,
                0x72,
                0x65,
                0x66,
                0x69,
                0x78,
                0x2e,
                0xc9, // inner list prefix (payload is 9 bytes)
                0x83,
                0x64,
                0x6f,
                0x67, // "dog"
                0x84,
                0x66,
                0x69,
                0x73,
                0x68, // "fish"
                0x83,
                0x63,
                0x6f,
                0x77 // "cow"
            ])
        );
    }

    #[test]
    fn test_encode_list_crossing_55_bytes() {
        let item_long_list = RlpItem::List(vec![RlpItem::Bytes(Bytes::from(vec![0x00; 56]))]);
        let mut expected_long_list = vec![0xf7 + 1, 0x3a]; // 56 in hex is 0x3a
        expected_long_list.push(0xb7 + 1); // length prefix for long string (56 bytes)
        expected_long_list.push(0x38); // 56 in hex is 0x38
        expected_long_list.extend(vec![0x00; 56]);
        assert_eq!(
            encode_to_bytes(&item_long_list),
            Bytes::from(expected_long_list)
        );
    }

    // Encode a deeply nested structure — at least four levels. Verify correctness.
    #[test]
    fn test_encode_deeply_nested_structure() {
        let item_deeply_nested = RlpItem::List(vec![RlpItem::List(vec![RlpItem::List(vec![
            RlpItem::List(vec![RlpItem::List(vec![RlpItem::Bytes(Bytes::from(
                "deeply nested",
            ))])]),
        ])])]);
        assert_eq!(
            encode_to_bytes(&item_deeply_nested),
            Bytes::from(vec![
                0xd2, 0xd1, 0xd0, 0xcf, 0xce, 0x8d, 0x64, 0x65, 0x65, 0x70, 0x6c, 0x79, 0x20, 0x6e,
                0x65, 0x73, 0x74, 0x65, 0x64
            ])
        );
    }

    // Encode the empty string, the empty list, and a list containing the empty string, all in one test. These three are the most commonly confused edge cases in RLP.
    #[test]
    fn test_encode_edge_cases() {
        let item_edge_cases = RlpItem::List(vec![
            RlpItem::Bytes(Bytes::from(vec![])), // empty string
            RlpItem::List(vec![]),               // empty list
            RlpItem::List(vec![RlpItem::Bytes(Bytes::from(vec![]))]), // list containing the empty string
        ]);
        assert_eq!(
            encode_to_bytes(&item_edge_cases),
            Bytes::from(vec![0xc4, 0x80, 0xc0, 0xc1, 0x80])
        );
    }
}
