use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::codec::EthCodec;
use crate::connection::{ConnectionContext, handle_connection};
use crate::manager::{PeerEvent, PeerId};
use crate::message::Message;
use crate::{error::NetworkError, manager::ChainState};
use tokio::{
    net::TcpListener,
    sync::{RwLock, broadcast, mpsc},
    task::JoinSet,
};
use tokio_util::codec::Framed;

pub async fn listen(
    addr: &str,
    sender: mpsc::Sender<PeerEvent>,
    mut shutdown_rx: broadcast::Receiver<()>,
    chain_state: Arc<RwLock<ChainState>>,
    chain_id: u64,
) -> Result<(), NetworkError> {
    let listener = TcpListener::bind(addr).await?;
    let peer_counter = Arc::new(AtomicU64::new(0));
    let mut set: JoinSet<()> = JoinSet::new();
    loop {
        tokio::select! {
            result = listener.accept() => {
                if let Ok((stream, address)) = result {
                    let framed = Framed::new(stream, EthCodec());
                    let (peer_tx, peer_rx) = mpsc::channel::<Message>(32);
                    let peer_id = PeerId(peer_counter.fetch_add(1, Ordering::SeqCst) + 1);
                    let event_sender = sender.clone();
                    let connection_chain_state = chain_state.clone();
                    set.spawn(async move {
                        let context = ConnectionContext {
                            peer_id,
                            peer_address: address,
                            expected_chain_id: chain_id,
                            peer_sender: peer_tx,
                            event_sender,
                            chain_state: connection_chain_state,
                        };
                        handle_connection(framed, context, peer_rx).await
                    });
                }
            },
            _ =  shutdown_rx.recv() => {
                break;
            }
        }
    }

    while (set.join_next().await).is_some() {}
    Ok(())
}

//pub async fn run(addr: &str) -> Result<(), NetworkError> {
//    let listener = TcpListener::bind(addr).await?;
//    loop {
//        let (stream, _peer_addr) = listener.accept().await?;
//        let mut framed = Framed::new(stream, EthCodec());
//
//        tokio::spawn(async move {
//            handle_connection(&mut framed).await
//        });
//    }
//
//}
