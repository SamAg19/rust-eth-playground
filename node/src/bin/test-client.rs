use std::{collections::HashMap, error::Error, io, time::Duration};

use block_builder::block_builder::{AccountSnapshot, BlockBuilder, BlockSpec, TransactionSpec};
use clap::Parser;
use execution::{InMemoryProvider, executor::compute_state_root};
use futures::{SinkExt, StreamExt};
use networking::{codec::EthCodec, message::Message};
use rlp_codec::{
    hash_header,
    signing::{recover_sender, sign},
};
use tokio::{
    net::TcpStream,
    time::{Instant, sleep, timeout},
};
use tokio_util::codec::Framed;
use types::{Account, Address, GenesisAccount, GenesisConfig, Transaction};

#[derive(Debug, Parser)]
#[command(version, about = "Sends generated test blocks to the local node")]
struct Cli {
    /// Server IP address to connect to
    #[arg(long, default_value_t = String::from("127.0.0.1"))]
    server_address: String,

    /// Server TCP port to connect to
    #[arg(long, default_value_t = 30303)]
    server_port: u16,

    /// Chain ID. Must match the node.
    #[arg(long, default_value_t = 1337)]
    chain_id: u64,

    /// Genesis timestamp. Must match the node.
    #[arg(long, default_value_t = 0)]
    genesis_timestamp: u64,

    /// Number of blocks to generate and send
    #[arg(long, default_value_t = 10)]
    block_count: u64,

    /// Number of transactions to include in each block
    #[arg(long, default_value_t = 3)]
    transactions_per_block: u8,

    /// Genesis account count. Must match the node.
    #[arg(long, default_value_t = 5)]
    genesis_account_count: u8,

    /// Genesis balance in wei. Must match the node.
    #[arg(long, default_value_t = 1_000_000_000_000_000_000_000)]
    genesis_balance: u128,

    /// Send blocks out of order. Disabled for the processor-backed nonce query flow.
    #[arg(long, default_value_t = false)]
    out_of_order: bool,
}

pub async fn query_account_state(
    framed: &mut Framed<TcpStream, EthCodec>,
    address: Address,
) -> Result<AccountSnapshot, Box<dyn Error>> {
    framed.send(Message::GetAccountState { address }).await?;
    framed.flush().await?;

    let response = match timeout(Duration::from_secs(5), framed.next()).await {
        Ok(Some(Ok(message))) => message,
        Ok(Some(Err(error))) => {
            tracing::error!(%address, %error, "failed to decode account-state response");
            return Err(error.into());
        }
        Ok(None) => {
            tracing::error!(%address, "node disconnected before account-state response");
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "node disconnected before account-state response",
            )
            .into());
        }
        Err(_) => {
            tracing::error!(%address, "timed out waiting for account-state response");
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "timed out waiting for account-state response",
            )
            .into());
        }
    };

    match response {
        Message::AccountState {
            address: response_address,
            nonce,
            balance,
        } => {
            if response_address != address {
                tracing::error!(
                    requested_address = %address,
                    received_address = %response_address,
                    "account-state response address mismatch"
                );
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "account-state response address mismatch",
                )
                .into());
            }

            Ok(AccountSnapshot {
                address,
                nonce,
                balance,
            })
        }
        other => {
            tracing::error!(%address, ?other, "unexpected account-state response");
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected account-state response",
            )
            .into())
        }
    }
}

async fn wait_for_sender_nonces(
    framed: &mut Framed<TcpStream, EthCodec>,
    block_number: u64,
    expected_nonces: &[(Address, u64)],
) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(5);

    loop {
        let mut all_processed = true;

        for (address, expected_nonce) in expected_nonces {
            let snapshot = query_account_state(framed, *address).await?;
            if snapshot.nonce < *expected_nonce {
                all_processed = false;
            }
        }

        if all_processed {
            tracing::debug!(block_number, "block processing confirmed by sender nonces");
            return Ok(());
        }

        if Instant::now() >= deadline {
            tracing::error!(
                block_number,
                "timed out waiting for block processing confirmation"
            );
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "timed out waiting for block processing confirmation",
            )
            .into());
        }

        sleep(Duration::from_millis(50)).await;
    }
}

async fn query_chain_head(
    framed: &mut Framed<TcpStream, EthCodec>,
) -> Result<(u64, types::B256, u128), Box<dyn Error>> {
    framed.send(Message::GetChainHead).await?;
    framed.flush().await?;

    let response = match timeout(Duration::from_secs(5), framed.next()).await {
        Ok(Some(Ok(message))) => message,
        Ok(Some(Err(error))) => {
            tracing::error!(%error, "failed to decode chain-head response");
            return Err(error.into());
        }
        Ok(None) => {
            tracing::error!("node disconnected before chain-head response");
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "node disconnected before chain-head response",
            )
            .into());
        }
        Err(_) => {
            tracing::error!("timed out waiting for chain-head response");
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "timed out waiting for chain-head response",
            )
            .into());
        }
    };

    match response {
        Message::ChainHead {
            number,
            hash,
            total_difficulty,
        } => Ok((number, hash, total_difficulty)),
        other => {
            tracing::error!(?other, "unexpected chain-head response");
            Err(io::Error::new(io::ErrorKind::InvalidData, "unexpected chain-head response").into())
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .compact()
        .init();

    let cli = Cli::parse();
    let dummy_tx = Transaction::Legacy {
        nonce: 0,
        gas_price: 1_000_000_000,
        gas_limit: 21_000,
        to: None,
        value: 0,
        data: vec![],
    };
    let mut signing_keys = Vec::with_capacity(cli.genesis_account_count as usize);
    let mut genesis_accounts = Vec::with_capacity(cli.genesis_account_count as usize);

    for i in 0..cli.genesis_account_count {
        let private_key = [i + 1; 32];
        let signed_transaction = sign(&dummy_tx, &private_key, cli.chain_id)?;
        let address = recover_sender(&signed_transaction, cli.chain_id)?;
        signing_keys.push(private_key);
        genesis_accounts.push(GenesisAccount {
            address,
            balance: cli.genesis_balance,
        });
    }

    let genesis_config = GenesisConfig {
        chain_id: cli.chain_id,
        accounts: genesis_accounts,
        genesis_timestamp: cli.genesis_timestamp,
    };
    let mut provider = InMemoryProvider::default();

    for genesis_account in &genesis_config.accounts {
        let account = Account {
            balance: genesis_account.balance,
            ..Account::default()
        };
        provider.set_account(genesis_account.address, account);
    }

    let state_root = compute_state_root(&provider)?;
    let genesis_header = genesis_config.genesis_header_with_state_root(state_root);
    let genesis_hash = hash_header(&genesis_header)?;
    let mut builder = BlockBuilder::new(
        cli.chain_id,
        genesis_hash,
        cli.genesis_timestamp,
        genesis_header.gas_limit,
        signing_keys,
    )?;

    tracing::info!(%genesis_hash, "computed test-client genesis block hash");

    if cli.out_of_order {
        tracing::error!(
            "out-of-order mode requires local execution or state prediction and is disabled with node-backed nonce queries"
        );
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "out-of-order mode requires local execution or state prediction and is disabled with node-backed nonce queries",
        )
        .into());
    }

    let server_addr = format!("{}:{}", cli.server_address, cli.server_port);
    let stream = match TcpStream::connect(&server_addr).await {
        Ok(stream) => stream,
        Err(error) => {
            tracing::error!(%server_addr, %error, "failed to connect to node");
            return Err(error.into());
        }
    };
    let mut framed = Framed::new(stream, EthCodec());
    let (_, head_hash) = builder.current_head();

    framed
        .send(Message::Status {
            chain_id: cli.chain_id,
            head_hash,
            total_difficulty: 0,
        })
        .await?;

    let response = match timeout(Duration::from_secs(5), framed.next()).await {
        Ok(Some(Ok(message))) => message,
        Ok(Some(Err(error))) => {
            tracing::error!(%error, "failed to decode handshake response");
            return Err(error.into());
        }
        Ok(None) => {
            tracing::error!("node disconnected before sending handshake response");
            return Ok(());
        }
        Err(_) => {
            tracing::error!("timed out waiting for handshake response");
            return Ok(());
        }
    };

    match response {
        Message::Status {
            chain_id,
            head_hash: server_head_hash,
            total_difficulty,
        } => {
            if chain_id != cli.chain_id {
                tracing::error!(
                    expected_chain_id = cli.chain_id,
                    actual_chain_id = chain_id,
                    "server chain ID mismatch"
                );
                return Ok(());
            }

            tracing::info!(
                server_chain_id = chain_id,
                server_head_hash = %server_head_hash,
                server_total_difficulty = total_difficulty,
                "handshake completed"
            );
        }
        other => {
            tracing::error!(?other, "unexpected handshake response");
            return Ok(());
        }
    }

    let (server_head_number, server_head_hash, server_total_difficulty) =
        query_chain_head(&mut framed).await?;
    builder.set_current_head(server_head_number, server_head_hash)?;
    tracing::info!(
        server_head_number,
        server_head_hash = %server_head_hash,
        server_total_difficulty,
        "resuming from server chain head"
    );

    if cli.genesis_account_count == 0 {
        tracing::error!("genesis_account_count must be greater than zero");
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "genesis_account_count must be greater than zero",
        )
        .into());
    }

    if cli.transactions_per_block > cli.genesis_account_count {
        tracing::error!(
            transactions_per_block = cli.transactions_per_block,
            genesis_account_count = cli.genesis_account_count,
            "transactions_per_block cannot exceed genesis_account_count because this client allows at most one transaction per sender per block"
        );
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "transactions_per_block cannot exceed genesis_account_count",
        )
        .into());
    }

    let account_count = cli.genesis_account_count as usize;

    for block_index in 0..cli.block_count {
        let mut transaction_specs = Vec::with_capacity(cli.transactions_per_block as usize);
        let mut account_snapshots = HashMap::new();
        let mut expected_sender_nonces = Vec::with_capacity(cli.transactions_per_block as usize);

        for transaction_index in 0..cli.transactions_per_block {
            let sender_index =
                ((block_index as usize) + (transaction_index as usize)) % account_count;
            let recipient_index = (sender_index + 1) % account_count;
            let sender_address = genesis_config.accounts[sender_index].address;
            let recipient = genesis_config.accounts[recipient_index].address;

            let snapshot = query_account_state(&mut framed, sender_address).await?;
            let expected_nonce = snapshot.nonce.checked_add(1).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "sender nonce overflow")
            })?;
            expected_sender_nonces.push((sender_address, expected_nonce));
            account_snapshots.insert(sender_address, snapshot);
            transaction_specs.push(TransactionSpec {
                sender_index,
                recipient,
                value: 1_000_000_000_000_000_000,
                gas_limit: 21_000,
            });
        }

        let block_spec = BlockSpec {
            transactions: transaction_specs,
        };
        let block = match builder.generate_block(block_spec, account_snapshots) {
            Ok(block) => block,
            Err(error) => {
                tracing::error!(%error, block_index, "failed to generate block");
                return Err(error.into());
            }
        };
        let block_hash = hash_header(&block.header)?;
        let block_number = block.header.number;
        let tx_count = block.transactions.len();

        framed
            .send(Message::NewBlock {
                td: block.header.gas_used as u128,
                block,
            })
            .await?;
        framed.flush().await?;

        tracing::debug!(
            block_number,
            block_hash = %block_hash,
            tx_count,
            "generated and sent block"
        );

        wait_for_sender_nonces(&mut framed, block_number, &expected_sender_nonces).await?;
        sleep(Duration::from_millis(100)).await;
    }

    sleep(Duration::from_millis(500)).await;
    framed
        .send(Message::Disconnect {
            reason: "all blocks sent".to_string(),
        })
        .await?;
    framed.flush().await?;

    tracing::info!("test client disconnected cleanly");

    Ok(())
}
