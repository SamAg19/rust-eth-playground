use std::sync::Arc;

use crate::codec::EthCodec;
use crate::connection::handle_connection;
use crate::manager::PeerEvent;
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
) -> Result<(), NetworkError> {
    let listener = TcpListener::bind(addr).await?;
    let mut peer_id: u64 = 0;
    let mut set: JoinSet<()> = JoinSet::new();
    loop {
        tokio::select! {
            result = listener.accept() => {
                if let Ok((stream, _)) = result {
                    let framed = Framed::new(stream, EthCodec());
                    let (peer_tx, peer_rx) = mpsc::channel::<Message>(32);
                    peer_id += 1;
                    if (sender.send(PeerEvent::Connected { peer_id, sender: peer_tx }).await).is_err() {
                        return Ok(());
                    }
                    let event_sender = sender.clone();
                    let connection_chain_state = chain_state.clone();
                    set.spawn(async move {
                        handle_connection(framed, peer_id, event_sender, peer_rx, connection_chain_state).await
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
