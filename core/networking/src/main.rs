use networking::{
    client::connect,
    manager::manage,
    manager::{ChainState, PeerEvent},
    server::listen,
};
use std::{sync::Arc, time::Duration};
use tokio::{
    signal,
    sync::{RwLock, broadcast, mpsc},
};
use types::B256;

#[tokio::main]
async fn main() {
    let addr = "127.0.0.1:9000";

    let (event_tx, event_rx) = mpsc::channel::<PeerEvent>(128);

    let (shutdown_tx, shutdown_rx) = broadcast::channel::<()>(1);
    let listener_shutdown_rx = shutdown_tx.subscribe();

    let chain_state = Arc::new(RwLock::new(ChainState {
        head_block_number: 0,
        head_hash: B256::from([0x00; 32]),
        total_difficulty: 0,
    }));

    let manager_chain_state = chain_state.clone();
    let manager_handle =
        tokio::spawn(async move { manage(event_rx, shutdown_rx, manager_chain_state).await });
    tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                if shutdown_tx.send(()).is_err() {
                    eprintln!("Sending shutdown signal to receiver fails");
                }
            }
            Err(e) => eprintln!("Unable to listen for shutdown signal: {}", e),
        }
    });

    let event_tx_clone = event_tx.clone();
    let listener_chain_state = chain_state.clone();
    let listener_handle = tokio::spawn(async move {
        if let Err(e) = listen(
            addr,
            event_tx_clone,
            listener_shutdown_rx,
            listener_chain_state,
        )
        .await
        {
            eprintln!("Listening error: {e}");
        }
    });

    tokio::time::sleep(Duration::from_secs(1)).await;

    let _ = connect(addr).await;

    let _ = manager_handle.await;
    let _ = listener_handle.await;
}
