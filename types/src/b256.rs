use crate::error;
use std::fmt::{self, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::str::FromStr;

#[derive(alloy_rlp::RlpEncodable, alloy_rlp::RlpDecodable, Copy, Clone)]
pub struct B256([u8; 32]);

#[allow(clippy::derivable_impls)]
impl Default for B256 {
    fn default() -> Self {
        Self([0; 32])
    }
}

impl PartialEq for B256 {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for B256 {}

impl Hash for B256 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl From<[u8; 32]> for B256 {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for B256 {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Display for B256 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "0x")?;
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

impl fmt::LowerHex for B256 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

impl fmt::UpperHex for B256 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "0x")?;
        }
        for byte in &self.0 {
            write!(f, "{:02X}", byte)?;
        }
        Ok(())
    }
}

impl fmt::Debug for B256 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "B256({})", self)?;
        Ok(())
    }
}

impl TryFrom<&[u8]> for B256 {
    type Error = error::ParseError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| error::ParseError::WrongLength(32, bytes.len()))?;
        Ok(Self(arr))
    }
}

impl FromStr for B256 {
    type Err = error::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(error::decode_hex(s)?.as_slice())
    }
}

impl B256 {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self::from(bytes)
    }

    pub fn zero() -> Self {
        Self::default()
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const TEST_BYTES: [u8; 32] = [
        0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc,
        0xfe, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff, 0x00,
    ];
    const TEST_HEX: &str = "0x0123456789abcdef1032547698badcfe112233445566778899aabbccddeeff00";
    const TEST_HEX_NO_PREFIX: &str =
        "0123456789abcdef1032547698badcfe112233445566778899aabbccddeeff00";

    // B256::default()` produces 32 zero bytes; its `Display` output is exactly 42 characters and starts with `0x`
    #[test]
    fn test_default() {
        let b256 = B256::zero();
        assert_eq!(b256.as_bytes(), &[0; 32]);
        let b256_str = format!("{}", b256);
        assert_eq!(b256_str.len(), 66);
        assert_eq!(&b256_str[..2], "0x");
    }

    // `From<[u8; 32]>` round-trips correctly through `AsRef`
    #[test]
    fn test_from_array_round_trip() {
        let b256 = B256::new([1; 32]);
        let bytes: &[u8] = b256.as_ref();
        assert_eq!(bytes, &[1; 32]);
    }

    // Two b256 with identical bytes are `==`; two with different bytes are `!=`
    #[test]
    fn test_equality() {
        let b256_1 = B256::from([1; 32]);
        let b256_2 = B256::from([1; 32]);
        let b256_3 = B256::from([2; 32]);
        assert_eq!(b256_1, b256_2);
        assert_ne!(b256_1, b256_3);
    }

    #[test]
    fn test_same_b256_hashset() {
        let mut set: HashSet<B256> = HashSet::new();
        let b256_1 = B256::from([1; 32]);
        let b256_2 = B256::from([1; 32]);
        set.insert(b256_1);
        set.insert(b256_2);
        assert_eq!(set.len(), 1);
    }

    fn copy_test(b256_1: B256, b256_2: B256) {
        assert_eq!(b256_1, b256_2);
    }

    // Assign an `B256` to two variables, pass both to a function by value — this should compile because `B256` is `Copy`.
    #[test]
    fn test_copy() {
        let b256_1 = B256::from([1; 32]);
        let b256_2 = b256_1; // This should compile without error due to the `Copy` trait
        copy_test(b256_1, b256_2); // Both `b256_1` and `b256_2` should be usable here
    }

    // `Display` output for a known byte sequence matches the expected string.
    #[test]
    fn test_display() {
        let b256 = B256::from(TEST_BYTES);
        assert_eq!(format!("{}", b256), TEST_HEX);
    }

    #[test]
    fn test_debug() {
        let b256 = B256::from(TEST_BYTES);
        assert_eq!(
            format!("{:?}", b256),
            "B256(0x0123456789abcdef1032547698badcfe112233445566778899aabbccddeeff00)"
        );
    }

    // `format!("{:x}", b256)` and `format!("{:#x}", b256)` produce the expected outputs for `LowerHex`
    #[test]
    fn test_lower_hex() {
        let b256 = B256::from(TEST_BYTES);
        assert_eq!(format!("{:x}", b256), TEST_HEX_NO_PREFIX);
        assert_eq!(format!("{:#x}", b256), TEST_HEX);
    }

    // `format!("{:X}", b256)` and `format!("{:#X}", b256)` produce the expected outputs for `UpperHex`
    #[test]
    fn test_upper_hex() {
        let b256 = B256::from(TEST_BYTES);
        assert_eq!(format!("{:X}", b256), TEST_HEX_NO_PREFIX.to_uppercase());
        assert_eq!(
            format!("{:#X}", b256),
            "0x0123456789ABCDEF1032547698BADCFE112233445566778899AABBCCDDEEFF00"
        );
    }

    // `TryFrom<&[u8]>` succeeds with a 20-byte slice, fails with 19 bytes (`WrongLength`), fails with 21 bytes (`WrongLength`).
    #[test]
    fn test_try_from_slice() {
        let valid_bytes: [u8; 32] = [1; 32];
        let b256 = B256::try_from(&valid_bytes[..]).unwrap();
        assert_eq!(b256.as_bytes(), &valid_bytes);
        let short_bytes: [u8; 31] = [1; 31];
        let err_short = B256::try_from(&short_bytes[..]).unwrap_err();
        assert!(matches!(err_short, error::ParseError::WrongLength(32, 31)));
        let long_bytes: [u8; 33] = [1; 33];
        let err_long = B256::try_from(&long_bytes[..]).unwrap_err();
        assert!(matches!(err_long, error::ParseError::WrongLength(32, 33)));
    }

    // `FromStr` succeeds with a valid 40-character hex string (with and without `0x` prefix), fails for wrong length, fails for invalid characters, fails for odd length.
    #[test]
    fn test_from_str() {
        let valid_hex = "0x0123456789abcdef1032547698badcfe112233445566778899aabbccddeeff00";
        let b256 = B256::from_str(valid_hex).unwrap();
        assert_eq!(b256.as_bytes(), &TEST_BYTES);
        let valid_hex_no_prefix =
            "0123456789abcdef1032547698badcfe112233445566778899aabbccddeeff00";
        let b256_no_prefix = B256::from_str(valid_hex_no_prefix).unwrap();
        assert_eq!(b256_no_prefix.as_bytes(), &TEST_BYTES);
        let odd_hex = "0123456789abcdef1032547698badcfe112233445566778899aabbccddeeff0";
        let err_odd = B256::from_str(odd_hex).unwrap_err();
        assert!(matches!(err_odd, error::ParseError::OddHexLength(63)));

        let short_hex = "0x0123456789abcdef1032547698badcfe112233445566778899aabbccddeeff";
        let err_short = B256::from_str(short_hex).unwrap_err();
        assert!(matches!(err_short, error::ParseError::WrongLength(32, 31)));

        let long_hex = "0x0123456789abcdef1032547698badcfe112233445566778899aabbccddeeff0011";
        let err_long = B256::from_str(long_hex).unwrap_err();
        assert!(matches!(err_long, error::ParseError::WrongLength(32, 33)));

        let invalid_hex = "0x0123456789abcdef1032547698badcfe112233445566778899aabbccddeeff0g";
        let err_invalid = B256::from_str(invalid_hex).unwrap_err();
        assert!(matches!(
            err_invalid,
            error::ParseError::InvalidHex('g', 65)
        ));
    }
}
