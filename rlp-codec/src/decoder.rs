use crate::item::RlpItem;
use crate::error::RlpError;
use bytes::Bytes;

pub fn decode(input: &[u8]) -> Result<(RlpItem, &[u8]), RlpError> {
    if input.is_empty() {
        return Err(RlpError::InputTooShort { expected: 1, actual: 0 });
    }

    if input[0] < 0x80 {
        Ok((RlpItem::Bytes(Bytes::from(vec![input[0]])), &input[1..]))
    }
    else if input[0] < 0xb8 {
        let len = (input[0] - 0x80) as usize;
        if len > input[1..].len() {
            return Err(RlpError::InputTooShort { expected: len, actual: input.len() - 1});
        }

        Ok((RlpItem::Bytes(Bytes::copy_from_slice(&input[1..1+len])), &input[1+len..]))
    }
    else if input[0] < 0xc0 {
        let len_of_len = (input[0] - 0xb7) as usize;
        let payload_len = decode_length(&input[1..], len_of_len)?;
        let remaining_after_len = &input[1 + len_of_len..];
        if payload_len > remaining_after_len.len() {
            return Err(RlpError::InputTooShort { expected: payload_len, actual: remaining_after_len.len() });
        }

        Ok((RlpItem::Bytes(Bytes::copy_from_slice(&remaining_after_len[..payload_len])), &remaining_after_len[payload_len..]))        
    }
    else if input[0] < 0xf8 {
        let payload_len = (input[0] - 0xc0) as usize;
        if payload_len > input.len() - 1 {
            return Err(RlpError::InputTooShort { expected: payload_len, actual: input.len() - 1});
        }
        let mut payload_slice = &input[1..payload_len+1];

        let mut decoded: Vec<RlpItem> = vec![];

        while !payload_slice.is_empty() {
            let (item, remaining) = decode(payload_slice)?;
            decoded.push(item);
            payload_slice = remaining;
        }

        Ok((RlpItem::List(decoded), &input[1+payload_len..]))
        
    }
    else {
        let len_of_len = (input[0] - 0xf7) as usize;
        let payload_len = decode_length(&input[1..], len_of_len)?;
        let remaining_after_len = &input[1 + len_of_len..];
        if payload_len > remaining_after_len.len() {
            return Err(RlpError::InputTooShort { expected: payload_len, actual: remaining_after_len.len()});
        }
        let mut payload_slice = &remaining_after_len[..payload_len]; 

        let mut decoded: Vec<RlpItem> = vec![];

        while !payload_slice.is_empty() {
            let (item, remaining) = decode(payload_slice)?;
            decoded.push(item);
            payload_slice = remaining;
        }

        Ok((RlpItem::List(decoded), &remaining_after_len[payload_len..]))
    }
}

fn decode_length(slice: &[u8], len_of_len: usize) -> Result<usize, RlpError> {
    if len_of_len == 0 {
        return Err(RlpError::InvalidLength(0));
    }

    if len_of_len > slice.len() {
        return Err(RlpError::InputTooShort { expected: len_of_len, actual: slice.len() });
    }

    let mut arr: [u8; 8] = [0x00; 8];
    arr[8-len_of_len..].copy_from_slice(&slice[..len_of_len]); 

    Ok(usize::from_be_bytes(arr))
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::error::RlpError;

    #[test]
    fn test_decode_single_byte() {
        let input_00 = [0x00];
        let input_7f = [0x7f];
        let input_mul = [0x01, 0x02];

        let (item_00, rem_00) = decode(&input_00).unwrap();
        assert_eq!(item_00, RlpItem::Bytes(Bytes::from(vec![0x00])));
        assert_eq!(rem_00.len(), 0);

        let (item_7f, rem_7f) = decode(&input_7f).unwrap();
        assert_eq!(item_7f, RlpItem::Bytes(Bytes::from(vec![0x7f])));
        assert_eq!(rem_7f.len(), 0);

        let (item_mul, rem_mul) = decode(&input_mul).unwrap();
        assert_eq!(item_mul, RlpItem::Bytes(Bytes::from(vec![0x01])));
        assert_eq!(rem_mul.len(), 1);
        assert_eq!(rem_mul[0], 0x02);
    }

    #[test]
    fn test_decode_short_string() {
        let input_empty = [0x80];
        let input_dog = [0x83, 0x64, 0x6f, 0x67];
        let input_80 = [0x81, 0x80];
        let input_err  = [0x82, 0x01];

        let (item_empty, rem_empty) = decode(&input_empty).unwrap();
        assert_eq!(item_empty, RlpItem::Bytes(Bytes::from(vec![])));
        assert_eq!(rem_empty.len(), 0);

        let (item_dog, rem_dog) = decode(&input_dog).unwrap();
        assert_eq!(item_dog, RlpItem::Bytes(Bytes::from(b"dog".to_vec())));
        assert_eq!(rem_dog.len(), 0);

        let (item_80, rem_80) = decode(&input_80).unwrap();
        assert_eq!(item_80, RlpItem::Bytes(Bytes::from(vec![0x80])));
        assert_eq!(rem_80.len(), 0);

        let err = decode(&input_err).unwrap_err();
        assert!(matches!(err, RlpError::InputTooShort{ expected: 2, actual: 1}));

        let input = [0x81, 0xAA, 0xBB];         
        let (item, rem) = decode(&input).unwrap();                                                                                          
        assert_eq!(item, RlpItem::Bytes(Bytes::from(vec![0xAA])));                                                                          
        assert_eq!(rem, &[0xBB]);
    }

    #[test]
    fn test_decode_long_string() {
        let mut input_56 = vec![0xb7 + 1, 0x38]; // 56 in hex is 0x38
        input_56.extend(vec![0x00; 56]);

        let mut input_1024 = vec![0xb7 + 2, 0x04, 0x00]; // 1024 in hex is 0x0400
        input_1024.extend(vec![0x00; 1024]);

        let mut input_err = vec![0xb7 + 1, 0x38]; // 56 in hex is 0x38
        input_err.extend(vec![0x00; 50]);

        let (item_56, rem_56) = decode(&input_56).unwrap();
        assert_eq!(item_56, RlpItem::Bytes(Bytes::from(vec![0x00; 56])));
        assert_eq!(rem_56.len(), 0);

        let (item_1024, rem_1024) = decode(&input_1024).unwrap();
        assert_eq!(item_1024, RlpItem::Bytes(Bytes::from(vec![0x00; 1024])));
        assert_eq!(rem_1024.len(), 0);

        let input_too_short_err = decode(&input_err).unwrap_err();
        assert!(matches!(input_too_short_err, RlpError::InputTooShort{ expected: 56, actual: 50}));

        let invalid_length = decode_length(&[0x01, 0x02], 0).unwrap_err();                                                                  
        assert!(matches!(invalid_length, RlpError::InvalidLength(0)));  
    }

    #[test]
    fn test_decode_short_list() {
        let input_empty_list = [0xc0];
        let input_list_empty_bytes = [0xc1, 0x80];
        let input_list_empty_list = [0xc1, 0xc0];
        let input_list_animals = [0xc8, 0x83, 0x63, 0x61, 0x74, 0x83, 0x64, 0x6f, 0x67];

        let (item_empty_list, rem_empty_list) = decode(&input_empty_list).unwrap();
        assert_eq!(item_empty_list, RlpItem::List(vec![]));
        assert_eq!(rem_empty_list.len(), 0);

        let (item_list_empty_bytes, rem_list_empty_bytes) = decode(&input_list_empty_bytes).unwrap();
        assert_eq!(item_list_empty_bytes, RlpItem::List(vec![RlpItem::Bytes(Bytes::from(vec![]))]));
        assert_eq!(rem_list_empty_bytes.len(), 0);

        let (item_list_empty_list, rem_list_empty_list) = decode(&input_list_empty_list).unwrap();
        assert_eq!(item_list_empty_list, RlpItem::List(vec![RlpItem::List(vec![])]));
        assert_eq!(rem_list_empty_list.len(), 0);

        let (item_list_animals, rem_list_animals) = decode(&input_list_animals).unwrap();
        let item_strings_list = RlpItem::List(vec![
            RlpItem::Bytes(Bytes::from("cat")),
            RlpItem::Bytes(Bytes::from("dog")),
        ]);
        assert_eq!(item_list_animals, item_strings_list);
        assert_eq!(rem_list_animals.len(), 0);
    }

    #[test]
    fn test_decode_long_list() {
        let input_mixed_list = [
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
            ];

        let (item_mixed_list, rem_mixed_list) = decode(&input_mixed_list).unwrap();
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
        assert_eq!(item_mixed_list, item_mixed);
        assert_eq!(rem_mixed_list.len(), 0);

        let input_nested = [
                0xd2, 0xd1, 0xd0, 0xcf, 0xce, 0x8d, 0x64, 0x65, 0x65, 0x70, 0x6c, 0x79, 0x20, 0x6e,
                0x65, 0x73, 0x74, 0x65, 0x64, 0x01
            ];
        let (item_nested, rem_nested) = decode(&input_nested).unwrap();
        let expected_item_deeply_nested = RlpItem::List(vec![RlpItem::List(vec![RlpItem::List(vec![
            RlpItem::List(vec![RlpItem::List(vec![RlpItem::Bytes(Bytes::from(
                "deeply nested",
            ))])]),
        ])])]);
        assert_eq!(item_nested, expected_item_deeply_nested);
        assert_eq!(rem_nested.len(), 1);
    }
}