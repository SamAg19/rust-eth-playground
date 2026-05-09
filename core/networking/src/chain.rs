use rlp_codec::{RlpDecodable, RlpEncodable, RlpError, RlpItem};
use types::B256;

#[derive(Clone, Debug, PartialEq)]
pub struct BlockAnnouncement {
    pub hash: B256,
    pub number: u64,
}

impl RlpEncodable for BlockAnnouncement {
    fn to_rlp_item(&self) -> RlpItem {
        RlpItem::List(vec![self.hash.to_rlp_item(), self.number.to_rlp_item()])
    }
}

impl RlpDecodable for BlockAnnouncement {
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
                Ok(Self {
                    hash: B256::from_rlp_item(&x[0])?,
                    number: u64::from_rlp_item(&x[1])?,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use rlp_codec::{decode, encode};

    use super::*;

    #[test]
    fn block_announcement_roundtrips_through_rlp() {
        let original = BlockAnnouncement {
            hash: B256::from([0xab; 32]),
            number: 42,
        };

        let mut encoded = BytesMut::new();
        encode(&original.to_rlp_item(), &mut encoded).unwrap();
        let encoded = encoded.freeze();
        let (decoded_item, remaining) = decode(&encoded).unwrap();
        assert!(remaining.is_empty());
        let decoded = BlockAnnouncement::from_rlp_item(&decoded_item).unwrap();

        assert_eq!(decoded, original);
    }
}
