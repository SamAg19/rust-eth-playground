use crate::item::RlpItem;
use crate::error::RlpError;
use types::{Address, B256};
use bytes::Bytes;

pub trait RlpEncodable {
    fn to_rlp_item(&self) -> RlpItem;
}

pub trait RlpDecodable {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError> where Self: Sized;
}

impl RlpEncodable for u64 {
    fn to_rlp_item(&self) -> RlpItem {
        let bytes = self.to_be_bytes();
        let first_nonzero = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
        RlpItem::Bytes(Bytes::copy_from_slice(&bytes[first_nonzero..]))
    }
}

impl RlpDecodable for u64 {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError> where Self: Sized {
        match item {
            RlpItem::Bytes(x) => {
                if x.len() > 8 {
                    return Err(RlpError::InvalidLength(x.len()));
                }
                let mut arr: [u8; 8] = [0x00; 8];
                arr[8 - x.len()..].copy_from_slice(&x);
                Ok(u64::from_be_bytes(arr))
            },
            RlpItem::List(_) => Err(RlpError::UnexpectedType(0xc0))
        }
    }
}

impl RlpEncodable for u128 {
    fn to_rlp_item(&self) -> RlpItem {
        let bytes = self.to_be_bytes();
        let first_nonzero = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
        RlpItem::Bytes(Bytes::copy_from_slice(&bytes[first_nonzero..]))
    }
}

impl RlpDecodable for u128 {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError> where Self: Sized {
        match item {
            RlpItem::Bytes(x) => {
                if x.len() > 16 {
                    return Err(RlpError::InvalidLength(x.len()));
                }
                let mut arr: [u8; 16] = [0x00; 16];
                arr[16 - x.len()..].copy_from_slice(&x);
                Ok(u128::from_be_bytes(arr))
            },
            RlpItem::List(_) => Err(RlpError::UnexpectedType(0xc0))
        }
    }
}

impl RlpEncodable for bool {
    fn to_rlp_item(&self) -> RlpItem {
        match self {
            false => RlpItem::Bytes(Bytes::from(vec![0x00])),
            true => RlpItem::Bytes(Bytes::from(vec![0x01]))
        }
    }
}

impl RlpDecodable for bool {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError> where Self: Sized {
        match item {
            RlpItem::Bytes(x) => {
                if x.len() > 1 || x.len() == 0 {
                    return Err(RlpError::InvalidLength(x.len()));
                }
                match x[0] {
                    0x00 => Ok(false),
                    0x01 => Ok(true),
                    _ => Err(RlpError::UnexpectedType(x[0]))
                }
            },
            RlpItem::List(_) => Err(RlpError::UnexpectedType(0xc0))
        }
    }
}

impl RlpEncodable for Vec<u8> {
    fn to_rlp_item(&self) -> RlpItem {
        RlpItem::Bytes(Bytes::from(self.clone()))
    }
}

impl RlpDecodable for Vec<u8> {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError> where Self: Sized {
        match item {
            RlpItem::Bytes(x) => {
                Ok(x.to_vec())
            },
            RlpItem::List(_) => Err(RlpError::UnexpectedType(0xc0))
        }
    }
}

impl RlpEncodable for Address {
    fn to_rlp_item(&self) -> RlpItem {
        RlpItem::Bytes(Bytes::copy_from_slice(&self.as_bytes()[..]))
    }
}

impl RlpDecodable for Address {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError> where Self: Sized {
         match item {
            RlpItem::Bytes(x) => {
                let arr = Address::try_from(&x[..]).map_err(|_| RlpError::InvalidLength(x.len()))?;
                Ok(arr)
            },
            RlpItem::List(_) => Err(RlpError::UnexpectedType(0xc0))
        }
    }
}

impl RlpEncodable for B256 {
    fn to_rlp_item(&self) -> RlpItem {
        RlpItem::Bytes(Bytes::copy_from_slice(&self.as_bytes()[..]))
    }
}

impl RlpDecodable for B256 {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError> where Self: Sized {
         match item {
            RlpItem::Bytes(x) => {
                let arr = B256::try_from(&x[..]).map_err(|_| RlpError::InvalidLength(x.len()))?;
                Ok(arr)
            },
            RlpItem::List(_) => Err(RlpError::UnexpectedType(0xc0))
        }
    }
}