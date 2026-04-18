use crate::error;
use std::fmt::{self, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::str::FromStr;

// 20 bytes is small enough that implicit copying is desirable and matches how addresses are used throughout Ethereum codebases.
#[derive(alloy_rlp::RlpEncodable, alloy_rlp::RlpDecodable, Copy, Clone)]
pub struct Address([u8; 20]);

#[allow(clippy::derivable_impls)]
impl Default for Address {
    fn default() -> Self {
        Self([0; 20])
    }
}

impl PartialEq for Address {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Address {}

impl Hash for Address {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl From<[u8; 20]> for Address {
    fn from(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for Address {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "0x")?;
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

impl fmt::LowerHex for Address {
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

impl fmt::UpperHex for Address {
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

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self)?;
        Ok(())
    }
}

impl TryFrom<&[u8]> for Address {
    type Error = error::ParseError;
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let addr: [u8; 20] = value
            .try_into()
            .map_err(|_| error::ParseError::WrongLength(20, value.len()))?;
        Ok(Address(addr))
    }
}

impl FromStr for Address {
    type Err = error::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Address::try_from(error::decode_hex(s)?.as_slice())
    }
}

impl Address {
    pub fn new(bytes: [u8; 20]) -> Self {
        Self::from(bytes)
    }

    pub fn zero() -> Self {
        Self::default()
    }

    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // Address::default()` produces 20 zero bytes; its `Display` output is exactly 42 characters and starts with `0x`
    #[test]
    fn test_default_address() {
        let addr = Address::zero();
        assert_eq!(addr.as_bytes(), &[0; 20]);
        let addr_str = format!("{}", addr);
        assert_eq!(addr_str.len(), 42);
        assert_eq!(&addr_str[..2], "0x");
    }

    // `From<[u8; 20]>` round-trips correctly through `AsRef`
    #[test]
    fn test_from_array_round_trip() {
        let addr = Address::new([1; 20]);
        let bytes: &[u8] = addr.as_ref();
        assert_eq!(bytes, &[1; 20]);
    }

    // Two addresses with identical bytes are `==`; two with different bytes are `!=`
    #[test]
    fn test_equality() {
        let addr1 = Address::from([1; 20]);
        let addr2 = Address::from([1; 20]);
        let addr3 = Address::from([2; 20]);
        assert_eq!(addr1, addr2);
        assert_ne!(addr1, addr3);
    }

    #[test]
    fn test_same_address_hashset() {
        let mut set: HashSet<Address> = HashSet::new();
        let addr1 = Address::from([1; 20]);
        let addr2 = Address::from([1; 20]);
        set.insert(addr1);
        set.insert(addr2);
        assert_eq!(set.len(), 1);
    }

    fn copy_test(addr1: Address, addr2: Address) {
        assert_eq!(addr1, addr2);
    }

    // Assign an `Address` to two variables, pass both to a function by value — this should compile because `Address` is `Copy`.
    #[test]
    fn test_copy() {
        let addr1 = Address::from([1; 20]);
        let addr2 = addr1; // This should compile without error due to the `Copy` trait
        copy_test(addr1, addr2); // Both `addr1` and `addr2` should be usable here
    }

    // `Display` output for a known byte sequence matches the expected string.
    #[test]
    fn test_display() {
        let bytes: [u8; 20] = [
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc,
        ];
        let addr = Address::from(bytes);
        assert_eq!(
            format!("{}", addr),
            "0x123456789abcdef0112233445566778899aabbcc"
        );
    }

    #[test]
    fn test_debug() {
        let bytes: [u8; 20] = [
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc,
        ];
        let addr = Address::from(bytes);
        assert_eq!(
            format!("{:?}", addr),
            "Address(0x123456789abcdef0112233445566778899aabbcc)"
        );
    }

    // `format!("{:x}", addr)` and `format!("{:#x}", addr)` produce the expected outputs for `LowerHex`
    #[test]
    fn test_lower_hex() {
        let bytes: [u8; 20] = [
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc,
        ];
        let addr = Address::from(bytes);
        assert_eq!(
            format!("{:x}", addr),
            "123456789abcdef0112233445566778899aabbcc"
        );
        assert_eq!(
            format!("{:#x}", addr),
            "0x123456789abcdef0112233445566778899aabbcc"
        );
    }

    // `format!("{:X}", addr)` and `format!("{:#X}", addr)` produce the expected outputs for `UpperHex`
    #[test]
    fn test_upper_hex() {
        let bytes: [u8; 20] = [
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc,
        ];
        let addr = Address::from(bytes);
        assert_eq!(
            format!("{:X}", addr),
            "123456789ABCDEF0112233445566778899AABBCC"
        );
        assert_eq!(
            format!("{:#X}", addr),
            "0x123456789ABCDEF0112233445566778899AABBCC"
        );
    }

    // `TryFrom<&[u8]>` succeeds with a 20-byte slice, fails with 19 bytes (`WrongLength`), fails with 21 bytes (`WrongLength`).
    #[test]
    fn test_try_from_slice() {
        let valid_bytes: [u8; 20] = [1; 20];
        let addr = Address::try_from(&valid_bytes[..]).unwrap();
        assert_eq!(addr.as_bytes(), &valid_bytes);
        let short_bytes: [u8; 19] = [1; 19];
        let err_short = Address::try_from(&short_bytes[..]).unwrap_err();
        assert!(matches!(err_short, error::ParseError::WrongLength(20, 19)));
        let long_bytes: [u8; 21] = [1; 21];
        let err_long = Address::try_from(&long_bytes[..]).unwrap_err();
        assert!(matches!(err_long, error::ParseError::WrongLength(20, 21)));
    }

    // `FromStr` succeeds with a valid 40-character hex string (with and without `0x` prefix), fails for wrong length, fails for invalid characters, fails for odd length.
    #[test]
    fn test_from_str() {
        let valid_hex = "0x123456789abcdef0112233445566778899aabbcc";
        let addr = Address::from_str(valid_hex).unwrap();
        assert_eq!(
            addr.as_bytes(),
            &[
                0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
                0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc
            ]
        );
        let valid_hex_no_prefix = "123456789abcdef0112233445566778899aabbcc";
        let addr_no_prefix = Address::from_str(valid_hex_no_prefix).unwrap();
        assert_eq!(
            addr_no_prefix.as_bytes(),
            &[
                0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
                0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc
            ]
        );
        let odd_hex = "0x123456789abcdef0112233445566778899aabbc";
        let err_odd = Address::from_str(odd_hex).unwrap_err();
        assert!(matches!(err_odd, error::ParseError::OddHexLength(41)));

        let short_hex = "0x123456789abcdef0112233445566778899aabb";
        let err_short = Address::from_str(short_hex).unwrap_err();
        assert!(matches!(err_short, error::ParseError::WrongLength(20, 19)));

        let long_hex = "0x123456789abcdef0112233445566778899aabbccdd";
        let err_long = Address::from_str(long_hex).unwrap_err();
        assert!(matches!(err_long, error::ParseError::WrongLength(20, 21)));

        let invalid_hex = "0x123456789abcdef0112233445566778899aabbcg";
        let err_invalid = Address::from_str(invalid_hex).unwrap_err();
        assert!(matches!(
            err_invalid,
            error::ParseError::InvalidHex('g', 41)
        ));
    }
}
