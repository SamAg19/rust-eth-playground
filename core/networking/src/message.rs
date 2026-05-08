use rlp_codec::{RlpDecodable, RlpEncodable, RlpError, RlpItem};
use types::{B256, Transaction};

#[derive(Debug, Clone)]
pub enum Message {
    Ping,
    Pong,
    Status {
        chain_id: u64,
        head_hash: B256,
        total_difficulty: u128,
    },
    Transactions {
        txs: Vec<Transaction>,
    },
    GetBlockHeaders {
        start_hash: B256,
        count: u64,
    },
}

pub const MSG_PING: u8 = 0x00;
pub const MSG_PONG: u8 = 0x01;
pub const MSG_STATUS: u8 = 0x02;
pub const MSG_TRANSACTIONS: u8 = 0x03;
pub const MSG_GET_BLOCK_HEADERS: u8 = 0x04;

impl RlpEncodable for Message {
    fn to_rlp_item(&self) -> RlpItem {
        match self {
            Message::Ping => {
                let tag = MSG_PING as u64;
                RlpItem::List(vec![tag.to_rlp_item()])
            }
            Message::Pong => {
                let tag = MSG_PONG as u64;
                RlpItem::List(vec![tag.to_rlp_item()])
            }
            Message::Status {
                chain_id,
                head_hash,
                total_difficulty,
            } => {
                let tag = MSG_STATUS as u64;

                RlpItem::List(vec![
                    tag.to_rlp_item(),
                    chain_id.to_rlp_item(),
                    head_hash.to_rlp_item(),
                    total_difficulty.to_rlp_item(),
                ])
            }
            Message::Transactions { txs } => {
                let mut fields = vec![];
                let tag = MSG_TRANSACTIONS as u64;
                fields.push(tag.to_rlp_item());
                let mut item_vector = vec![];
                for tx in txs {
                    item_vector.push(tx.to_rlp_item());
                }
                fields.push(RlpItem::List(item_vector));

                RlpItem::List(fields)
            }
            Message::GetBlockHeaders { start_hash, count } => {
                let mut fields = vec![];
                let tag = MSG_GET_BLOCK_HEADERS as u64;
                fields.push(tag.to_rlp_item());
                fields.push(start_hash.to_rlp_item());
                fields.push(count.to_rlp_item());

                RlpItem::List(fields)
            }
        }
    }
}

impl RlpDecodable for Message {
    fn from_rlp_item(item: &RlpItem) -> Result<Self, rlp_codec::RlpError>
    where
        Self: Sized,
    {
        match item {
            RlpItem::Bytes(_) => Err(RlpError::UnexpectedType(0x80)),
            RlpItem::List(x) => {
                if x.is_empty() {
                    return Err(RlpError::InvalidLength(0));
                }

                let tag = u64::from_rlp_item(&x[0])? as u8;

                match tag {
                    MSG_PING => {
                        if x.len() != 1 {
                            return Err(RlpError::InvalidLength(x.len()));
                        }
                        Ok(Message::Ping)
                    }
                    MSG_PONG => {
                        if x.len() != 1 {
                            return Err(RlpError::InvalidLength(x.len()));
                        }
                        Ok(Message::Pong)
                    }
                    MSG_STATUS => {
                        if x.len() != 4 {
                            return Err(RlpError::InvalidLength(x.len()));
                        }

                        Ok(Message::Status {
                            chain_id: u64::from_rlp_item(&x[1])?,
                            head_hash: B256::from_rlp_item(&x[2])?,
                            total_difficulty: u128::from_rlp_item(&x[3])?,
                        })
                    }
                    MSG_TRANSACTIONS => {
                        if x.len() != 2 {
                            return Err(RlpError::InvalidLength(x.len()));
                        }
                        let mut txs = vec![];
                        match &x[1] {
                            RlpItem::Bytes(_) => return Err(RlpError::UnexpectedType(0x80)),
                            RlpItem::List(x) => {
                                for x_item in x {
                                    txs.push(Transaction::from_rlp_item(x_item)?);
                                }
                            }
                        }
                        Ok(Message::Transactions { txs })
                    }
                    MSG_GET_BLOCK_HEADERS => {
                        if x.len() != 3 {
                            return Err(RlpError::InvalidLength(x.len()));
                        }

                        Ok(Message::GetBlockHeaders {
                            start_hash: B256::from_rlp_item(&x[1])?,
                            count: u64::from_rlp_item(&x[2])?,
                        })
                    }
                    _ => Err(RlpError::UnexpectedType(tag)),
                }
            }
        }
    }
}
