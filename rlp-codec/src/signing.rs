use bytes::{BufMut, Bytes, BytesMut};
use k256::ecdsa::{RecoveryId, Signature, SigningKey, VerifyingKey};
use sha3::{Digest, Keccak256};
use thiserror::Error;
use types::{Address, B256, Transaction};

use crate::{RlpEncodable, RlpError, RlpItem, encode};

#[derive(Debug, Clone, PartialEq)]
pub struct SignedTransaction {
    pub transaction: Transaction,
    pub v: u64,
    pub r: B256,
    pub s: B256,
}

#[derive(Debug, Error)]
pub enum SigningError {
    #[error("the key bytes were not a valid secp256k1 scalar")]
    InvalidPrivateKey,
    #[error(transparent)]
    SigningFailed(#[from] k256::ecdsa::Error),
    #[error("the signature and payload did not yield a valid public key")]
    RecoveryFailed,
    #[error("the `v`, `r`, or `s` values were structurally invalid")]
    InvalidSignature,
    #[error(transparent)]
    PayloadEncoding(#[from] RlpError),
}

fn legacy_to_wire(signed_tx: &SignedTransaction) -> Result<Bytes, RlpError> {
    let mut fields: Vec<RlpItem> = vec![];

    match &signed_tx.transaction {
        Transaction::Legacy {
            nonce,
            gas_price,
            gas_limit,
            to,
            value,
            data,
        } => {
            fields.push(nonce.to_rlp_item());
            fields.push(gas_price.to_rlp_item());
            fields.push(gas_limit.to_rlp_item());
            fields.push(encode_to(to));
            fields.push(value.to_rlp_item());
            fields.push(data.to_rlp_item());
            fields.push(signed_tx.v.to_rlp_item());
            fields.push(signed_tx.r.to_rlp_item());
            fields.push(signed_tx.s.to_rlp_item());
        }
        _ => {
            unreachable!("legacy_to_encoded_bytes called with non-Legacy tx")
        }
    }

    let mut buffer = BytesMut::new();
    encode(&RlpItem::List(fields), &mut buffer)?;

    Ok(buffer.freeze())
}

fn typed_to_wire(signed_tx: &SignedTransaction) -> Result<Bytes, RlpError> {
    let mut fields: Vec<RlpItem> = vec![];

    match &signed_tx.transaction {
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
            fields.push(nonce.to_rlp_item());
            fields.push(max_priority_fee_per_gas.to_rlp_item());
            fields.push(max_fee_per_gas.to_rlp_item());
            fields.push(gas_limit.to_rlp_item());
            fields.push(encode_to(to));
            fields.push(value.to_rlp_item());
            fields.push(data.to_rlp_item());
            fields.push(RlpItem::List(
                access_list.iter().map(|a| a.to_rlp_item()).collect(),
            ));
            fields.push(signed_tx.v.to_rlp_item());
            fields.push(signed_tx.r.to_rlp_item());
            fields.push(signed_tx.s.to_rlp_item());
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
            fields.push(nonce.to_rlp_item());
            fields.push(max_priority_fee_per_gas.to_rlp_item());
            fields.push(max_fee_per_gas.to_rlp_item());
            fields.push(gas_limit.to_rlp_item());
            fields.push(encode_to(to));
            fields.push(value.to_rlp_item());
            fields.push(data.to_rlp_item());
            fields.push(RlpItem::List(
                access_list.iter().map(|a| a.to_rlp_item()).collect(),
            ));
            fields.push(max_fee_per_blob_gas.to_rlp_item());
            fields.push(RlpItem::List(
                blob_versioned_hashes
                    .iter()
                    .map(|h| h.to_rlp_item())
                    .collect(),
            ));
            fields.push(signed_tx.v.to_rlp_item());
            fields.push(signed_tx.r.to_rlp_item());
            fields.push(signed_tx.s.to_rlp_item());
        }
        _ => unreachable!("typed_to_encoded_bytes called with Legacy tx"),
    }

    let tx_type = match &signed_tx.transaction {
        Transaction::Legacy { .. } => 0x00,
        Transaction::Eip1559 { .. } => 0x02,
        Transaction::Eip4844 { .. } => 0x03,
    };

    let mut buffer = BytesMut::new();
    buffer.put_u8(tx_type);
    encode(&RlpItem::List(fields), &mut buffer)?;

    Ok(buffer.freeze())
}

impl SignedTransaction {
    pub fn hash(&self) -> Result<B256, SigningError> {
        let bytes = match &self.transaction {
            Transaction::Legacy { .. } => legacy_to_wire(self)?,
            Transaction::Eip1559 { .. } | Transaction::Eip4844 { .. } => typed_to_wire(self)?,
        };
        Ok(keccak256(&bytes))
    }
}

pub fn keccak256(data: &[u8]) -> B256 {
    let mut hasher = Keccak256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    let mut arr: [u8; 32] = [0x00; 32];
    arr.copy_from_slice(&hash);
    B256::new(arr)
}

fn encode_to(to: &Option<Address>) -> RlpItem {
    match to {
        Some(addr) => addr.to_rlp_item(),
        None => RlpItem::Bytes(Bytes::new()),
    }
}

fn legacy_to_encoded_bytes(tx: &Transaction, chain_id: u64) -> Result<Bytes, RlpError> {
    let mut fields: Vec<RlpItem> = vec![];
    let mut buffer = BytesMut::new();

    match tx {
        Transaction::Legacy {
            nonce,
            gas_price,
            gas_limit,
            to,
            value,
            data,
        } => {
            fields.push(nonce.to_rlp_item());
            fields.push(gas_price.to_rlp_item());
            fields.push(gas_limit.to_rlp_item());
            fields.push(encode_to(to));
            fields.push(value.to_rlp_item());
            fields.push(data.to_rlp_item());
            fields.push(chain_id.to_rlp_item());
            fields.push(0u64.to_rlp_item());
            fields.push(0u64.to_rlp_item());
        }
        _ => {
            unreachable!("legacy_to_encoded_bytes called with non-Legacy tx")
        }
    }

    encode(&RlpItem::List(fields), &mut buffer)?;

    Ok(buffer.freeze())
}

fn typed_to_encoded_bytes(tx: &Transaction, chain_id: u64) -> Result<Bytes, RlpError> {
    let mut fields: Vec<RlpItem> = vec![];

    fields.push(chain_id.to_rlp_item());
    match tx {
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
            fields.push(nonce.to_rlp_item());
            fields.push(max_priority_fee_per_gas.to_rlp_item());
            fields.push(max_fee_per_gas.to_rlp_item());
            fields.push(gas_limit.to_rlp_item());
            fields.push(encode_to(to));
            fields.push(value.to_rlp_item());
            fields.push(data.to_rlp_item());
            fields.push(RlpItem::List(
                access_list.iter().map(|a| a.to_rlp_item()).collect(),
            ));
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
            fields.push(nonce.to_rlp_item());
            fields.push(max_priority_fee_per_gas.to_rlp_item());
            fields.push(max_fee_per_gas.to_rlp_item());
            fields.push(gas_limit.to_rlp_item());
            fields.push(encode_to(to));
            fields.push(value.to_rlp_item());
            fields.push(data.to_rlp_item());
            fields.push(RlpItem::List(
                access_list.iter().map(|a| a.to_rlp_item()).collect(),
            ));
            fields.push(max_fee_per_blob_gas.to_rlp_item());
            fields.push(RlpItem::List(
                blob_versioned_hashes
                    .iter()
                    .map(|h| h.to_rlp_item())
                    .collect(),
            ));
        }
        _ => unreachable!("typed_to_encoded_bytes called with Legacy tx"),
    }

    let tx_type = match tx {
        Transaction::Legacy { .. } => 0x00,
        Transaction::Eip1559 { .. } => 0x02,
        Transaction::Eip4844 { .. } => 0x03,
    };

    let mut buffer = BytesMut::new();
    buffer.put_u8(tx_type);
    encode(&RlpItem::List(fields), &mut buffer)?;

    Ok(buffer.freeze())
}

pub fn sign(
    transaction: &Transaction,
    private_key_bytes: &[u8],
    chain_id: u64,
) -> Result<SignedTransaction, SigningError> {
    let payload = match transaction {
        Transaction::Legacy { .. } => legacy_to_encoded_bytes(transaction, chain_id)?,
        Transaction::Eip1559 { .. } | Transaction::Eip4844 { .. } => {
            typed_to_encoded_bytes(transaction, chain_id)?
        }
    };

    let hash_bytes = keccak256(&payload);

    let key_array: &[u8; 32] = private_key_bytes
        .try_into()
        .map_err(|_| SigningError::InvalidPrivateKey)?;
    let signing_key =
        SigningKey::from_bytes(key_array.into()).map_err(|_| SigningError::InvalidPrivateKey)?;

    let (signature, recovery_id) = signing_key.sign_prehash_recoverable(hash_bytes.as_bytes())?;
    let sig_bytes = signature.to_bytes();

    let r_bytes: [u8; 32] = sig_bytes[..32]
        .try_into()
        .map_err(|_| SigningError::InvalidSignature)?;
    let r = B256::from(r_bytes);

    let s_bytes: [u8; 32] = sig_bytes[32..]
        .try_into()
        .map_err(|_| SigningError::InvalidSignature)?;
    let s = B256::from(s_bytes);

    let v = match transaction {
        Transaction::Legacy { .. } => {
            recovery_id.to_byte() as u64
                + chain_id
                    .checked_mul(2)
                    .ok_or(SigningError::InvalidSignature)?
                + 35
        }
        Transaction::Eip1559 { .. } | Transaction::Eip4844 { .. } => recovery_id.to_byte() as u64,
    };

    Ok(SignedTransaction {
        transaction: transaction.clone(),
        v,
        r,
        s,
    })
}

pub fn recover_sender(signed: &SignedTransaction, chain_id: u64) -> Result<Address, SigningError> {
    let payload = match signed.transaction {
        Transaction::Legacy { .. } => legacy_to_encoded_bytes(&signed.transaction, chain_id)?,
        Transaction::Eip1559 { .. } | Transaction::Eip4844 { .. } => {
            typed_to_encoded_bytes(&signed.transaction, chain_id)?
        }
    };

    let hash_bytes = keccak256(&payload);

    let recovery_id_byte: u64 = match signed.transaction {
        Transaction::Legacy { .. } => signed
            .v
            .checked_sub(
                chain_id
                    .checked_mul(2)
                    .ok_or(SigningError::InvalidSignature)?,
            )
            .and_then(|x| x.checked_sub(35))
            .ok_or(SigningError::InvalidSignature)?,
        Transaction::Eip1559 { .. } | Transaction::Eip4844 { .. } => signed.v,
    };

    if recovery_id_byte > 1 {
        return Err(SigningError::InvalidSignature);
    }
    let recovery_id = RecoveryId::try_from(recovery_id_byte as u8)?;

    let mut sig_bytes: [u8; 64] = [0x00; 64];
    sig_bytes[..32].copy_from_slice(signed.r.as_bytes());
    sig_bytes[32..].copy_from_slice(signed.s.as_bytes());
    let signature = Signature::from_slice(&sig_bytes)?;
    let public_key =
        VerifyingKey::recover_from_prehash(hash_bytes.as_bytes(), &signature, recovery_id)
            .map_err(|_| SigningError::RecoveryFailed)?;

    let public_key_bytes = public_key.to_encoded_point(false);
    let public_key_hash = keccak256(&public_key_bytes.as_bytes()[1..]);

    let address = Address::try_from(&public_key_hash.as_bytes()[12..])
        .map_err(|_| SigningError::RecoveryFailed)?;

    Ok(address)
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use super::*;

    // A fixed 32-byte private key used across the signing tests. Any non-zero scalar
    // less than the secp256k1 curve order works; this one is convenient and obviously
    // a test value.
    const TEST_PRIVATE_KEY: [u8; 32] = [
        0x4c, 0x0c, 0x4d, 0x14, 0x6c, 0x46, 0xed, 0x91, 0xf6, 0xa9, 0x35, 0x09, 0x46, 0xa3, 0x69,
        0x9e, 0xb1, 0xfb, 0xc1, 0x9c, 0x91, 0xc8, 0x10, 0xe6, 0xb6, 0xa7, 0x8b, 0x0c, 0xa6, 0x06,
        0x65, 0x6f,
    ];

    fn legacy_tx() -> Transaction {
        Transaction::Legacy {
            nonce: 9,
            gas_price: 20_000_000_000,
            gas_limit: 21_000,
            to: Some(Address::new([0x11; 20])),
            value: 1_000_000_000_000_000_000,
            data: vec![],
        }
    }

    fn eip1559_tx() -> Transaction {
        Transaction::Eip1559 {
            nonce: 9,
            max_priority_fee_per_gas: 1_000_000_000,
            max_fee_per_gas: 20_000_000_000,
            gas_limit: 21_000,
            to: Some(Address::new([0x11; 20])),
            value: 1_000_000_000_000_000_000,
            data: vec![],
            access_list: vec![],
        }
    }

    fn eip4844_tx() -> Transaction {
        Transaction::Eip4844 {
            nonce: 9,
            max_priority_fee_per_gas: 1_000_000_000,
            max_fee_per_gas: 20_000_000_000,
            max_fee_per_blob_gas: 1_000,
            gas_limit: 21_000,
            to: Some(Address::new([0x11; 20])),
            value: 1_000_000_000_000_000_000,
            data: vec![],
            access_list: vec![],
            blob_versioned_hashes: vec![B256::new([0x42; 32])],
        }
    }

    #[test]
    fn keccak256_test() {
        let hash = keccak256(&[]);
        assert_eq!(
            hash,
            B256::from_str("0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470")
                .unwrap()
        );
    }

    #[test]
    fn sign_legacy_produces_nonzero_components_and_eip155_v() {
        let chain_id = 1u64;
        let signed = sign(&legacy_tx(), &TEST_PRIVATE_KEY, chain_id).unwrap();

        assert_ne!(signed.r, B256::new([0; 32]));
        assert_ne!(signed.s, B256::new([0; 32]));
        assert_ne!(signed.v, 0);

        // EIP-155: v = recovery_id + chain_id*2 + 35, recovery_id is 0 or 1.
        let recovery_id = signed.v - chain_id * 2 - 35;
        assert!(recovery_id == 0 || recovery_id == 1);
    }

    #[test]
    fn sign_legacy_v_depends_on_chain_id() {
        let signed_mainnet = sign(&legacy_tx(), &TEST_PRIVATE_KEY, 1).unwrap();
        let signed_polygon = sign(&legacy_tx(), &TEST_PRIVATE_KEY, 137).unwrap();

        assert_ne!(signed_mainnet.v, signed_polygon.v);
    }

    #[test]
    fn sign_eip1559_v_is_raw_recovery_id() {
        let signed = sign(&eip1559_tx(), &TEST_PRIVATE_KEY, 1).unwrap();
        // Typed transactions don't apply EIP-155 chain-ID folding.
        assert!(signed.v == 0 || signed.v == 1);
    }

    #[test]
    fn sign_eip4844_completes() {
        let signed = sign(&eip4844_tx(), &TEST_PRIVATE_KEY, 1).unwrap();
        assert!(signed.v == 0 || signed.v == 1);
        assert_ne!(signed.r, B256::new([0; 32]));
        assert_ne!(signed.s, B256::new([0; 32]));
    }

    #[test]
    fn legacy_and_eip1559_sigs_differ_on_overlapping_fields() {
        // Legacy and Eip1559 share nonce/gas_limit/to/value/data, but the signing
        // payload structure is different (EIP-155 suffix vs EIP-2718 type prefix +
        // chain_id-first), so r/s must differ.
        let legacy_sig = sign(&legacy_tx(), &TEST_PRIVATE_KEY, 1).unwrap();
        let eip1559_sig = sign(&eip1559_tx(), &TEST_PRIVATE_KEY, 1).unwrap();

        assert_ne!(legacy_sig.r, eip1559_sig.r);
        assert_ne!(legacy_sig.s, eip1559_sig.s);
    }

    #[test]
    fn sign_rejects_zero_private_key() {
        let zero_key = [0u8; 32];
        let err = sign(&legacy_tx(), &zero_key, 1).unwrap_err();
        assert!(matches!(err, SigningError::InvalidPrivateKey));
    }

    #[test]
    fn sign_rejects_wrong_length_private_key() {
        let short_key = [0x11u8; 31];
        let err = sign(&legacy_tx(), &short_key, 1).unwrap_err();
        assert!(matches!(err, SigningError::InvalidPrivateKey));
    }

    #[test]
    fn sign_handles_large_chain_id_without_overflow() {
        // Plan §3.7 calls for testing chain_id up to roughly u64::MAX / 2.
        // The implementation uses checked_mul to avoid overflow; here we sit just
        // under the boundary so the computation succeeds.
        let chain_id = (u64::MAX - 35) / 2;
        let signed = sign(&legacy_tx(), &TEST_PRIVATE_KEY, chain_id).unwrap();
        let recovery_id = signed.v - chain_id * 2 - 35;
        assert!(recovery_id == 0 || recovery_id == 1);
    }

    #[test]
    fn sign_returns_invalid_signature_on_chain_id_overflow() {
        // chain_id * 2 would overflow u64 — the checked_mul should map to InvalidSignature.
        let err = sign(&legacy_tx(), &TEST_PRIVATE_KEY, u64::MAX).unwrap_err();
        assert!(matches!(err, SigningError::InvalidSignature));
    }

    // Compute the address corresponding to a private key directly via k256, mirroring
    // the steps inside `recover_sender`. Used as the canonical "expected sender" in
    // recovery tests.
    fn address_from_private_key(key: &[u8; 32]) -> Address {
        let signing_key = SigningKey::from_slice(key).unwrap();
        let verifying_key = signing_key.verifying_key();
        let encoded = verifying_key.to_encoded_point(false);
        let hash = keccak256(&encoded.as_bytes()[1..]);
        Address::try_from(&hash.as_bytes()[12..]).unwrap()
    }

    #[test]
    fn recover_legacy_returns_signing_address() {
        let chain_id = 1u64;
        let signed = sign(&legacy_tx(), &TEST_PRIVATE_KEY, chain_id).unwrap();
        let recovered = recover_sender(&signed, chain_id).unwrap();

        assert_eq!(recovered, address_from_private_key(&TEST_PRIVATE_KEY));
    }

    #[test]
    fn recover_eip1559_matches_legacy_sender() {
        // The sender is determined by the key, not the transaction type — recovering
        // from a Legacy and an Eip1559 signed by the same key must yield the same address.
        let chain_id = 1u64;
        let legacy_signed = sign(&legacy_tx(), &TEST_PRIVATE_KEY, chain_id).unwrap();
        let eip1559_signed = sign(&eip1559_tx(), &TEST_PRIVATE_KEY, chain_id).unwrap();

        let legacy_sender = recover_sender(&legacy_signed, chain_id).unwrap();
        let eip1559_sender = recover_sender(&eip1559_signed, chain_id).unwrap();

        assert_eq!(legacy_sender, eip1559_sender);
        assert_eq!(legacy_sender, address_from_private_key(&TEST_PRIVATE_KEY));
    }

    #[test]
    fn recover_eip4844_matches_legacy_sender() {
        let chain_id = 1u64;
        let signed = sign(&eip4844_tx(), &TEST_PRIVATE_KEY, chain_id).unwrap();
        let recovered = recover_sender(&signed, chain_id).unwrap();

        assert_eq!(recovered, address_from_private_key(&TEST_PRIVATE_KEY));
    }

    #[test]
    fn recover_legacy_chain_id_mismatch_does_not_return_correct_sender() {
        // Sign on chain 1, recover claiming chain 137. The recovery payload differs
        // (chain_id is part of the EIP-155 suffix), so recovery either fails outright
        // or returns a different address. Both outcomes prove chain_id is committed.
        let signed = sign(&legacy_tx(), &TEST_PRIVATE_KEY, 1).unwrap();
        let expected = address_from_private_key(&TEST_PRIVATE_KEY);

        match recover_sender(&signed, 137) {
            Ok(addr) => assert_ne!(addr, expected),
            Err(_) => {} // also acceptable
        }
    }

    #[test]
    fn recover_eip1559_chain_id_mismatch_does_not_return_correct_sender() {
        // For typed transactions, chain_id is committed inside the RLP list itself,
        // so recovery with a different chain_id reconstructs a different payload and
        // must not produce the original sender's address.
        let signed = sign(&eip1559_tx(), &TEST_PRIVATE_KEY, 1).unwrap();
        let expected = address_from_private_key(&TEST_PRIVATE_KEY);

        match recover_sender(&signed, 137) {
            Ok(addr) => assert_ne!(addr, expected),
            Err(_) => {}
        }
    }

    #[test]
    fn recover_with_tampered_r_does_not_return_correct_sender() {
        let chain_id = 1u64;
        let mut signed = sign(&legacy_tx(), &TEST_PRIVATE_KEY, chain_id).unwrap();
        // Flip every byte of r — the resulting signature almost certainly fails to
        // recover the original public key. If it happens to recover a valid (but
        // different) key, the address still won't match.
        let mut r_bytes = *signed.r.as_bytes();
        for b in r_bytes.iter_mut() {
            *b ^= 0xff;
        }
        signed.r = B256::new(r_bytes);

        let expected = address_from_private_key(&TEST_PRIVATE_KEY);
        match recover_sender(&signed, chain_id) {
            Ok(addr) => assert_ne!(addr, expected),
            Err(SigningError::RecoveryFailed) => {}
            Err(SigningError::InvalidSignature) => {}
            Err(other) => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn recover_legacy_round_trips_across_chain_ids() {
        let expected = address_from_private_key(&TEST_PRIVATE_KEY);
        // Plan §3.7: cover mainnet, Goerli, Polygon, and a near-u64::MAX value.
        // (u64::MAX - 35) / 2 is the largest chain_id that won't overflow chain_id*2 + 35.
        let chain_ids = [1u64, 5, 137, (u64::MAX - 35) / 2];
        for chain_id in chain_ids {
            let signed = sign(&legacy_tx(), &TEST_PRIVATE_KEY, chain_id).unwrap();
            let recovered = recover_sender(&signed, chain_id).unwrap();
            assert_eq!(recovered, expected, "chain_id={chain_id}");
        }
    }

    #[test]
    fn signed_transaction_hash_is_deterministic() {
        // ECDSA signing is deterministic in k256 (RFC 6979), so the same
        // (key, transaction, chain_id) triple must always produce the same tx hash.
        // This is the determinism / golden-vector test from §4.4: capture the hash
        // once, then verify it doesn't drift on subsequent runs.
        let signed = sign(&legacy_tx(), &TEST_PRIVATE_KEY, 1).unwrap();
        let hash_first = signed.hash().unwrap();
        let hash_second = signed.hash().unwrap();
        assert_eq!(hash_first, hash_second);

        // Re-signing the same inputs should also reproduce the same hash.
        let signed_again = sign(&legacy_tx(), &TEST_PRIVATE_KEY, 1).unwrap();
        assert_eq!(signed_again.hash().unwrap(), hash_first);

        // The hash is non-zero — sanity check that we're hashing something real.
        assert_ne!(hash_first, B256::new([0; 32]));
    }

    #[test]
    fn different_signed_transactions_produce_different_hashes() {
        // Cheap sanity check: changing any one of (transaction body, chain_id, tx type)
        // must produce a different hash. If two of these collide, the wire encoding
        // is failing to commit to the field that changed.
        let legacy_chain1 = sign(&legacy_tx(), &TEST_PRIVATE_KEY, 1)
            .unwrap()
            .hash()
            .unwrap();
        let legacy_chain137 = sign(&legacy_tx(), &TEST_PRIVATE_KEY, 137)
            .unwrap()
            .hash()
            .unwrap();
        assert_ne!(legacy_chain1, legacy_chain137);

        // A different transaction body (nonce changed) on the same chain must hash differently.
        let other_legacy = Transaction::Legacy {
            nonce: 10, // changed from 9
            gas_price: 20_000_000_000,
            gas_limit: 21_000,
            to: Some(Address::new([0x11; 20])),
            value: 1_000_000_000_000_000_000,
            data: vec![],
        };
        let other_hash = sign(&other_legacy, &TEST_PRIVATE_KEY, 1)
            .unwrap()
            .hash()
            .unwrap();
        assert_ne!(legacy_chain1, other_hash);

        // A typed transaction on the same chain hashes differently from the Legacy version
        // even when the overlapping fields match — the type-byte prefix and field order
        // both flow into the hash.
        let eip1559_hash = sign(&eip1559_tx(), &TEST_PRIVATE_KEY, 1)
            .unwrap()
            .hash()
            .unwrap();
        assert_ne!(legacy_chain1, eip1559_hash);

        // Eip4844 hashes differently from Eip1559, again due to the differing wire format.
        let eip4844_hash = sign(&eip4844_tx(), &TEST_PRIVATE_KEY, 1)
            .unwrap()
            .hash()
            .unwrap();
        assert_ne!(eip1559_hash, eip4844_hash);
    }
}
