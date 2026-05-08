use crate::error;
use std::fmt::{self, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::BitOrAssign;
use std::str::FromStr;

byte_array_newtype!(Bloom, 256, no_copy);

impl Bloom {
    pub fn set_bit(&mut self, bit: usize) -> Result<(), error::ParseError> {
        if bit >= 2048 {
            return Err(error::ParseError::BitOutOfRange(bit));
        }

        let byte_index = bit / 8;
        let bit_index = bit % 8;
        self.0[byte_index] |= 1 << bit_index;
        Ok(())
    }

    pub fn has_bit(&self, bit: usize) -> Result<bool, error::ParseError> {
        if bit >= 2048 {
            return Err(error::ParseError::BitOutOfRange(bit));
        }

        let byte_index = bit / 8;
        let bit_index = bit % 8;
        Ok((self.0[byte_index] & (1 << bit_index)) != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const TEST_BYTES: [u8; 256] = [0xab; 256];

    // `Bloom::default()` produces 256 zero bytes; its `Display` output is exactly
    // 514 characters (2 for "0x" + 2*256 hex chars) and starts with `0x`.
    #[test]
    fn test_default() {
        let bloom = Bloom::zero();
        assert_eq!(bloom.as_bytes(), &[0; 256]);
        let bloom_str = format!("{}", bloom);
        assert_eq!(bloom_str.len(), 514);
        assert_eq!(&bloom_str[..2], "0x");
    }

    // `From<[u8; 256]>` round-trips correctly through `AsRef`
    #[test]
    fn test_from_array_round_trip() {
        let bloom = Bloom::new([1; 256]);
        let bytes: &[u8] = bloom.as_ref();
        assert_eq!(bytes, &[1; 256]);
    }

    // Two Blooms with identical bytes are `==`; two with different bytes are `!=`
    #[test]
    fn test_equality() {
        let bloom_1 = Bloom::from([1; 256]);
        let bloom_2 = Bloom::from([1; 256]);
        let bloom_3 = Bloom::from([2; 256]);
        assert_eq!(bloom_1, bloom_2);
        assert_ne!(bloom_1, bloom_3);
    }

    // Two equal Blooms inserted into a `HashSet` result in a set of length 1.
    #[test]
    fn test_same_bloom_hashset() {
        let mut set: HashSet<Bloom> = HashSet::new();
        let bloom_1 = Bloom::from([1; 256]);
        let bloom_2 = Bloom::from([1; 256]);
        set.insert(bloom_1);
        set.insert(bloom_2);
        assert_eq!(set.len(), 1);
    }

    // Move semantics: Bloom is NOT Copy. Assigning to a second variable moves it,
    // and using the original after the move is a compile error. The lines below
    // are commented out because they would fail to compile — the intent is
    // documented here rather than enforced by a passing test.
    //
    //     let bloom_1 = Bloom::from([1; 256]);
    //     let bloom_2 = bloom_1;           // `bloom_1` is moved into `bloom_2`
    //     let _ = bloom_1.as_bytes();      // ERROR: use of moved value `bloom_1`
    //     let _ = bloom_2;

    // Contrast with Address, which IS Copy. Assigning to a second variable copies
    // it implicitly, so the original remains usable. This test compiles and passes
    // — the point is the asymmetry with Bloom's move-only semantics above.
    #[test]
    fn test_address_copy_contrast() {
        use crate::address::Address;
        let addr_1 = Address::from([1; 20]);
        let addr_2 = addr_1; // This is a COPY, not a move, because Address is Copy.
        assert_eq!(addr_1, addr_2); // `addr_1` is still valid here — no compile error.
    }

    // `Display` output for a known byte sequence matches the expected string.
    #[test]
    fn test_display() {
        let bloom = Bloom::from(TEST_BYTES);
        let expected = format!("0x{}", "ab".repeat(256));
        assert_eq!(format!("{}", bloom), expected);
    }

    // `Debug` output wraps the `Display` output in a `Bloom(...)` label.
    #[test]
    fn test_debug() {
        let bloom = Bloom::from(TEST_BYTES);
        let expected = format!("Bloom(0x{})", "ab".repeat(256));
        assert_eq!(format!("{:?}", bloom), expected);
    }

    // `format!("{:x}", bloom)` and `format!("{:#x}", bloom)` produce the expected
    // outputs for `LowerHex` (no prefix by default, `0x` prefix with alternate).
    #[test]
    fn test_lower_hex() {
        let bloom = Bloom::from(TEST_BYTES);
        let no_prefix = "ab".repeat(256);
        let with_prefix = format!("0x{}", no_prefix);
        assert_eq!(format!("{:x}", bloom), no_prefix);
        assert_eq!(format!("{:#x}", bloom), with_prefix);
    }

    // `format!("{:X}", bloom)` and `format!("{:#X}", bloom)` produce the expected
    // outputs for `UpperHex` (uppercase hex digits, lowercase `0x` prefix).
    #[test]
    fn test_upper_hex() {
        let bloom = Bloom::from(TEST_BYTES);
        let no_prefix = "AB".repeat(256);
        let with_prefix = format!("0x{}", no_prefix);
        assert_eq!(format!("{:X}", bloom), no_prefix);
        assert_eq!(format!("{:#X}", bloom), with_prefix);
    }

    // `TryFrom<&[u8]>` succeeds with a 256-byte slice, fails with 255 bytes
    // (`WrongLength`), fails with 257 bytes (`WrongLength`).
    #[test]
    fn test_try_from_slice() {
        let valid_bytes: [u8; 256] = [1; 256];
        let bloom = Bloom::try_from(&valid_bytes[..]).unwrap();
        assert_eq!(bloom.as_bytes(), &valid_bytes);

        let short_bytes: [u8; 255] = [1; 255];
        let err_short = Bloom::try_from(&short_bytes[..]).unwrap_err();
        assert!(matches!(
            err_short,
            error::ParseError::WrongLength(256, 255)
        ));

        let long_bytes: [u8; 257] = [1; 257];
        let err_long = Bloom::try_from(&long_bytes[..]).unwrap_err();
        assert!(matches!(err_long, error::ParseError::WrongLength(256, 257)));
    }

    // `FromStr` succeeds with a valid 512-character hex string (with and without
    // `0x` prefix), fails for wrong length, invalid characters, and odd length.
    #[test]
    fn test_from_str() {
        let valid_hex = format!("0x{}", "ab".repeat(256));
        let bloom = Bloom::from_str(&valid_hex).unwrap();
        assert_eq!(bloom.as_bytes(), &TEST_BYTES);

        let valid_hex_no_prefix = "ab".repeat(256);
        let bloom_no_prefix = Bloom::from_str(&valid_hex_no_prefix).unwrap();
        assert_eq!(bloom_no_prefix.as_bytes(), &TEST_BYTES);

        // Odd length: "0x" + 510 chars + "a" => 513 total characters (odd).
        // Note: `decode_hex` reports the *total* input length (not the stripped
        // hex portion), so the expected value is 513, matching the convention
        // already used in the Address/B256 tests.
        let odd_hex = format!("0x{}a", "ab".repeat(255));
        let err_odd = Bloom::from_str(&odd_hex).unwrap_err();
        assert!(matches!(err_odd, error::ParseError::OddHexLength(513)));

        // Short: "0x" + 510 chars => 255 bytes.
        let short_hex = format!("0x{}", "ab".repeat(255));
        let err_short = Bloom::from_str(&short_hex).unwrap_err();
        assert!(matches!(
            err_short,
            error::ParseError::WrongLength(256, 255)
        ));

        // Long: "0x" + 514 chars => 257 bytes.
        let long_hex = format!("0x{}", "ab".repeat(257));
        let err_long = Bloom::from_str(&long_hex).unwrap_err();
        assert!(matches!(err_long, error::ParseError::WrongLength(256, 257)));

        // Invalid char at position 513 (the last character of a 514-char string):
        // "0x" + 510 chars of "ab" + "a" at pos 512 + "g" at pos 513.
        let invalid_hex = format!("0x{}ag", "ab".repeat(255));
        let err_invalid = Bloom::from_str(&invalid_hex).unwrap_err();
        assert!(matches!(
            err_invalid,
            error::ParseError::InvalidHex('g', 513)
        ));
    }

    // `set_bit` then `has_bit` roundtrip for bit indices 0, 1, 7, 8, 2047.
    #[test]
    fn test_set_and_has_bit_roundtrip() {
        let mut bloom = Bloom::default();
        for bit in [0usize, 1, 7, 8, 2047] {
            assert!(
                !bloom.has_bit(bit).unwrap(),
                "bit {} should be unset initially",
                bit
            );
            bloom.set_bit(bit).unwrap();
            assert!(
                bloom.has_bit(bit).unwrap(),
                "bit {} should be set after set_bit",
                bit
            );
        }
    }

    // `set_bit(2048)` returns `Err(BitOutOfRange(2048))`.
    #[test]
    fn test_set_bit_out_of_range() {
        let mut bloom = Bloom::default();
        let err = bloom.set_bit(2048).unwrap_err();
        assert!(matches!(err, error::ParseError::BitOutOfRange(2048)));
    }

    // `has_bit(2048)` returns the same error as `set_bit(2048)`.
    #[test]
    fn test_has_bit_out_of_range() {
        let bloom = Bloom::default();
        let err = bloom.has_bit(2048).unwrap_err();
        assert!(matches!(err, error::ParseError::BitOutOfRange(2048)));
    }
}
