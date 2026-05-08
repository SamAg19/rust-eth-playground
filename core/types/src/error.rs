use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Wrong length: expected {0}, found {1}")]
    WrongLength(usize, usize),
    #[error("Invalid hex character: {0} at position {1}")]
    InvalidHex(char, usize),
    #[error("Odd hex string length: {0}")]
    OddHexLength(usize),
    #[error("Bit out of range: {0}")]
    BitOutOfRange(usize),
}

fn check_hex_char(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

pub fn decode_hex(s: &str) -> Result<Vec<u8>, ParseError> {
    let mut bytes: Vec<u8> = Vec::new();

    if s.is_empty() {
        return Ok(bytes);
    }

    let length = s.len();
    if !length.is_multiple_of(2) {
        return Err(ParseError::OddHexLength(length));
    }

    let starts_with_0x = s.starts_with("0x") || s.starts_with("0X");
    let hex_str = if starts_with_0x { &s[2..] } else { s };

    for (i, chunk) in hex_str.as_bytes().chunks(2).enumerate() {
        let high = chunk[0];
        let high_index = if starts_with_0x { i * 2 + 2 } else { i * 2 };
        let low = chunk[1];
        let low_index = high_index + 1;

        let high_val = match check_hex_char(high) {
            Some(val) => val,
            None => return Err(ParseError::InvalidHex(high as char, high_index)),
        };
        let low_val = match check_hex_char(low) {
            Some(val) => val,
            None => return Err(ParseError::InvalidHex(low as char, low_index)),
        };

        bytes.push(high_val << 4 | low_val);
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_hex_empty() {
        let result = decode_hex("").unwrap();
        assert_eq!(result, vec![]);

        let result_with_0x = decode_hex("0x").unwrap();
        assert_eq!(result_with_0x, vec![]);
    }

    #[test]
    fn test_decode_hex_valid_with_0x() {
        // lowercase testing
        let result_lower = decode_hex("0x48656c6c6f").unwrap();
        assert_eq!(result_lower, b"Hello");

        // uppercase testing
        let result_upper = decode_hex("0X48656C6C6F").unwrap();
        assert_eq!(result_upper, b"Hello");

        //mixed case testing
        let result_mixed = decode_hex("0x48656c6C6F").unwrap();
        assert_eq!(result_mixed, b"Hello");
    }

    #[test]
    fn test_decode_hex_valid_without_0x() {
        let result = decode_hex("48656c6c6f").unwrap();
        assert_eq!(result, b"Hello");
    }

    #[test]
    fn test_decode_hex_odd_length() {
        let result = decode_hex("0x123").unwrap_err();
        assert!(matches!(result, ParseError::OddHexLength(5)));
    }

    #[test]
    fn test_decode_hex_invalid_char_high_nibble() {
        let result_with_0x = decode_hex("0x12G4").unwrap_err();
        assert!(matches!(result_with_0x, ParseError::InvalidHex('G', 4)));

        let result_without_0x = decode_hex("12G4").unwrap_err();
        assert!(matches!(result_without_0x, ParseError::InvalidHex('G', 2)));
    }

    #[test]
    fn test_decode_hex_invalid_char_low_nibble() {
        let result_with_0x = decode_hex("0x123G").unwrap_err();
        assert!(matches!(result_with_0x, ParseError::InvalidHex('G', 5)));

        let result_without_0x = decode_hex("123G").unwrap_err();
        assert!(matches!(result_without_0x, ParseError::InvalidHex('G', 3)));
    }

    #[test]
    fn test_order_of_checks() {
        // Odd length should be checked before invalid characters
        let result = decode_hex("0x1234G").unwrap_err();
        assert!(matches!(result, ParseError::OddHexLength(7)));
    }
}
