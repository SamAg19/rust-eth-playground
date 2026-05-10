use std::{sync::Arc, time::Duration};

use futures::{SinkExt, StreamExt};
use networking::{
    codec::EthCodec,
    connection::{ConnectionContext, handle_connection},
    manager::{PeerEvent, PeerId},
    message::Message,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{RwLock, mpsc},
    time::timeout,
};
use tokio_util::codec::Framed;
use types::{B256, ChainHead};

const CHAIN_ID: u64 = 1;
const PEER_ID: PeerId = PeerId(7);

async fn spawn_one_connection_server() -> (std::net::SocketAddr, mpsc::Receiver<PeerEvent>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (event_tx, event_rx) = mpsc::channel::<PeerEvent>(16);
    let chain_state = Arc::new(RwLock::new(ChainHead {
        number: 0,
        hash: B256::zero(),
        total_difficulty: 0,
    }));

    tokio::spawn(async move {
        let (stream, peer_address) = listener.accept().await.unwrap();
        let framed = Framed::new(stream, EthCodec());
        let (peer_tx, peer_rx) = mpsc::channel::<Message>(16);
        let context = ConnectionContext {
            peer_id: PEER_ID,
            peer_address,
            expected_chain_id: CHAIN_ID,
            peer_sender: peer_tx,
            event_sender: event_tx,
            chain_state,
        };
        handle_connection(framed, context, peer_rx).await;
    });

    (address, event_rx)
}

async fn connect_client(address: std::net::SocketAddr) -> Framed<TcpStream, EthCodec> {
    let stream = TcpStream::connect(address).await.unwrap();
    Framed::new(stream, EthCodec())
}

fn status(chain_id: u64) -> Message {
    Message::Status {
        chain_id,
        head_hash: B256::zero(),
        total_difficulty: 0,
    }
}

async fn assert_no_connected_event(event_rx: &mut mpsc::Receiver<PeerEvent>) {
    while let Ok(Some(event)) = timeout(Duration::from_millis(100), event_rx.recv()).await {
        if let PeerEvent::Connected { .. } = event {
            panic!("peer should not have been registered");
        }
    }
}

#[tokio::test]
async fn handshake_valid_chain_id() {
    let (address, mut event_rx) = spawn_one_connection_server().await;
    let mut client = connect_client(address).await;

    client.send(status(CHAIN_ID)).await.unwrap();

    let response = timeout(Duration::from_secs(1), client.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match response {
        Message::Status {
            chain_id,
            head_hash,
            total_difficulty,
        } => {
            assert_eq!(chain_id, CHAIN_ID);
            assert_eq!(head_hash, B256::zero());
            assert_eq!(total_difficulty, 0);
        }
        _ => panic!("expected Status response"),
    }

    let event = timeout(Duration::from_secs(1), event_rx.recv())
        .await
        .unwrap()
        .unwrap();
    match event {
        PeerEvent::Connected { peer_id, .. } => assert_eq!(peer_id, PEER_ID),
        _ => panic!("expected Connected event"),
    }
}

#[tokio::test]
async fn handshake_wrong_chain_id() {
    let (address, mut event_rx) = spawn_one_connection_server().await;
    let mut client = connect_client(address).await;

    client.send(status(CHAIN_ID + 1)).await.unwrap();

    let response = timeout(Duration::from_secs(1), client.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match response {
        Message::Disconnect { reason } => {
            assert!(reason.contains("Invalid Chain Id"));
            assert!(reason.contains(&(CHAIN_ID + 1).to_string()));
            assert!(reason.contains(&CHAIN_ID.to_string()));
        }
        _ => panic!("expected Disconnect response"),
    }

    let closed = timeout(Duration::from_secs(1), client.next())
        .await
        .unwrap();
    assert!(closed.is_none());
    assert_no_connected_event(&mut event_rx).await;
}

#[tokio::test]
async fn handshake_non_status_first() {
    let (address, mut event_rx) = spawn_one_connection_server().await;
    let mut client = connect_client(address).await;

    client.send(Message::Ping).await.unwrap();

    let closed = timeout(Duration::from_secs(1), client.next())
        .await
        .unwrap();
    assert!(closed.is_none());
    assert_no_connected_event(&mut event_rx).await;
}

#[tokio::test]
async fn handshake_second_status_after_established_closes() {
    let (address, mut event_rx) = spawn_one_connection_server().await;
    let mut client = connect_client(address).await;

    client.send(status(CHAIN_ID)).await.unwrap();
    let response = timeout(Duration::from_secs(1), client.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(response, Message::Status { .. }));

    let event = timeout(Duration::from_secs(1), event_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        event,
        PeerEvent::Connected {
            peer_id: PEER_ID,
            ..
        }
    ));

    client.send(status(CHAIN_ID)).await.unwrap();
    let closed = timeout(Duration::from_secs(1), client.next())
        .await
        .unwrap();
    assert!(closed.is_none());

    let event = timeout(Duration::from_secs(1), event_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        event,
        PeerEvent::Disconnected { peer_id: PEER_ID }
    ));
}
