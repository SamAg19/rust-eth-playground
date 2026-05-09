use crate::message::Message;
use std::fmt::Display;
use std::net::SocketAddr;
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tokio::{
    sync::{broadcast, mpsc},
    time,
};
use types::B256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PeerId(pub u64);

impl Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "peer-{}", &self.0)?;
        Ok(())
    }
}

pub enum PeerEvent {
    Connected {
        peer_id: PeerId,
        address: SocketAddr,
        sender: mpsc::Sender<Message>,
    },
    Disconnected {
        peer_id: PeerId,
    },
    Message {
        peer_id: PeerId,
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
    _chain_state: Arc<RwLock<ChainState>>,
) {
    let mut peer_map = HashMap::new();
    let mut interval = time::interval(Duration::from_secs(10));

    loop {
        tokio::select! {
            evt = event_rx.recv() => {
                if let Some(event) = evt {
                    match event {
                        PeerEvent::Connected { peer_id, address, sender } => {
                            peer_map.insert(peer_id, sender);
                            eprintln!("Peer {peer_id} Socket Address {address} connected");
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
                                    let mut stale_peers: Vec<PeerId> = vec![];

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
                                Message::Status { .. } => {
                                    eprintln!("Unexpected Status Message received");
                                },
                                Message::Pong => eprintln!("Pong messsage received"),
                                Message::GetBlockHeaders { .. } => {
                                    eprintln!("Get ExecutionBlock Headers messsage received");
                                }
                                Message::NewBlock { .. } => {
                                    eprintln!("NewBlock messsage received");
                                }
                                Message::NewBlockHashes { .. } => {
                                    eprintln!("NewBlockHashes messsage received");
                                }
                                Message::BlockHeaders { .. } => {
                                    eprintln!("BlockHeaders messsage received");
                                }
                                Message::Disconnect { .. } => {
                                    eprintln!("Unexpected Disconnect messsage received");
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
                let mut stale_peers: Vec<PeerId> = vec![];

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
