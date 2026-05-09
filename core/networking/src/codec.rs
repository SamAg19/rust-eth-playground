use bytes::{Buf, BufMut, BytesMut};
use rlp_codec::{RlpDecodable, RlpEncodable, RlpError, RlpItem, decode, encode};
use std::str;
use tokio_util::codec::{Decoder, Encoder};
use types::{B256, Block, Header, Transaction};

use crate::{
    chain::BlockAnnouncement,
    constants::{
        MSG_BLOCK_HEADERS, MSG_DISCONNECT, MSG_GET_BLOCK_HEADERS, MSG_NEW_BLOCK,
        MSG_NEW_BLOCK_HASHES, MSG_PING, MSG_PONG, MSG_STATUS, MSG_TRANSACTIONS,
    },
    error::NetworkError,
    message::Message,
};

pub struct EthCodec();

const MAX_FRAME_SIZE: usize = 10 * 1024 * 1024;

impl Encoder<Message> for EthCodec {
    type Error = NetworkError;
    fn encode(&mut self, item: Message, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let message_item = item.to_rlp_item();
        match message_item {
            RlpItem::Bytes(_) => Err(NetworkError::Encode(RlpError::UnexpectedType(0x80))),
            RlpItem::List(x) => {
                let tag = u64::from_rlp_item(&x[0])? as u8;
                let mut payload = BytesMut::new();
                encode(&RlpItem::List(x[1..].to_vec()), &mut payload)
                    .map_err(NetworkError::Encode)?;
                let payload_bytes = payload.freeze();
                let frame_length = 1 + payload_bytes.len();

                if frame_length > MAX_FRAME_SIZE {
                    return Err(NetworkError::FrameTooLarge(frame_length));
                }
                dst.reserve(4 + frame_length);
                dst.put_u32(frame_length as u32);
                dst.put_u8(tag);
                dst.put_slice(&payload_bytes);
                Ok(())
            }
        }
    }
}

impl Decoder for EthCodec {
    type Error = NetworkError;
    type Item = Message;
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            return Ok(None);
        }

        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&src[..4]);
        let frame_length = u32::from_be_bytes(len_bytes) as usize;

        if src.len() < 4 + frame_length {
            return Ok(None);
        }

        src.advance(4);
        let frame = src.split_to(frame_length);
        let tag = frame[0];
        let payload = &frame[1..];
        let (decoded_payload, _) = decode(payload).map_err(NetworkError::Decode)?;
        let fields = match decoded_payload {
            RlpItem::List(f) => f,
            RlpItem::Bytes(_) => return Err(NetworkError::Decode(RlpError::UnexpectedType(0x80))),
        };
        match tag {
            MSG_PING => {
                if !fields.is_empty() {
                    return Err(NetworkError::Decode(RlpError::InvalidLength(fields.len())));
                }
                Ok(Some(Message::Ping))
            }
            MSG_PONG => {
                if !fields.is_empty() {
                    return Err(NetworkError::Decode(RlpError::InvalidLength(fields.len())));
                }
                Ok(Some(Message::Pong))
            }
            MSG_STATUS => {
                if fields.len() != 3 {
                    return Err(NetworkError::Decode(RlpError::InvalidLength(fields.len())));
                }
                Ok(Some(Message::Status {
                    chain_id: u64::from_rlp_item(&fields[0])?,
                    head_hash: B256::from_rlp_item(&fields[1])?,
                    total_difficulty: u128::from_rlp_item(&fields[2])?,
                }))
            }
            MSG_TRANSACTIONS => {
                if fields.len() != 1 {
                    return Err(NetworkError::Decode(RlpError::InvalidLength(fields.len())));
                }
                let mut txs = vec![];
                match &fields[0] {
                    RlpItem::Bytes(_) => {
                        return Err(NetworkError::Decode(RlpError::UnexpectedType(0x80)));
                    }
                    RlpItem::List(x) => {
                        for x_item in x {
                            txs.push(Transaction::from_rlp_item(x_item)?);
                        }
                    }
                }
                Ok(Some(Message::Transactions { txs }))
            }
            MSG_GET_BLOCK_HEADERS => {
                if fields.len() != 2 {
                    return Err(NetworkError::Decode(RlpError::InvalidLength(fields.len())));
                }
                Ok(Some(Message::GetBlockHeaders {
                    start_hash: B256::from_rlp_item(&fields[0])?,
                    count: u64::from_rlp_item(&fields[1])?,
                }))
            }
            MSG_NEW_BLOCK => {
                if fields.len() != 2 {
                    return Err(NetworkError::Decode(RlpError::InvalidLength(fields.len())));
                }
                Ok(Some(Message::NewBlock {
                    block: Block::from_rlp_item(&fields[0])?,
                    td: u128::from_rlp_item(&fields[1])?,
                }))
            }
            MSG_NEW_BLOCK_HASHES => {
                if fields.len() != 1 {
                    return Err(NetworkError::Decode(RlpError::InvalidLength(fields.len())));
                }
                let mut blocks = vec![];
                match &fields[0] {
                    RlpItem::Bytes(_) => {
                        return Err(NetworkError::Decode(RlpError::UnexpectedType(0x80)));
                    }
                    RlpItem::List(x) => {
                        for x_item in x {
                            blocks.push(BlockAnnouncement::from_rlp_item(x_item)?);
                        }
                    }
                }

                Ok(Some(Message::NewBlockHashes { new_blocks: blocks }))
            }
            MSG_BLOCK_HEADERS => {
                if fields.len() != 1 {
                    return Err(NetworkError::Decode(RlpError::InvalidLength(fields.len())));
                }
                let mut headers = vec![];
                match &fields[0] {
                    RlpItem::Bytes(_) => {
                        return Err(NetworkError::Decode(RlpError::UnexpectedType(0x80)));
                    }
                    RlpItem::List(x) => {
                        for x_item in x {
                            headers.push(Header::from_rlp_item(x_item)?);
                        }
                    }
                }

                Ok(Some(Message::BlockHeaders { headers }))
            }
            MSG_DISCONNECT => {
                if fields.len() != 1 {
                    return Err(NetworkError::Decode(RlpError::InvalidLength(fields.len())));
                }
                let reason = match &fields[0] {
                    RlpItem::List(_) => {
                        return Err(NetworkError::Decode(RlpError::UnexpectedType(0xc0)));
                    }
                    RlpItem::Bytes(bytes) => str::from_utf8(bytes)
                        .map_err(|_| NetworkError::Decode(RlpError::InvalidString))?,
                };

                Ok(Some(Message::Disconnect {
                    reason: reason.to_string(),
                }))
            }
            _ => Err(NetworkError::Decode(RlpError::UnexpectedType(tag))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::{Address, SignedTransaction};

    fn roundtrip(msg: Message) -> Message {
        let mut codec = EthCodec();
        let mut buf = BytesMut::new();
        codec.encode(msg, &mut buf).unwrap();
        codec
            .decode(&mut buf)
            .unwrap()
            .expect("frame should decode")
    }

    fn assert_message_eq(a: &Message, b: &Message) {
        match (a, b) {
            (Message::Ping, Message::Ping) => {}
            (Message::Pong, Message::Pong) => {}
            (
                Message::Status {
                    chain_id: c1,
                    head_hash: h1,
                    total_difficulty: t1,
                },
                Message::Status {
                    chain_id: c2,
                    head_hash: h2,
                    total_difficulty: t2,
                },
            ) => {
                assert_eq!(c1, c2);
                assert_eq!(h1, h2);
                assert_eq!(t1, t2);
            }
            (Message::Transactions { txs: t1 }, Message::Transactions { txs: t2 }) => {
                assert_eq!(t1.len(), t2.len());
            }
            (
                Message::GetBlockHeaders {
                    start_hash: s1,
                    count: c1,
                },
                Message::GetBlockHeaders {
                    start_hash: s2,
                    count: c2,
                },
            ) => {
                assert_eq!(s1, s2);
                assert_eq!(c1, c2);
            }
            (
                Message::NewBlock { block: b1, td: td1 },
                Message::NewBlock { block: b2, td: td2 },
            ) => {
                assert_eq!(b1, b2);
                assert_eq!(td1, td2);
            }
            (
                Message::NewBlockHashes { new_blocks: b1 },
                Message::NewBlockHashes { new_blocks: b2 },
            ) => {
                assert_eq!(b1, b2);
            }
            (Message::BlockHeaders { headers: h1 }, Message::BlockHeaders { headers: h2 }) => {
                assert_eq!(h1, h2);
            }
            (Message::Disconnect { reason: r1 }, Message::Disconnect { reason: r2 }) => {
                assert_eq!(r1, r2);
            }
            _ => panic!("variant mismatch"),
        }
    }

    fn test_header(number: u64, byte: u8) -> Header {
        Header {
            parent_hash: B256::from([byte; 32]),
            beneficiary: Address::from([byte; 20]),
            state_root: B256::from([byte.wrapping_add(1); 32]),
            transactions_root: B256::from([byte.wrapping_add(2); 32]),
            gas_limit: 30_000_000,
            gas_used: number * 21_000,
            timestamp: 1_700_000_000 + number * 12,
            number,
        }
    }

    #[test]
    fn test_roundtrip_ping() {
        assert_message_eq(&roundtrip(Message::Ping), &Message::Ping);
    }

    #[test]
    fn test_roundtrip_pong() {
        assert_message_eq(&roundtrip(Message::Pong), &Message::Pong);
    }

    #[test]
    fn test_roundtrip_status() {
        let msg = Message::Status {
            chain_id: 1,
            head_hash: B256::from([0xaa; 32]),
            total_difficulty: 1_000_000_000_000,
        };
        assert_message_eq(&roundtrip(msg.clone()), &msg);
    }

    #[test]
    fn test_roundtrip_transactions() {
        let tx = Transaction::Legacy {
            nonce: 1,
            gas_price: 100,
            gas_limit: 21_000,
            to: Some(Address::from([0xbb; 20])),
            value: 42,
            data: vec![0x01, 0x02, 0x03],
        };
        let msg = Message::Transactions { txs: vec![tx] };
        assert_message_eq(&roundtrip(msg.clone()), &msg);
    }

    #[test]
    fn test_roundtrip_get_block_headers() {
        let msg = Message::GetBlockHeaders {
            start_hash: B256::from([0xcc; 32]),
            count: 100,
        };
        assert_message_eq(&roundtrip(msg.clone()), &msg);
    }

    #[test]
    fn test_roundtrip_block_headers_with_three_headers() {
        let msg = Message::BlockHeaders {
            headers: vec![
                test_header(1, 0x11),
                test_header(2, 0x22),
                test_header(3, 0x33),
            ],
        };

        let mut codec = EthCodec();
        let mut buf = BytesMut::new();
        codec.encode(msg.clone(), &mut buf).unwrap();

        assert_eq!(buf[4], MSG_BLOCK_HEADERS);

        let decoded = codec
            .decode(&mut buf)
            .unwrap()
            .expect("frame should decode");
        assert_message_eq(&decoded, &msg);
    }

    #[test]
    fn test_roundtrip_block_headers_empty() {
        let msg = Message::BlockHeaders { headers: vec![] };

        let mut codec = EthCodec();
        let mut buf = BytesMut::new();
        codec.encode(msg, &mut buf).unwrap();

        assert_eq!(buf[4], MSG_BLOCK_HEADERS);

        let decoded = codec
            .decode(&mut buf)
            .unwrap()
            .expect("frame should decode");
        match decoded {
            Message::BlockHeaders { headers } => {
                assert!(headers.is_empty());
            }
            _ => panic!("expected BlockHeaders"),
        }
    }

    #[test]
    fn test_message_type_tags_are_unique() {
        let tags = [
            MSG_PING,
            MSG_PONG,
            MSG_STATUS,
            MSG_TRANSACTIONS,
            MSG_GET_BLOCK_HEADERS,
            MSG_NEW_BLOCK,
            MSG_NEW_BLOCK_HASHES,
            MSG_BLOCK_HEADERS,
            MSG_DISCONNECT,
        ];
        let mut deduped = tags.to_vec();

        deduped.sort();
        deduped.dedup();

        assert_eq!(deduped.len(), tags.len());
    }

    #[test]
    fn test_roundtrip_new_block() {
        let header = Header {
            parent_hash: B256::from([0x11; 32]),
            beneficiary: Address::from([0x22; 20]),
            state_root: B256::from([0x33; 32]),
            transactions_root: B256::from([0x44; 32]),
            gas_limit: 30_000_000,
            gas_used: 42_000,
            timestamp: 1_700_000_000,
            number: 7,
        };
        let transactions = vec![
            SignedTransaction {
                transaction: Transaction::Legacy {
                    nonce: 0,
                    gas_price: 1_000_000_000,
                    gas_limit: 21_000,
                    to: Some(Address::from([0xbb; 20])),
                    value: 100,
                    data: vec![],
                },
                v: 27,
                r: B256::from([0x55; 32]),
                s: B256::from([0x66; 32]),
            },
            SignedTransaction {
                transaction: Transaction::Eip1559 {
                    nonce: 1,
                    max_priority_fee_per_gas: 1_000_000,
                    max_fee_per_gas: 2_000_000_000,
                    gas_limit: 21_000,
                    to: Some(Address::from([0xcc; 20])),
                    value: 200,
                    data: vec![0x01, 0x02],
                    access_list: vec![],
                },
                v: 1,
                r: B256::from([0x77; 32]),
                s: B256::from([0x88; 32]),
            },
        ];
        let block = Block {
            header: header.clone(),
            transactions,
        };
        let msg = Message::NewBlock {
            block,
            td: 1_000_000,
        };

        let mut codec = EthCodec();
        let mut buf = BytesMut::new();
        codec.encode(msg, &mut buf).unwrap();

        assert_eq!(buf[4], MSG_NEW_BLOCK);

        let decoded = codec
            .decode(&mut buf)
            .unwrap()
            .expect("frame should decode");
        match decoded {
            Message::NewBlock { block, td } => {
                assert_eq!(block.header, header);
                assert_eq!(block.transactions.len(), 2);
                assert_eq!(td, 1_000_000);
            }
            _ => panic!("expected NewBlock"),
        }
    }

    #[test]
    fn test_roundtrip_disconnect() {
        let msg = Message::Disconnect {
            reason: "chain id mismatch".to_string(),
        };

        let mut codec = EthCodec();
        let mut buf = BytesMut::new();
        codec.encode(msg.clone(), &mut buf).unwrap();

        assert_eq!(buf[4], MSG_DISCONNECT);

        let decoded = codec
            .decode(&mut buf)
            .unwrap()
            .expect("frame should decode");
        assert_message_eq(&decoded, &msg);
    }

    #[test]
    fn test_roundtrip_new_block_hashes_with_three_announcements() {
        let msg = Message::NewBlockHashes {
            new_blocks: vec![
                BlockAnnouncement {
                    hash: B256::from([0x11; 32]),
                    number: 1,
                },
                BlockAnnouncement {
                    hash: B256::from([0x22; 32]),
                    number: 2,
                },
                BlockAnnouncement {
                    hash: B256::from([0x33; 32]),
                    number: 3,
                },
            ],
        };

        let mut codec = EthCodec();
        let mut buf = BytesMut::new();
        codec.encode(msg.clone(), &mut buf).unwrap();

        assert_eq!(buf[4], MSG_NEW_BLOCK_HASHES);

        let decoded = codec
            .decode(&mut buf)
            .unwrap()
            .expect("frame should decode");
        assert_message_eq(&decoded, &msg);
    }

    #[test]
    fn test_roundtrip_new_block_hashes_empty() {
        let msg = Message::NewBlockHashes { new_blocks: vec![] };

        let mut codec = EthCodec();
        let mut buf = BytesMut::new();
        codec.encode(msg.clone(), &mut buf).unwrap();

        assert_eq!(buf[4], MSG_NEW_BLOCK_HASHES);

        let decoded = codec
            .decode(&mut buf)
            .unwrap()
            .expect("frame should decode");
        match decoded {
            Message::NewBlockHashes { new_blocks } => {
                assert!(new_blocks.is_empty());
            }
            _ => panic!("expected NewBlockHashes"),
        }
    }

    #[test]
    fn test_decode_exact_buffer() {
        let mut codec = EthCodec();
        let mut buf = BytesMut::new();
        codec.encode(Message::Ping, &mut buf).unwrap();

        let decoded = codec.decode(&mut buf).unwrap().expect("should decode");
        assert_message_eq(&decoded, &Message::Ping);
        assert_eq!(buf.len(), 0, "buffer should be fully consumed");
    }

    #[test]
    fn test_decode_insufficient_length_prefix() {
        let mut codec = EthCodec();
        let mut buf = BytesMut::from(&[0x00, 0x00, 0x00][..]);
        assert!(codec.decode(&mut buf).unwrap().is_none());
        assert_eq!(buf.len(), 3, "buffer should be unchanged");
    }

    #[test]
    fn test_decode_partial_payload() {
        let mut codec = EthCodec();
        let mut full = BytesMut::new();
        codec.encode(Message::Ping, &mut full).unwrap();

        let truncated = &full[..full.len() - 1];
        let mut buf = BytesMut::from(truncated);
        let buf_len_before = buf.len();

        assert!(codec.decode(&mut buf).unwrap().is_none());
        assert_eq!(
            buf.len(),
            buf_len_before,
            "buffer should be unchanged on partial read"
        );
    }

    #[test]
    fn test_encode_frame_too_large() {
        let mut codec = EthCodec();
        let mut buf = BytesMut::new();
        let huge_tx = Transaction::Legacy {
            nonce: 0,
            gas_price: 0,
            gas_limit: 0,
            to: None,
            value: 0,
            data: vec![0x00; MAX_FRAME_SIZE + 1],
        };
        let msg = Message::Transactions { txs: vec![huge_tx] };

        let err = codec.encode(msg, &mut buf).unwrap_err();
        assert!(matches!(err, NetworkError::FrameTooLarge(_)));
    }

    #[test]
    fn test_decode_streaming_concat() {
        let mut codec = EthCodec();
        let mut buf = BytesMut::new();
        codec.encode(Message::Ping, &mut buf).unwrap();
        codec.encode(Message::Pong, &mut buf).unwrap();

        let first = codec.decode(&mut buf).unwrap().expect("first decodes");
        let second = codec.decode(&mut buf).unwrap().expect("second decodes");
        assert_message_eq(&first, &Message::Ping);
        assert_message_eq(&second, &Message::Pong);
        assert_eq!(buf.len(), 0);
    }
}
