use crate::message::Message;
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tokio::{
    sync::{broadcast, mpsc},
    time,
};
use types::B256;

pub enum PeerEvent {
    Connected {
        peer_id: u64,
        sender: mpsc::Sender<Message>,
    },
    Disconnected {
        peer_id: u64,
    },
    Message {
        peer_id: u64,
        message: Message,
    },
}

#[derive(Clone, Debug)]
pub struct ChainState {
    pub head_block_number: u64,
    pub head_hash: B256,
    pub total_difficulty: u128,
}

pub async fn manage(
    mut event_rx: mpsc::Receiver<PeerEvent>,
    mut shutdown_rx: broadcast::Receiver<()>,
    chain_state: Arc<RwLock<ChainState>>,
) {
    let mut peer_map = HashMap::new();
    let mut interval = time::interval(Duration::from_secs(10));
    let mut block_number = 1;

    loop {
        tokio::select! {
            evt = event_rx.recv() => {
                if let Some(event) = evt {
                    match event {
                        PeerEvent::Connected { peer_id, sender } => {
                            peer_map.insert(peer_id, sender);
                            eprintln!("Peer {peer_id} connected");
                        },
                        PeerEvent::Disconnected { peer_id } => {
                            peer_map.remove(&peer_id);
                            eprintln!("Peer {peer_id} disconnected");
                        },
                        PeerEvent::Message { peer_id, message } => {
                            match message {
                                Message::Ping => {
                                    if let Some(sender) = peer_map.get(&peer_id)
                                        && (sender.send(Message::Pong).await).is_err() {
                                        peer_map.remove(&peer_id);
                                    }
                                },
                                Message::Transactions { txs } => {
                                    let mut stale_peers: Vec<u64> = vec![];

                                    for (id, sender) in &peer_map {
                                        if *id == peer_id {
                                            continue;
                                        }

                                        if (sender.send(Message::Transactions { txs: txs.clone() }).await).is_err() {
                                            stale_peers.push(*id);
                                        }
                                    }

                                    for peer in stale_peers {
                                        peer_map.remove(&peer);
                                    }
                                }
                                Message::Status { chain_id: _chain_id, head_hash, total_difficulty } => {
                                    let mut state = chain_state.write().await;
                                    state.head_block_number = block_number;
                                    block_number += 1;
                                    state.head_hash = head_hash;
                                    state.total_difficulty = total_difficulty;
                                },
                                Message::Pong => eprintln!("Pong messsage received"),
                                Message::GetBlockHeaders { .. } => {
                                    eprintln!("Get Block Headers messsage received");
                                }
                            }
                        }
                    }
                }
                else {
                    break;
                }
            },
            _ = interval.tick() => {
                let mut stale_peers: Vec<u64> = vec![];

                for (id, sender) in &peer_map {

                    if (sender.send(Message::Ping).await).is_err() {
                        stale_peers.push(*id);
                    }
                }

                for peer in stale_peers {
                    peer_map.remove(&peer);
                }
            }
            _ = shutdown_rx.recv() => {
                for sender in peer_map.values() {
                    let _ = sender.send(Message::Pong).await;
                }
                peer_map.clear();
                break;
            }
        }
    }
}
