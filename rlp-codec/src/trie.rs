use std::vec;

use crate::{
    RlpEncodable, RlpError, RlpItem, encode, encoder::add_rlp_list_prefix, signing::keccak256,
};
use bytes::{BufMut, BytesMut};
use thiserror::Error;
use types::B256;

enum TrieNode {
    Leaf {
        remaining_nibbles: Vec<u8>,
        value: Vec<u8>,
    },
    Extension {
        shared_nibbles: Vec<u8>,
        child: Box<TrieNode>,
    },
    Branch {
        children: Box<[Option<TrieNode>; 16]>,
        value: Option<Vec<u8>>,
    },
}

enum NodeRef {
    Inline(Vec<u8>),
    Hash(B256),
}

#[derive(Debug, Error)]
pub enum TrieError {
    #[error("Key is empty")]
    InvalidKey,
    #[error[transparent]]
    Encode(#[from] RlpError),
}

pub struct MerkleTrie {
    root: Option<TrieNode>,
}

fn bytes_to_nibbles(bytes: &[u8]) -> Vec<u8> {
    let mut nibbles = vec![];

    for byte in bytes {
        let high_nibble = byte >> 4;
        let low_nibble = byte & 0x0f;
        nibbles.push(high_nibble);
        nibbles.push(low_nibble);
    }

    nibbles
}

fn encode_node(node: &TrieNode) -> Result<NodeRef, TrieError> {
    let mut buffer = BytesMut::new();

    match node {
        TrieNode::Leaf {
            remaining_nibbles,
            value,
        } => {
            let remaining_nibbles_rlp = hp_encode(remaining_nibbles, true).to_rlp_item();
            let value_rlp = value.to_rlp_item();
            let item = RlpItem::List(vec![remaining_nibbles_rlp, value_rlp]);
            encode(&item, &mut buffer)?;
        }

        TrieNode::Branch { children, value } => {
            let mut encoded_items = BytesMut::new();
            for child in children.iter() {
                match child {
                    None => encode(&vec![].to_rlp_item(), &mut encoded_items)?,
                    Some(node) => match encode_node(node)? {
                        NodeRef::Inline(x) => {
                            encoded_items.put_slice(&x);
                        }
                        NodeRef::Hash(x) => encode(&x.to_rlp_item(), &mut encoded_items)?,
                    },
                }
            }
            match value {
                None => encode(&vec![].to_rlp_item(), &mut encoded_items)?,
                Some(x) => encode(&x.to_rlp_item(), &mut encoded_items)?,
            }
            add_rlp_list_prefix(&mut buffer, encoded_items.len());

            buffer.put_slice(&encoded_items);
        }
        TrieNode::Extension {
            shared_nibbles,
            child,
        } => {
            let shared_nibbles_rlp = hp_encode(shared_nibbles, false).to_rlp_item();
            match encode_node(child)? {
                NodeRef::Inline(x) => {
                    let mut encoded_items = BytesMut::new();

                    encode(&shared_nibbles_rlp, &mut encoded_items)?;
                    encoded_items.put_slice(&x);

                    add_rlp_list_prefix(&mut buffer, encoded_items.len());

                    buffer.put_slice(&encoded_items);
                }
                NodeRef::Hash(x) => {
                    let child_rlp = x.to_rlp_item();
                    let item = RlpItem::List(vec![shared_nibbles_rlp, child_rlp]);
                    encode(&item, &mut buffer)?;
                }
            }
        }
    }

    let bytes = buffer.freeze();
    if bytes.len() < 32 {
        return Ok(NodeRef::Inline(bytes.to_vec()));
    }
    Ok(NodeRef::Hash(keccak256(&bytes)))
}

fn hp_encode(nibbles: &[u8], is_leaf: bool) -> Vec<u8> {
    let mut hp_encoding = vec![];
    match is_leaf {
        false => match nibbles.len().is_multiple_of(2) {
            true => {
                hp_encoding.push(0x0 << 4);
                for pair in nibbles.chunks(2) {
                    hp_encoding.push((pair[0] << 4) | pair[1]);
                }
            }
            false => {
                hp_encoding.push(0x1 << 4 | nibbles[0]);
                for pair in nibbles[1..].chunks(2) {
                    hp_encoding.push((pair[0] << 4) | pair[1]);
                }
            }
        },
        true => match nibbles.len().is_multiple_of(2) {
            true => {
                hp_encoding.push(0x2 << 4);
                for pair in nibbles.chunks(2) {
                    hp_encoding.push((pair[0] << 4) | pair[1]);
                }
            }
            false => {
                hp_encoding.push(0x3 << 4 | nibbles[0]);
                for pair in nibbles[1..].chunks(2) {
                    hp_encoding.push((pair[0] << 4) | pair[1]);
                }
            }
        },
    }
    hp_encoding
}

fn recursive_insert(slot: &mut Option<TrieNode>, nibbles: &[u8], new_value: &Vec<u8>) {
    match slot.take() {
        None => {
            *slot = Some(TrieNode::Leaf {
                remaining_nibbles: nibbles.to_vec(),
                value: new_value.to_vec(),
            });
        }
        Some(node) => match node {
            TrieNode::Leaf {
                remaining_nibbles,
                value: old_value,
            } => {
                if *nibbles == remaining_nibbles {
                    *slot = Some(TrieNode::Leaf {
                        remaining_nibbles: nibbles.to_vec(),
                        value: new_value.clone(),
                    });
                    return;
                }

                let shared_prefix_len = remaining_nibbles
                    .iter()
                    .zip(nibbles.iter())
                    .take_while(|(a, b)| a == b)
                    .count();

                if remaining_nibbles.len() == shared_prefix_len {
                    let child = nibbles[shared_prefix_len] as usize;
                    let new_leaf = TrieNode::Leaf {
                        remaining_nibbles: nibbles[shared_prefix_len + 1..].to_vec(),
                        value: new_value.to_vec(),
                    };
                    let mut children: [Option<TrieNode>; 16] = [const { None }; 16];
                    children[child] = Some(new_leaf);
                    *slot = Some(TrieNode::Branch {
                        children: Box::new(children),
                        value: Some(old_value.to_vec()),
                    })
                } else if nibbles.len() == shared_prefix_len {
                    let child = remaining_nibbles[shared_prefix_len] as usize;
                    let new_leaf = TrieNode::Leaf {
                        remaining_nibbles: remaining_nibbles[shared_prefix_len + 1..].to_vec(),
                        value: old_value.to_vec(),
                    };
                    let mut children: [Option<TrieNode>; 16] = [const { None }; 16];
                    children[child] = Some(new_leaf);
                    *slot = Some(TrieNode::Branch {
                        children: Box::new(children),
                        value: Some(new_value.to_vec()),
                    })
                } else {
                    let old_child = remaining_nibbles[shared_prefix_len] as usize;
                    let old_leaf = TrieNode::Leaf {
                        remaining_nibbles: remaining_nibbles[shared_prefix_len + 1..].to_vec(),
                        value: old_value.to_vec(),
                    };

                    let new_child = nibbles[shared_prefix_len] as usize;
                    let new_leaf = TrieNode::Leaf {
                        remaining_nibbles: nibbles[shared_prefix_len + 1..].to_vec(),
                        value: new_value.to_vec(),
                    };

                    let mut children: [Option<TrieNode>; 16] = [const { None }; 16];
                    children[old_child] = Some(old_leaf);
                    children[new_child] = Some(new_leaf);
                    let branch = TrieNode::Branch {
                        children: Box::new(children),
                        value: None,
                    };
                    if shared_prefix_len == 0 {
                        *slot = Some(branch);
                    } else {
                        *slot = Some(TrieNode::Extension {
                            shared_nibbles: nibbles[..shared_prefix_len].to_vec(),
                            child: Box::new(branch),
                        });
                    }
                }
            }
            TrieNode::Extension {
                shared_nibbles,
                child: old_child,
            } => {
                let shared_prefix_len = shared_nibbles
                    .iter()
                    .zip(nibbles.iter())
                    .take_while(|(a, b)| a == b)
                    .count();

                if shared_prefix_len == shared_nibbles.len() {
                    let mut child_slot = Some(*old_child);
                    recursive_insert(&mut child_slot, &nibbles[shared_prefix_len..], new_value);
                    if let Some(node) = child_slot {
                        *slot = Some(TrieNode::Extension {
                            shared_nibbles,
                            child: Box::new(node),
                        })
                    }
                } else {
                    let new_shared_prefix = shared_nibbles[..shared_prefix_len].to_vec();
                    let mut branch_value = None;
                    let mut children: [Option<TrieNode>; 16] = [const { None }; 16];

                    if nibbles.len() == shared_prefix_len {
                        branch_value = Some(new_value.clone());
                    } else {
                        let new_child_index = nibbles[shared_prefix_len] as usize;
                        let new_leaf = TrieNode::Leaf {
                            remaining_nibbles: nibbles[shared_prefix_len + 1..].to_vec(),
                            value: new_value.to_vec(),
                        };
                        children[new_child_index] = Some(new_leaf);
                    }

                    let old_child_index = shared_nibbles[shared_prefix_len] as usize;
                    if shared_nibbles.len() == shared_prefix_len + 1 {
                        children[old_child_index] = Some(*old_child);
                    } else {
                        let old_continuation = TrieNode::Extension {
                            shared_nibbles: shared_nibbles[shared_prefix_len + 1..].to_vec(),
                            child: old_child,
                        };
                        children[old_child_index] = Some(old_continuation);
                    }

                    let branch = TrieNode::Branch {
                        children: Box::new(children),
                        value: branch_value,
                    };
                    if shared_prefix_len == 0 {
                        *slot = Some(branch);
                    } else {
                        *slot = Some(TrieNode::Extension {
                            shared_nibbles: new_shared_prefix,
                            child: Box::new(branch),
                        });
                    }
                }
            }
            TrieNode::Branch {
                mut children,
                mut value,
            } => {
                if nibbles.is_empty() {
                    value = Some(new_value.to_vec());
                } else {
                    let child_index = nibbles[0] as usize;
                    recursive_insert(&mut children[child_index], &nibbles[1..], new_value);
                }

                *slot = Some(TrieNode::Branch { children, value });
            }
        },
    }
}

impl MerkleTrie {
    pub fn new() -> Self {
        Self { root: None }
    }
    pub fn insert(&mut self, key: &[u8], new_value: Vec<u8>) -> Result<(), TrieError> {
        if key.is_empty() {
            return Err(TrieError::InvalidKey);
        }

        let nibbles = bytes_to_nibbles(key);

        recursive_insert(&mut self.root, &nibbles, &new_value);

        Ok(())
    }

    pub fn root_hash(&self) -> Result<B256, TrieError> {
        match &self.root {
            None => Ok(keccak256(&[0x80])),
            Some(node) => {
                let encoded_node = encode_node(node)?;
                match encoded_node {
                    NodeRef::Inline(x) => Ok(keccak256(&x)),
                    NodeRef::Hash(x) => Ok(x),
                }
            }
        }
    }
}

impl Default for MerkleTrie {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn bytes_to_nibbles_handles_empty_input() {
        assert_eq!(bytes_to_nibbles(&[]), Vec::<u8>::new());
    }

    #[test]
    fn bytes_to_nibbles_splits_single_byte_high_then_low() {
        assert_eq!(bytes_to_nibbles(&[0xab]), vec![0x0a, 0x0b]);
        assert_eq!(bytes_to_nibbles(&[0x00]), vec![0x00, 0x00]);
        assert_eq!(bytes_to_nibbles(&[0xff]), vec![0x0f, 0x0f]);
    }

    #[test]
    fn bytes_to_nibbles_preserves_multi_byte_order() {
        assert_eq!(bytes_to_nibbles(&[0x12, 0x34]), vec![1, 2, 3, 4]);
    }

    #[test]
    fn new_trie_starts_empty() {
        let trie = MerkleTrie::new();
        assert!(trie.root.is_none());
    }

    #[test]
    fn default_trie_starts_empty() {
        let trie = MerkleTrie::default();
        assert!(trie.root.is_none());
    }

    #[test]
    fn hp_encode_extension_even_paths() {
        assert_eq!(hp_encode(&[], false), vec![0x00]);
        assert_eq!(hp_encode(&[1, 2], false), vec![0x00, 0x12]);
    }

    #[test]
    fn hp_encode_extension_odd_paths() {
        assert_eq!(hp_encode(&[1], false), vec![0x11]);
        assert_eq!(hp_encode(&[1, 2, 3], false), vec![0x11, 0x23]);
    }

    #[test]
    fn hp_encode_leaf_even_paths() {
        assert_eq!(hp_encode(&[], true), vec![0x20]);
        assert_eq!(hp_encode(&[1, 2], true), vec![0x20, 0x12]);
    }

    #[test]
    fn hp_encode_leaf_odd_paths() {
        assert_eq!(hp_encode(&[1], true), vec![0x31]);
        assert_eq!(hp_encode(&[1, 2, 3], true), vec![0x31, 0x23]);
    }

    #[test]
    fn insert_rejects_empty_key() {
        let mut trie = MerkleTrie::new();
        assert!(matches!(
            trie.insert(&[], b"value".to_vec()),
            Err(TrieError::InvalidKey)
        ));
        assert!(trie.root.is_none());
    }

    #[test]
    fn insert_into_empty_trie_creates_leaf() {
        let mut trie = MerkleTrie::new();
        trie.insert(&[0x12], b"first".to_vec()).unwrap();

        match trie.root {
            Some(TrieNode::Leaf {
                remaining_nibbles,
                value,
            }) => {
                assert_eq!(remaining_nibbles, vec![1, 2]);
                assert_eq!(value, b"first".to_vec());
            }
            _ => panic!("expected root leaf"),
        }
    }

    #[test]
    fn inserting_same_key_overwrites_leaf_value() {
        let mut trie = MerkleTrie::new();
        trie.insert(&[0x12], b"first".to_vec()).unwrap();
        trie.insert(&[0x12], b"second".to_vec()).unwrap();

        match trie.root {
            Some(TrieNode::Leaf {
                remaining_nibbles,
                value,
            }) => {
                assert_eq!(remaining_nibbles, vec![1, 2]);
                assert_eq!(value, b"second".to_vec());
            }
            _ => panic!("expected root leaf"),
        }
    }

    #[test]
    fn existing_leaf_path_prefix_of_new_path_becomes_branch_with_terminal_value() {
        let mut trie = MerkleTrie::new();
        trie.insert(&[0x12], b"old".to_vec()).unwrap();
        trie.insert(&[0x12, 0x34], b"new".to_vec()).unwrap();

        match trie.root {
            Some(TrieNode::Branch { children, value }) => {
                assert_eq!(value, Some(b"old".to_vec()));

                match &children[3] {
                    Some(TrieNode::Leaf {
                        remaining_nibbles,
                        value,
                    }) => {
                        assert_eq!(remaining_nibbles, &vec![4]);
                        assert_eq!(value, &b"new".to_vec());
                    }
                    _ => panic!("expected new leaf at child 3"),
                }
            }
            _ => panic!("expected root branch"),
        }
    }

    #[test]
    fn new_path_prefix_of_existing_leaf_path_becomes_branch_with_new_terminal_value() {
        let mut trie = MerkleTrie::new();
        trie.insert(&[0x12, 0x34], b"old".to_vec()).unwrap();
        trie.insert(&[0x12], b"new".to_vec()).unwrap();

        match trie.root {
            Some(TrieNode::Branch { children, value }) => {
                assert_eq!(value, Some(b"new".to_vec()));

                match &children[3] {
                    Some(TrieNode::Leaf {
                        remaining_nibbles,
                        value,
                    }) => {
                        assert_eq!(remaining_nibbles, &vec![4]);
                        assert_eq!(value, &b"old".to_vec());
                    }
                    _ => panic!("expected old leaf at child 3"),
                }
            }
            _ => panic!("expected root branch"),
        }
    }

    #[test]
    fn diverging_leaf_paths_without_shared_prefix_become_branch() {
        let mut trie = MerkleTrie::new();
        trie.insert(&[0x12], b"old".to_vec()).unwrap();
        trie.insert(&[0x34], b"new".to_vec()).unwrap();

        match trie.root {
            Some(TrieNode::Branch { children, value }) => {
                assert_eq!(value, None);

                match &children[1] {
                    Some(TrieNode::Leaf {
                        remaining_nibbles,
                        value,
                    }) => {
                        assert_eq!(remaining_nibbles, &vec![2]);
                        assert_eq!(value, &b"old".to_vec());
                    }
                    _ => panic!("expected old leaf at child 1"),
                }

                match &children[3] {
                    Some(TrieNode::Leaf {
                        remaining_nibbles,
                        value,
                    }) => {
                        assert_eq!(remaining_nibbles, &vec![4]);
                        assert_eq!(value, &b"new".to_vec());
                    }
                    _ => panic!("expected new leaf at child 3"),
                }
            }
            _ => panic!("expected root branch"),
        }
    }

    #[test]
    fn diverging_leaf_paths_with_shared_prefix_become_extension_to_branch() {
        let mut trie = MerkleTrie::new();
        trie.insert(&[0x12], b"old".to_vec()).unwrap();
        trie.insert(&[0x13], b"new".to_vec()).unwrap();

        match trie.root {
            Some(TrieNode::Extension {
                shared_nibbles,
                child,
            }) => {
                assert_eq!(shared_nibbles, vec![1]);

                match *child {
                    TrieNode::Branch { children, value } => {
                        assert_eq!(value, None);

                        match &children[2] {
                            Some(TrieNode::Leaf {
                                remaining_nibbles,
                                value,
                            }) => {
                                assert_eq!(remaining_nibbles, &Vec::<u8>::new());
                                assert_eq!(value, &b"old".to_vec());
                            }
                            _ => panic!("expected old leaf at child 2"),
                        }

                        match &children[3] {
                            Some(TrieNode::Leaf {
                                remaining_nibbles,
                                value,
                            }) => {
                                assert_eq!(remaining_nibbles, &Vec::<u8>::new());
                                assert_eq!(value, &b"new".to_vec());
                            }
                            _ => panic!("expected new leaf at child 3"),
                        }
                    }
                    _ => panic!("expected extension child branch"),
                }
            }
            _ => panic!("expected root extension"),
        }
    }

    #[test]
    fn insert_two_keys_with_common_prefix_creates_compressed_root() {
        let mut trie = MerkleTrie::new();
        trie.insert(&[0x12], b"first".to_vec()).unwrap();
        trie.insert(&[0x13], b"second".to_vec()).unwrap();

        match trie.root {
            Some(TrieNode::Extension {
                shared_nibbles,
                child,
            }) => {
                assert_eq!(shared_nibbles, vec![1]);
                match *child {
                    TrieNode::Branch { children, value } => {
                        assert_eq!(value, None);
                        assert!(matches!(&children[2], Some(TrieNode::Leaf { .. })));
                        assert!(matches!(&children[3], Some(TrieNode::Leaf { .. })));
                    }
                    _ => panic!("expected branch under extension"),
                }
            }
            Some(TrieNode::Branch { .. }) => {}
            _ => panic!("expected extension or branch root"),
        }
    }

    #[test]
    fn insert_two_keys_with_no_common_prefix_creates_root_branch() {
        let mut trie = MerkleTrie::new();
        trie.insert(&[0x12], b"first".to_vec()).unwrap();
        trie.insert(&[0x34], b"second".to_vec()).unwrap();

        match trie.root {
            Some(TrieNode::Branch { children, value }) => {
                assert_eq!(value, None);
                assert!(matches!(&children[1], Some(TrieNode::Leaf { .. })));
                assert!(matches!(&children[3], Some(TrieNode::Leaf { .. })));
            }
            _ => panic!("expected root branch"),
        }
    }

    #[test]
    fn insert_key_that_is_prefix_of_existing_key_sets_branch_value() {
        let mut trie = MerkleTrie::new();
        trie.insert(&[0x12, 0x34], b"long".to_vec()).unwrap();
        trie.insert(&[0x12], b"short".to_vec()).unwrap();

        match trie.root {
            Some(TrieNode::Branch { children, value }) => {
                assert_eq!(value, Some(b"short".to_vec()));
                assert!(matches!(&children[3], Some(TrieNode::Leaf { .. })));
            }
            _ => panic!("expected root branch"),
        }
    }

    #[test]
    fn insert_same_key_twice_overwrites_value() {
        let mut trie = MerkleTrie::new();
        trie.insert(&[0xab], b"first".to_vec()).unwrap();
        trie.insert(&[0xab], b"second".to_vec()).unwrap();

        match trie.root {
            Some(TrieNode::Leaf {
                remaining_nibbles,
                value,
            }) => {
                assert_eq!(remaining_nibbles, vec![10, 11]);
                assert_eq!(value, b"second".to_vec());
            }
            _ => panic!("expected root leaf"),
        }
    }

    #[test]
    fn insert_100_incrementing_keys_accepts_all() {
        let mut trie = MerkleTrie::new();

        for i in 0u16..100 {
            trie.insert(&i.to_be_bytes(), i.to_be_bytes().to_vec())
                .unwrap();
        }

        assert!(trie.root.is_some());
    }

    #[test]
    fn insert_through_existing_extension_updates_child_subtrie() {
        let mut trie = MerkleTrie::new();
        trie.insert(&[0x12], b"first".to_vec()).unwrap();
        trie.insert(&[0x13], b"second".to_vec()).unwrap();
        trie.insert(&[0x14], b"third".to_vec()).unwrap();

        match trie.root {
            Some(TrieNode::Extension {
                shared_nibbles,
                child,
            }) => {
                assert_eq!(shared_nibbles, vec![1]);

                match *child {
                    TrieNode::Branch { children, value } => {
                        assert_eq!(value, None);
                        assert!(matches!(&children[2], Some(TrieNode::Leaf { .. })));
                        assert!(matches!(&children[3], Some(TrieNode::Leaf { .. })));

                        match &children[4] {
                            Some(TrieNode::Leaf {
                                remaining_nibbles,
                                value,
                            }) => {
                                assert_eq!(remaining_nibbles, &Vec::<u8>::new());
                                assert_eq!(value, &b"third".to_vec());
                            }
                            _ => panic!("expected new leaf at child 4"),
                        }
                    }
                    _ => panic!("expected extension child branch"),
                }
            }
            _ => panic!("expected root extension"),
        }
    }

    #[test]
    fn inserting_key_that_diverges_inside_existing_extension_splits_extension() {
        let mut trie = MerkleTrie::new();
        trie.insert(&[0x12, 0x34], b"first".to_vec()).unwrap();
        trie.insert(&[0x12, 0x35], b"second".to_vec()).unwrap();
        trie.insert(&[0x12, 0x40], b"third".to_vec()).unwrap();

        match trie.root {
            Some(TrieNode::Extension {
                shared_nibbles,
                child,
            }) => {
                assert_eq!(shared_nibbles, vec![1, 2]);

                match *child {
                    TrieNode::Branch { children, value } => {
                        assert_eq!(value, None);
                        assert!(matches!(&children[3], Some(TrieNode::Branch { .. })));

                        match &children[4] {
                            Some(TrieNode::Leaf {
                                remaining_nibbles,
                                value,
                            }) => {
                                assert_eq!(remaining_nibbles, &vec![0]);
                                assert_eq!(value, &b"third".to_vec());
                            }
                            _ => panic!("expected new leaf at child 4"),
                        }
                    }
                    _ => panic!("expected split extension child branch"),
                }
            }
            _ => panic!("expected root extension after split"),
        }
    }

    #[test]
    fn inserting_key_that_ends_at_extension_split_sets_branch_value() {
        let mut trie = MerkleTrie::new();
        trie.insert(&[0x12, 0x30], b"first".to_vec()).unwrap();
        trie.insert(&[0x12, 0x40], b"second".to_vec()).unwrap();
        trie.insert(&[0x12], b"prefix".to_vec()).unwrap();

        match trie.root {
            Some(TrieNode::Extension {
                shared_nibbles,
                child,
            }) => {
                assert_eq!(shared_nibbles, vec![1, 2]);

                match *child {
                    TrieNode::Branch { children, value } => {
                        assert_eq!(value, Some(b"prefix".to_vec()));
                        assert!(matches!(&children[3], Some(TrieNode::Leaf { .. })));
                        assert!(matches!(&children[4], Some(TrieNode::Leaf { .. })));
                    }
                    _ => panic!("expected extension child branch"),
                }
            }
            _ => panic!("expected root extension"),
        }
    }

    #[test]
    fn inserting_key_that_diverges_before_existing_extension_creates_root_branch() {
        let mut trie = MerkleTrie::new();
        trie.insert(&[0x12], b"first".to_vec()).unwrap();
        trie.insert(&[0x13], b"second".to_vec()).unwrap();
        trie.insert(&[0x01], b"outside".to_vec()).unwrap();

        match trie.root {
            Some(TrieNode::Branch { children, value }) => {
                assert_eq!(value, None);

                match &children[0] {
                    Some(TrieNode::Leaf {
                        remaining_nibbles,
                        value,
                    }) => {
                        assert_eq!(remaining_nibbles, &vec![1]);
                        assert_eq!(value, &b"outside".to_vec());
                    }
                    _ => panic!("expected outside leaf at child 0"),
                }

                match &children[1] {
                    Some(TrieNode::Branch { children, value }) => {
                        assert_eq!(value, &None);
                        assert!(matches!(&children[2], Some(TrieNode::Leaf { .. })));
                        assert!(matches!(&children[3], Some(TrieNode::Leaf { .. })));
                    }
                    _ => panic!("expected old extension child to become branch at child 1"),
                }
            }
            _ => panic!("expected root branch"),
        }
    }

    #[test]
    fn empty_trie_root_hash_matches_known_value() {
        let trie = MerkleTrie::new();

        assert_eq!(
            trie.root_hash().unwrap(),
            B256::from_str("0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421")
                .unwrap()
        );
    }

    #[test]
    fn single_entry_root_hash_is_deterministic() {
        let mut trie = MerkleTrie::new();
        trie.insert(&[0x12], b"value".to_vec()).unwrap();

        let first = trie.root_hash().unwrap();
        let second = trie.root_hash().unwrap();

        assert_eq!(first, second);
        assert_ne!(first, B256::default());
    }

    #[test]
    fn same_keys_inserted_in_different_orders_have_same_root_hash() {
        let entries = [
            ([0x12], b"alpha".to_vec()),
            ([0x34], b"beta".to_vec()),
            ([0x56], b"gamma".to_vec()),
            ([0x78], b"delta".to_vec()),
        ];

        let mut forward = MerkleTrie::new();
        for (key, value) in &entries {
            forward.insert(key, value.clone()).unwrap();
        }

        let mut reverse = MerkleTrie::new();
        for (key, value) in entries.iter().rev() {
            reverse.insert(key, value.clone()).unwrap();
        }

        assert_eq!(forward.root_hash().unwrap(), reverse.root_hash().unwrap());
    }

    #[test]
    fn fifty_entry_root_hash_is_stable() {
        let mut trie = MerkleTrie::new();

        for i in 0u16..50 {
            trie.insert(&i.to_be_bytes(), i.to_be_bytes().to_vec())
                .unwrap();
        }

        let first = trie.root_hash().unwrap();
        let second = trie.root_hash().unwrap();

        assert_eq!(first, second);
        assert_ne!(first, B256::default());
    }
}
