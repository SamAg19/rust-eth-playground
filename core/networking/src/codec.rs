use bytes::{Buf, BufMut, BytesMut};
use rlp_codec::{RlpDecodable, RlpEncodable, RlpError, RlpItem, decode, encode};
use tokio_util::codec::{Decoder, Encoder};
use types::{B256, Transaction};

use crate::{
    error::NetworkError,
    message::{MSG_GET_BLOCK_HEADERS, MSG_PING, MSG_PONG, MSG_STATUS, MSG_TRANSACTIONS, Message},
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
            _ => Err(NetworkError::Decode(RlpError::UnexpectedType(tag))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::Address;

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
            _ => panic!("variant mismatch"),
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
