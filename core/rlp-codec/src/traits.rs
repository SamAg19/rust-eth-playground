// =============================================================================
// Comparison: hand-written (this file) vs `alloy-rlp` derive-generated code
// Observed via `cargo expand -p types` after adding
// `#[derive(alloy_rlp::RlpEncodable, alloy_rlp::RlpDecodable)]` to the types.
// =============================================================================
//
// 1. Derive scope. alloy-rlp's derive emitted impls for `Address` and `B256`
//    only. `Transaction` (enum with struct variants) and `AccessListItem` did
//    not get impls — the derive doesn't handle enums out of the box, so the
//    transaction RLP layout still has to be written by hand either way.
//
// 2. Tuple struct wrapping. For `Address([u8; 20])`, alloy-rlp encodes the
//    struct as an RLP **list** containing the inner array
//    (`Header { list: true, payload_length }` then recurse on `self.0`). My
//    impl encodes it as a flat `RlpItem::Bytes` of the 20 raw bytes. Both are
//    self-consistent roundtrips but produce different byte strings — the
//    alloy form would not interop with Ethereum's expected "20-byte string"
//    encoding for addresses without a `#[rlp(trailing)]` / newtype escape.
//
// 3. Two-stage vs one-stage. My pipeline is
//        value -> RlpItem tree -> bytes
//    i.e. allocate an intermediate AST, then serialise. alloy-rlp emits code
//    that writes directly into a `&mut dyn BufMut` in one pass, and provides
//    a separate `fn length(&self) -> usize` so callers can pre-size buffers.
//    No intermediate tree is built.
//
// 4. Length-prefix strategy. alloy-rlp computes payload length up front via a
//    generated `_alloy_rlp_payload_length` method, then writes the header,
//    then streams children. My encoder writes children into a scratch
//    `BytesMut`, measures it, then writes the prefix and copies the payload.
//    alloy avoids the scratch buffer by trusting `length()` to agree with
//    `encode()`.
//
// 5. Header abstraction. alloy-rlp has a single `alloy_rlp::Header`
//    (list-bit + payload_length) that knows how to serialise the 5 RLP
//    length-prefix cases. My encoder/decoder handle those 5 cases inline at
//    the match sites.
//
// 6. Decoder cursor style. alloy-rlp's signature is
//        fn decode(b: &mut &[u8]) -> alloy_rlp::Result<Self>
//    i.e. it mutates the slice-reference to advance past consumed bytes. My
//    decoder returns `(RlpItem, &[u8])` — the remaining slice is a tuple
//    element rather than an out-parameter.
//
// 7. Post-decode length check. After decoding children, alloy-rlp asserts
//    `consumed == payload_length` and returns `ListLengthMismatch` otherwise.
//    My decoder slices the payload to exactly `payload_len` before recursing,
//    so an equivalent mismatch surfaces as leftover bytes inside the inner
//    loop rather than as a single tail check.
//
// 8. Error shape. alloy-rlp uses `alloy_rlp::Error` with semantic variants
//    like `UnexpectedString`, `InputTooShort`, `ListLengthMismatch`,
//    `Overflow`. My `RlpError` carries similar information but encodes the
//    "wrong variant" case as `UnexpectedType(u8)` with the received RLP
//    prefix byte (0x80 / 0xc0).
//
// 9. Trait recursion. alloy-rlp's generated code calls
//    `alloy_rlp::Encodable::encode(&self.0, out)`, relying on blanket impls
//    for `[u8; N]`, `Vec<T>`, etc. My impls call `self.as_bytes()` and wrap
//    in `RlpItem::Bytes` directly — no trait dispatch on the inner array.
//
// 10. Inlining and hygiene. alloy-rlp annotates every generated method with
//     `#[inline]` and wraps the impls in `const _: () = { ... }` blocks to
//     scope the `extern crate alloy_rlp;` declaration. My hand-written code
//     has neither — readability over micro-optimisation.
//
// 11. Integer encoding. Neither side special-cases integers here because
//     neither `Address` nor `B256` contains any; the byte arrays are
//     encoded/decoded verbatim. For my `u64` / `u128` impls I strip leading
//     zeros explicitly; alloy-rlp's equivalent blanket impls (not shown in
//     the expand for these types) do the same per the RLP spec.
// =============================================================================

use crate::error::RlpError;
use crate::item::RlpItem;
use bytes::Bytes;
use types::{AccessListItem, Address, B256, Bloom, Transaction};

pub trait RlpEncodable {
    fn to_rlp_item(&self) -> RlpItem;
}

pub trait RlpDecodable {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError>
    where
        Self: Sized;
}

impl RlpEncodable for u64 {
    fn to_rlp_item(&self) -> RlpItem {
        let bytes = self.to_be_bytes();
        let first_nonzero = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
        RlpItem::Bytes(Bytes::copy_from_slice(&bytes[first_nonzero..]))
    }
}

impl RlpDecodable for u64 {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError>
    where
        Self: Sized,
    {
        match item {
            RlpItem::Bytes(x) => {
                if x.len() > 8 {
                    return Err(RlpError::InvalidLength(x.len()));
                }
                let mut arr: [u8; 8] = [0x00; 8];
                arr[8 - x.len()..].copy_from_slice(x);
                Ok(u64::from_be_bytes(arr))
            }
            RlpItem::List(_) => Err(RlpError::UnexpectedType(0xc0)),
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
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError>
    where
        Self: Sized,
    {
        match item {
            RlpItem::Bytes(x) => {
                if x.len() > 16 {
                    return Err(RlpError::InvalidLength(x.len()));
                }
                let mut arr: [u8; 16] = [0x00; 16];
                arr[16 - x.len()..].copy_from_slice(x);
                Ok(u128::from_be_bytes(arr))
            }
            RlpItem::List(_) => Err(RlpError::UnexpectedType(0xc0)),
        }
    }
}

impl RlpEncodable for bool {
    fn to_rlp_item(&self) -> RlpItem {
        match self {
            false => RlpItem::Bytes(Bytes::from(vec![0x00])),
            true => RlpItem::Bytes(Bytes::from(vec![0x01])),
        }
    }
}

impl RlpDecodable for bool {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError>
    where
        Self: Sized,
    {
        match item {
            RlpItem::Bytes(x) => {
                if x.len() > 1 || x.is_empty() {
                    return Err(RlpError::InvalidLength(x.len()));
                }
                match x[0] {
                    0x00 => Ok(false),
                    0x01 => Ok(true),
                    _ => Err(RlpError::UnexpectedType(x[0])),
                }
            }
            RlpItem::List(_) => Err(RlpError::UnexpectedType(0xc0)),
        }
    }
}

impl RlpEncodable for Vec<u8> {
    fn to_rlp_item(&self) -> RlpItem {
        RlpItem::Bytes(Bytes::from(self.clone()))
    }
}

impl RlpDecodable for Vec<u8> {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError>
    where
        Self: Sized,
    {
        match item {
            RlpItem::Bytes(x) => Ok(x.to_vec()),
            RlpItem::List(_) => Err(RlpError::UnexpectedType(0xc0)),
        }
    }
}

impl RlpEncodable for Bloom {
    fn to_rlp_item(&self) -> RlpItem {
        RlpItem::Bytes(Bytes::copy_from_slice(&self.as_bytes()[..]))
    }
}

impl RlpDecodable for Bloom {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError>
    where
        Self: Sized
    {
        match item {
            RlpItem::Bytes(x) => {
                let arr = Bloom::try_from(&x[..]).map_err(|_| RlpError::InvalidLength(x.len()))?;
                Ok(arr)
            }
            RlpItem::List(_) => Err(RlpError::UnexpectedType(0xc0)),
        }
    }
}

impl RlpEncodable for Address {
    fn to_rlp_item(&self) -> RlpItem {
        RlpItem::Bytes(Bytes::copy_from_slice(&self.as_bytes()[..]))
    }
}

impl RlpDecodable for Address {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError>
    where
        Self: Sized,
    {
        match item {
            RlpItem::Bytes(x) => {
                let arr =
                    Address::try_from(&x[..]).map_err(|_| RlpError::InvalidLength(x.len()))?;
                Ok(arr)
            }
            RlpItem::List(_) => Err(RlpError::UnexpectedType(0xc0)),
        }
    }
}

impl RlpEncodable for B256 {
    fn to_rlp_item(&self) -> RlpItem {
        RlpItem::Bytes(Bytes::copy_from_slice(&self.as_bytes()[..]))
    }
}

impl RlpDecodable for B256 {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError>
    where
        Self: Sized,
    {
        match item {
            RlpItem::Bytes(x) => {
                let arr = B256::try_from(&x[..]).map_err(|_| RlpError::InvalidLength(x.len()))?;
                Ok(arr)
            }
            RlpItem::List(_) => Err(RlpError::UnexpectedType(0xc0)),
        }
    }
}

impl RlpEncodable for AccessListItem {
    fn to_rlp_item(&self) -> RlpItem {
        let mut list: Vec<RlpItem> = vec![];
        list.push(self.address.to_rlp_item());
        let item_list: Vec<RlpItem> = self
            .storage_keys
            .iter()
            .map(|sk| sk.to_rlp_item())
            .collect();
        list.push(RlpItem::List(item_list));

        RlpItem::List(list)
    }
}

impl RlpDecodable for AccessListItem {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError>
    where
        Self: Sized,
    {
        match item {
            RlpItem::Bytes(_) => Err(RlpError::UnexpectedType(0x80)),
            RlpItem::List(x) => {
                if x.len() != 2 {
                    return Err(RlpError::InvalidLength(x.len()));
                }
                let address = Address::from_rlp_item(&x[0])?;
                let storage_keys = match &x[1] {
                    RlpItem::Bytes(_) => return Err(RlpError::UnexpectedType(0x80)),
                    RlpItem::List(x) => {
                        let mut keys = vec![];
                        for key_item in x {
                            let key = B256::from_rlp_item(key_item)?;
                            keys.push(key);
                        }
                        keys
                    }
                };

                Ok(Self {
                    address,
                    storage_keys,
                })
            }
        }
    }
}

// Legacy: tag, nonce, gas_limit, to, value, data, gas_price
// EIP-1559: tag, nonce, gas_limit, to, value, data, max_fee_per_gas, max_priority_fee_per_gas, access_list
// EIP-4844: EIP-1559 fields + max_fee_per_blob_gas, blob_versioned_hashes
impl RlpEncodable for Transaction {
    fn to_rlp_item(&self) -> RlpItem {
        let mut fields = vec![];
        match self {
            Transaction::Legacy {
                nonce,
                gas_price,
                gas_limit,
                to,
                value,
                data,
            } => {
                fields.push(0u64.to_rlp_item());
                fields.push(nonce.to_rlp_item());
                fields.push(gas_limit.to_rlp_item());
                let to_item = match to {
                    Some(x) => x.to_rlp_item(),
                    None => RlpItem::Bytes(Bytes::from(vec![])),
                };
                fields.push(to_item);
                fields.push(value.to_rlp_item());
                fields.push(data.to_rlp_item());
                fields.push(gas_price.to_rlp_item());
            }
            Transaction::Eip1559 {
                nonce,
                max_priority_fee_per_gas,
                max_fee_per_gas,
                gas_limit,
                to,
                value,
                data,
                access_list,
            } => {
                fields.push(2u64.to_rlp_item());
                fields.push(nonce.to_rlp_item());
                fields.push(gas_limit.to_rlp_item());
                let to_item = match to {
                    Some(x) => x.to_rlp_item(),
                    None => RlpItem::Bytes(Bytes::from(vec![])),
                };
                fields.push(to_item);
                fields.push(value.to_rlp_item());
                fields.push(data.to_rlp_item());
                fields.push(max_fee_per_gas.to_rlp_item());
                fields.push(max_priority_fee_per_gas.to_rlp_item());
                let access_list_item: Vec<RlpItem> =
                    access_list.iter().map(|a| a.to_rlp_item()).collect();
                fields.push(RlpItem::List(access_list_item));
            }
            Transaction::Eip4844 {
                nonce,
                max_priority_fee_per_gas,
                max_fee_per_gas,
                max_fee_per_blob_gas,
                gas_limit,
                to,
                value,
                data,
                access_list,
                blob_versioned_hashes,
            } => {
                fields.push(3u64.to_rlp_item());
                fields.push(nonce.to_rlp_item());
                fields.push(gas_limit.to_rlp_item());
                let to_item = match to {
                    Some(x) => x.to_rlp_item(),
                    None => RlpItem::Bytes(Bytes::from(vec![])),
                };
                fields.push(to_item);
                fields.push(value.to_rlp_item());
                fields.push(data.to_rlp_item());
                fields.push(max_fee_per_gas.to_rlp_item());
                fields.push(max_priority_fee_per_gas.to_rlp_item());
                let access_list_item: Vec<RlpItem> =
                    access_list.iter().map(|a| a.to_rlp_item()).collect();
                fields.push(RlpItem::List(access_list_item));
                fields.push(max_fee_per_blob_gas.to_rlp_item());
                fields.push(RlpItem::List(
                    blob_versioned_hashes
                        .iter()
                        .map(|sk| sk.to_rlp_item())
                        .collect(),
                ));
            }
            #[cfg(feature = "optimism")]
            Transaction::Deposit { .. } => todo!(),
        }

        RlpItem::List(fields)
    }
}

fn decode_to(item: &RlpItem) -> Result<Option<Address>, RlpError> {
    match item {
        RlpItem::Bytes(b) => {
            if b.is_empty() {
                Ok(None)
            } else {
                Ok(Some(Address::from_rlp_item(item)?))
            }
        }
        RlpItem::List(_) => Err(RlpError::UnexpectedType(0xc0)),
    }
}

fn decode_list_of<T: RlpDecodable>(item: &RlpItem) -> Result<Vec<T>, RlpError> {
    match item {
        RlpItem::Bytes(_) => Err(RlpError::UnexpectedType(0x80)),
        RlpItem::List(x) => x.iter().map(T::from_rlp_item).collect(),
    }
}

// Legacy: tag, nonce, gas_limit, to, value, data, gas_price
// EIP-1559: tag, nonce, gas_limit, to, value, data, max_fee_per_gas, max_priority_fee_per_gas, access_list
// EIP-4844: EIP-1559 fields + max_fee_per_blob_gas, blob_versioned_hashes
impl RlpDecodable for Transaction {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, RlpError>
    where
        Self: Sized,
    {
        let x = match item {
            RlpItem::Bytes(_) => return Err(RlpError::UnexpectedType(0x80)),
            RlpItem::List(x) => x,
        };

        if x.is_empty() {
            return Err(RlpError::InvalidLength(0));
        }

        let tag = u64::from_rlp_item(&x[0])?;

        match tag {
            0 => {
                if x.len() != 7 {
                    return Err(RlpError::InvalidLength(x.len()));
                }
                Ok(Transaction::Legacy {
                    nonce: u64::from_rlp_item(&x[1])?,
                    gas_limit: u64::from_rlp_item(&x[2])?,
                    to: decode_to(&x[3])?,
                    value: u128::from_rlp_item(&x[4])?,
                    data: Vec::<u8>::from_rlp_item(&x[5])?,
                    gas_price: u128::from_rlp_item(&x[6])?,
                })
            }
            2 => {
                if x.len() != 9 {
                    return Err(RlpError::InvalidLength(x.len()));
                }
                Ok(Transaction::Eip1559 {
                    nonce: u64::from_rlp_item(&x[1])?,
                    gas_limit: u64::from_rlp_item(&x[2])?,
                    to: decode_to(&x[3])?,
                    value: u128::from_rlp_item(&x[4])?,
                    data: Vec::<u8>::from_rlp_item(&x[5])?,
                    max_fee_per_gas: u128::from_rlp_item(&x[6])?,
                    max_priority_fee_per_gas: u128::from_rlp_item(&x[7])?,
                    access_list: decode_list_of::<AccessListItem>(&x[8])?,
                })
            }
            3 => {
                if x.len() != 11 {
                    return Err(RlpError::InvalidLength(x.len()));
                }
                Ok(Transaction::Eip4844 {
                    nonce: u64::from_rlp_item(&x[1])?,
                    gas_limit: u64::from_rlp_item(&x[2])?,
                    to: decode_to(&x[3])?,
                    value: u128::from_rlp_item(&x[4])?,
                    data: Vec::<u8>::from_rlp_item(&x[5])?,
                    max_fee_per_gas: u128::from_rlp_item(&x[6])?,
                    max_priority_fee_per_gas: u128::from_rlp_item(&x[7])?,
                    access_list: decode_list_of::<AccessListItem>(&x[8])?,
                    max_fee_per_blob_gas: u128::from_rlp_item(&x[9])?,
                    blob_versioned_hashes: decode_list_of::<B256>(&x[10])?,
                })
            }
            _ => Err(RlpError::UnexpectedType(u8::MAX)),
        }
    }
}
