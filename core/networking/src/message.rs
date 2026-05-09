use bytes::Bytes;
use rlp_codec::{RlpDecodable, RlpEncodable, RlpError, RlpItem};
use std::str;
use types::{B256, Block, Header, Transaction};

use crate::{
    chain::BlockAnnouncement,
    constants::{
        MSG_BLOCK_HEADERS, MSG_DISCONNECT, MSG_GET_BLOCK_HEADERS, MSG_NEW_BLOCK,
        MSG_NEW_BLOCK_HASHES, MSG_PING, MSG_PONG, MSG_STATUS, MSG_TRANSACTIONS,
    },
};

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
    NewBlock {
        block: Block,
        td: u128,
    },
    NewBlockHashes {
        new_blocks: Vec<BlockAnnouncement>,
    },
    BlockHeaders {
        headers: Vec<Header>,
    },
    Disconnect {
        reason: String,
    },
}

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
            Message::NewBlock { block, td } => {
                let tag = MSG_NEW_BLOCK as u64;
                RlpItem::List(vec![
                    tag.to_rlp_item(),
                    block.to_rlp_item(),
                    td.to_rlp_item(),
                ])
            }
            Message::NewBlockHashes { new_blocks } => {
                let tag = MSG_NEW_BLOCK_HASHES as u64;
                let mut block_vector = vec![];
                for new_block in new_blocks {
                    block_vector.push(new_block.to_rlp_item());
                }
                RlpItem::List(vec![tag.to_rlp_item(), RlpItem::List(block_vector)])
            }
            Message::BlockHeaders { headers } => {
                let tag = MSG_BLOCK_HEADERS as u64;
                let mut headers_vector = vec![];
                for header in headers {
                    headers_vector.push(header.to_rlp_item());
                }
                RlpItem::List(vec![tag.to_rlp_item(), RlpItem::List(headers_vector)])
            }
            Message::Disconnect { reason } => {
                let tag = MSG_DISCONNECT as u64;

                RlpItem::List(vec![
                    tag.to_rlp_item(),
                    RlpItem::Bytes(Bytes::from(reason.clone())),
                ])
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
                    MSG_NEW_BLOCK => {
                        if x.len() != 3 {
                            return Err(RlpError::InvalidLength(x.len()));
                        }

                        Ok(Message::NewBlock {
                            block: Block::from_rlp_item(&x[1])?,
                            td: u128::from_rlp_item(&x[2])?,
                        })
                    }
                    MSG_NEW_BLOCK_HASHES => {
                        if x.len() != 2 {
                            return Err(RlpError::InvalidLength(x.len()));
                        }

                        let mut new_blocks = vec![];
                        match &x[1] {
                            RlpItem::Bytes(_) => return Err(RlpError::UnexpectedType(0x80)),
                            RlpItem::List(blocks_rlp) => {
                                for block in blocks_rlp {
                                    new_blocks.push(BlockAnnouncement::from_rlp_item(block)?);
                                }
                            }
                        }

                        Ok(Message::NewBlockHashes { new_blocks })
                    }
                    MSG_BLOCK_HEADERS => {
                        if x.len() != 2 {
                            return Err(RlpError::InvalidLength(x.len()));
                        }

                        let mut headers = vec![];
                        match &x[1] {
                            RlpItem::Bytes(_) => return Err(RlpError::UnexpectedType(0x80)),
                            RlpItem::List(blocks_rlp) => {
                                for block in blocks_rlp {
                                    headers.push(Header::from_rlp_item(block)?);
                                }
                            }
                        }

                        Ok(Message::BlockHeaders { headers })
                    }
                    MSG_DISCONNECT => {
                        if x.len() != 2 {
                            return Err(RlpError::InvalidLength(x.len()));
                        }

                        let reason = match &x[1] {
                            RlpItem::List(_) => return Err(RlpError::UnexpectedType(0xc0)),
                            RlpItem::Bytes(b) => {
                                let bytes_to_string = str::from_utf8(b);
                                match bytes_to_string {
                                    Err(_) => return Err(RlpError::InvalidString),
                                    Ok(s) => s,
                                }
                            }
                        };
                        Ok(Message::Disconnect {
                            reason: reason.to_string(),
                        })
                    }
                    _ => Err(RlpError::UnexpectedType(tag)),
                }
            }
        }
    }
}
