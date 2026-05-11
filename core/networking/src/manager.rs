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
use types::{Address, Block, ChainHead};

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
    SendMessage {
        peer_id: PeerId,
        message: Message,
    },
}

pub enum NetworkEvent {
    NewBlock { block: Block, peer_id: PeerId },
    GetAccountState { address: Address, peer_id: PeerId },
    GetChainHead { peer_id: PeerId },
}

pub async fn manage(
    mut event_rx: mpsc::Receiver<PeerEvent>,
    mut shutdown_rx: broadcast::Receiver<()>,
    _chain_state: Arc<RwLock<ChainHead>>,
    processor_event_tx: mpsc::Sender<NetworkEvent>,
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
                            tracing::info!("Peer {peer_id} Socket Address {address} connected");
                        },
                        PeerEvent::Disconnected { peer_id } => {
                            peer_map.remove(&peer_id);
                            tracing::info!("Peer {peer_id} disconnected");
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
                                    tracing::warn!("Unexpected Status Message received");
                                },
                                Message::Pong => tracing::info!("Pong messsage received"),
                                Message::GetBlockHeaders { .. } => {
                                    tracing::info!("Get ExecutionBlock Headers messsage received");
                                }
                                Message::NewBlock { block, .. } => {
                                    if processor_event_tx.send(NetworkEvent::NewBlock { block, peer_id }).await.is_err() {
                                        tracing::error!("Processor task has exited");
                                    } else {
                                        tracing::info!("NewBlock messsage received");
                                    }
                                }
                                Message::NewBlockHashes { .. } => {
                                    tracing::info!("NewBlockHashes messsage received");
                                }
                                Message::BlockHeaders { .. } => {
                                    tracing::info!("BlockHeaders messsage received");
                                }
                                Message::Disconnect { .. } => {
                                    tracing::info!("Disconnect messsage received");
                                }
                                Message::GetAccountState { address } => {
                                    if processor_event_tx.send(NetworkEvent::GetAccountState { address, peer_id }).await.is_err() {
                                        tracing::error!("Processor task has exited");
                                    } else {
                                        tracing::info!("GetAccountState messsage received");
                                    }
                                }
                                Message::AccountState { .. } => {
                                    tracing::warn!("Unexpected AccountState message received");
                                }
                                Message::GetChainHead => {
                                    if processor_event_tx
                                        .send(NetworkEvent::GetChainHead { peer_id })
                                        .await
                                        .is_err()
                                    {
                                        tracing::error!("Processor task has exited");
                                    } else {
                                        tracing::info!("GetChainHead messsage received");
                                    }
                                }
                                Message::ChainHead { .. } => {
                                    tracing::warn!("Unexpected ChainHead message received");
                                }
                            }
                        },
                        PeerEvent::SendMessage { peer_id, message } => {
                            if let Some(sender) = peer_map.get(&peer_id).cloned() {
                                if sender.send(message).await.is_err() {
                                    tracing::debug!(
                                        peer_id = %peer_id,
                                        "peer sender closed; dropping outbound message"
                                    );
                                    peer_map.remove(&peer_id);
                                }
                            } else {
                                tracing::debug!(
                                    peer_id = %peer_id,
                                    "peer not found; dropping outbound message"
                                );
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
                    let _ = sender
                        .send(Message::Disconnect {
                            reason: "node shutting down".to_string(),
                        })
                        .await;
                }
                peer_map.clear();
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Duration, timeout};
    use types::B256;

    #[tokio::test]
    async fn forwards_account_state_request_and_sends_targeted_response() {
        let (event_tx, event_rx) = mpsc::channel(8);
        let (network_event_tx, mut network_event_rx) = mpsc::channel(8);
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let chain_state = Arc::new(RwLock::new(ChainHead {
            number: 0,
            hash: B256::zero(),
            total_difficulty: 0,
        }));
        let manager_handle =
            tokio::spawn(manage(event_rx, shutdown_rx, chain_state, network_event_tx));

        let peer_id = PeerId(1);
        let peer_address = "127.0.0.1:30303".parse::<SocketAddr>().unwrap();
        let (peer_sender, mut peer_receiver) = mpsc::channel(8);
        let address = Address::from([0x42; 20]);

        event_tx
            .send(PeerEvent::Connected {
                peer_id,
                address: peer_address,
                sender: peer_sender,
            })
            .await
            .unwrap();
        event_tx
            .send(PeerEvent::Message {
                peer_id,
                message: Message::GetAccountState { address },
            })
            .await
            .unwrap();

        let network_event = timeout(Duration::from_secs(1), network_event_rx.recv())
            .await
            .unwrap()
            .unwrap();
        match network_event {
            NetworkEvent::GetAccountState {
                address: event_address,
                peer_id: event_peer_id,
            } => {
                assert_eq!(event_address, address);
                assert_eq!(event_peer_id, peer_id);
            }
            NetworkEvent::NewBlock { .. } => panic!("expected GetAccountState network event"),
            NetworkEvent::GetChainHead { .. } => panic!("expected GetAccountState network event"),
        }

        event_tx
            .send(PeerEvent::SendMessage {
                peer_id,
                message: Message::AccountState {
                    address,
                    nonce: 7,
                    balance: 123_456,
                },
            })
            .await
            .unwrap();

        let message = timeout(Duration::from_secs(1), peer_receiver.recv())
            .await
            .unwrap()
            .unwrap();
        match message {
            Message::AccountState {
                address: response_address,
                nonce,
                balance,
            } => {
                assert_eq!(response_address, address);
                assert_eq!(nonce, 7);
                assert_eq!(balance, 123_456);
            }
            other => panic!("expected AccountState response, got {other:?}"),
        }

        shutdown_tx.send(()).unwrap();
        timeout(Duration::from_secs(1), manager_handle)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn forwards_chain_head_request_and_sends_targeted_response() {
        let (event_tx, event_rx) = mpsc::channel(8);
        let (network_event_tx, mut network_event_rx) = mpsc::channel(8);
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let chain_state = Arc::new(RwLock::new(ChainHead {
            number: 0,
            hash: B256::zero(),
            total_difficulty: 0,
        }));
        let manager_handle =
            tokio::spawn(manage(event_rx, shutdown_rx, chain_state, network_event_tx));

        let peer_id = PeerId(2);
        let peer_address = "127.0.0.1:30304".parse::<SocketAddr>().unwrap();
        let (peer_sender, mut peer_receiver) = mpsc::channel(8);
        let head_hash = B256::from([0xab; 32]);

        event_tx
            .send(PeerEvent::Connected {
                peer_id,
                address: peer_address,
                sender: peer_sender,
            })
            .await
            .unwrap();
        event_tx
            .send(PeerEvent::Message {
                peer_id,
                message: Message::GetChainHead,
            })
            .await
            .unwrap();

        let network_event = timeout(Duration::from_secs(1), network_event_rx.recv())
            .await
            .unwrap()
            .unwrap();
        match network_event {
            NetworkEvent::GetChainHead {
                peer_id: event_peer_id,
            } => {
                assert_eq!(event_peer_id, peer_id);
            }
            NetworkEvent::NewBlock { .. } => panic!("expected GetChainHead network event"),
            NetworkEvent::GetAccountState { .. } => panic!("expected GetChainHead network event"),
        }

        event_tx
            .send(PeerEvent::SendMessage {
                peer_id,
                message: Message::ChainHead {
                    number: 12,
                    hash: head_hash,
                    total_difficulty: 34,
                },
            })
            .await
            .unwrap();

        let message = timeout(Duration::from_secs(1), peer_receiver.recv())
            .await
            .unwrap()
            .unwrap();
        match message {
            Message::ChainHead {
                number,
                hash,
                total_difficulty,
            } => {
                assert_eq!(number, 12);
                assert_eq!(hash, head_hash);
                assert_eq!(total_difficulty, 34);
            }
            other => panic!("expected ChainHead response, got {other:?}"),
        }

        shutdown_tx.send(()).unwrap();
        timeout(Duration::from_secs(1), manager_handle)
            .await
            .unwrap()
            .unwrap();
    }
}
