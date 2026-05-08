use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use types::{Address, B256, Transaction};

use crate::{codec::EthCodec, error::NetworkError, message::Message};

pub async fn connect(addr: &str) -> Result<(), NetworkError> {
    let stream = TcpStream::connect(addr).await?;
    let mut framed = Framed::new(stream, EthCodec());

    if let Err(e) = framed.send(Message::Ping).await {
        eprintln!("Send error: {e}");
    }
    match framed.next().await {
        Some(Ok(Message::Pong)) => eprintln!("Pong message received"),
        Some(Ok(_)) => eprintln!("Appropriate ping response not received"),
        Some(Err(e)) => {
            eprintln!("Error in response: {e}");
            return Err(e);
        }
        None => return Err(NetworkError::PeerDisconnected),
    }

    let status = Message::Status {
        chain_id: 1,
        head_hash: B256::from([0x11; 32]),
        total_difficulty: 12_345_678,
    };
    if let Err(e) = framed.send(status).await {
        eprintln!("Send error with Status: {e}");
    }

    let tx = Transaction::Legacy {
        nonce: 0,
        gas_price: 1_000_000_000,
        gas_limit: 21_000,
        to: Some(Address::from([0x22; 20])),
        value: 1_000,
        data: vec![0xde, 0xad, 0xbe, 0xef],
    };
    if let Err(e) = framed.send(Message::Transactions { txs: vec![tx] }).await {
        eprintln!("Send error with Transactions: {e}");
    }

    let get_headers = Message::GetBlockHeaders {
        start_hash: B256::from([0x33; 32]),
        count: 64,
    };
    if let Err(e) = framed.send(get_headers).await {
        eprintln!("Send error with GetBlockHeaders: {e}");
    }

    if let Err(e) = framed.send(Message::Pong).await {
        eprintln!("Send error with Pong: {e}");
    }

    Ok(())
}
