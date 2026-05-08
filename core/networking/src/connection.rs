use std::sync::Arc;

use crate::{
    codec::EthCodec,
    manager::{ChainState, PeerEvent},
    message::Message,
};
use futures::{SinkExt, StreamExt};
use tokio::{
    net::TcpStream,
    sync::{RwLock, mpsc},
};
use tokio_util::codec::Framed;

pub async fn handle_connection(
    mut framed: Framed<TcpStream, EthCodec>,
    peer_id: u64,
    event_sender: mpsc::Sender<PeerEvent>,
    mut peer_rx: mpsc::Receiver<Message>,
    chain_state: Arc<RwLock<ChainState>>,
) {
    loop {
        tokio::select! {
            msg = framed.next() => {
                match msg {
                    Some(Ok(Message::GetBlockHeaders { start_hash, count })) => {
                        // --- 8.4: intentionally broken version (left commented for reference) ---
                        // The code below fails to compile with an error like:
                        //     future cannot be sent between threads safely
                        //     `RwLockReadGuard<'_, ChainState>` is not `Send`
                        // Reason: `RwLockReadGuard` is !Send. Holding it across `.await`
                        // poisons the future's Send-ness, and this task runs on the
                        // multi-threaded runtime, which requires Send futures.
                        //
                        // let state = chain_state.read().await;
                        // let ack = Message::Pong;
                        // let _ = framed.send(ack).await; // ← .await while `state` is alive
                        // eprintln!(
                        //     "[peer {peer_id}] GetBlockHeaders(start={start_hash}, count={count}) | state = {:?}",
                        //     *state
                        // );
                        //
                        // --- 8.5: fix — clone the data out, drop the guard, then await ---
                        let snapshot = {
                            let state = chain_state.read().await;
                            state.clone()
                        }; // guard dropped here; `snapshot` is a plain ChainState (Send)
                        eprintln!(
                            "[peer {peer_id}] GetBlockHeaders(start={start_hash}, count={count}) | state = {:?}",
                            snapshot
                        );
                        let ack = Message::Pong;
                        if (framed.send(ack).await).is_err() {
                            eprintln!("Sending ack to peer errored out");
                            break;
                        }
                    },
                    Some(Ok(m)) => {
                        if event_sender.send(PeerEvent::Message { peer_id, message: m }).await.is_err() {
                            break;
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
                match rx {
                    Some(msg) => {
                        if (framed.send(msg).await).is_err() {
                            eprintln!("Sending message to peer errored out");
                            break;
                        }
                    } ,
                    None => break
                }
            }
        }
    }

    let _ = event_sender.send(PeerEvent::Disconnected { peer_id }).await;
}
