use std::{net::SocketAddr, sync::Arc};

use crate::{
    codec::EthCodec,
    manager::{ChainState, PeerEvent, PeerId},
    message::Message,
};
use futures::{SinkExt, StreamExt};
use tokio::{
    net::TcpStream,
    sync::{RwLock, mpsc},
};
use tokio_util::codec::Framed;

#[derive(Debug, Clone)]
enum HandshakeState {
    AwaitingStatus,
    Established,
}

pub struct ConnectionContext {
    pub peer_id: PeerId,
    pub peer_address: SocketAddr,
    pub expected_chain_id: u64,
    pub peer_sender: mpsc::Sender<Message>,
    pub event_sender: mpsc::Sender<PeerEvent>,
    pub chain_state: Arc<RwLock<ChainState>>,
}

pub async fn handle_connection(
    mut framed: Framed<TcpStream, EthCodec>,
    context: ConnectionContext,
    mut peer_rx: mpsc::Receiver<Message>,
) {
    let ConnectionContext {
        peer_id,
        peer_address,
        expected_chain_id,
        peer_sender,
        event_sender,
        chain_state,
    } = context;
    let mut state = HandshakeState::AwaitingStatus;
    loop {
        tokio::select! {
            msg = framed.next() => {
                match msg {
                    Some(Ok(m)) => {
                        match state {
                            HandshakeState::AwaitingStatus => {
                                match m {
                                    Message::Status { chain_id, .. } => {
                                        if chain_id != expected_chain_id {
                                            let reason = format!(
                                                "Invalid Chain Id. Received {chain_id}, Expected {expected_chain_id}"
                                            );
                                            if (framed.send(Message::Disconnect { reason }).await).is_err() {
                                                break;
                                            }
                                            eprintln!(
                                                "[Connection not established with peer {peer_id}(address: {peer_address})"
                                            );
                                            eprintln!(
                                                "Reason: Invalid Chain Id. Received {chain_id}, Expected {expected_chain_id}"
                                            );
                                            break;
                                        }
                                        let snapshot = {
                                            let state = chain_state.read().await;
                                            state.clone()
                                        };

                                        if (framed.send(Message::Status { chain_id: expected_chain_id, head_hash: snapshot.head_hash, total_difficulty: snapshot.total_difficulty }).await).is_err() {
                                            break;
                                        }

                                        if (event_sender.send(PeerEvent::Connected { peer_id, address: peer_address, sender: peer_sender.clone() }).await).is_err() {
                                            break;
                                        }

                                        eprintln!(
                                            "[Connection established with peer {peer_id}(address: {peer_address})"
                                        );

                                        state = HandshakeState::Established;
                                    },
                                    _ => {
                                        eprintln!(
                                            "[Invalid message sent to establish connection with peer {peer_id}(address: {peer_address})]"
                                        );
                                        break;
                                    }
                                }
                            },
                            HandshakeState::Established => {
                                match m {
                                    Message::Status {..} => {
                                        eprintln!(
                                            "Protocol Violation. Status Message is not to be sent"
                                        );
                                        break;
                                    },
                                    _ => {
                                        if event_sender.send(PeerEvent::Message { peer_id, message: m }).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    },
                    Some(Err(e)) => {
                        eprintln!("Codec error {e}");
                        break;
                    },
                    None => break

                }
            }
            rx = peer_rx.recv() => {
                match state {
                    HandshakeState::Established => {
                        match rx {
                            Some(msg) => {
                                if (framed.send(msg).await).is_err() {
                                    eprintln!("Sending message to peer errored out");
                                    break;
                                }
                            } ,
                            None => break
                        }
                    },
                    HandshakeState::AwaitingStatus => {
                        if rx.is_none() {
                            break;
                        }

                        eprintln!(
                            "[Awaiting connection with peer {peer_id}]"
                        );
                    }
                }
            }
        }
    }

    let _ = event_sender.send(PeerEvent::Disconnected { peer_id }).await;
}
