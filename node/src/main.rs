use std::{
    collections::HashMap,
    sync::{Arc, atomic::Ordering},
};

use clap::Parser;
use execution::{InMemoryProvider, executor::compute_state_root};
use networking::{
    manager::{NetworkEvent, PeerEvent, manage},
    message::Message,
    server::listen,
};
use processor::{
    block_processor::{BlockProcessor, ProcessorMessage},
    errors::ProcessorError,
};
use rlp_codec::{
    hash_header,
    signing::{recover_sender, sign},
};
use tokio::{
    signal,
    sync::{RwLock, broadcast, mpsc, oneshot},
};
use types::{Account, Address, Block, ChainHead, GenesisAccount, GenesisConfig, Transaction};

#[derive(Parser, Debug)]
#[command(version, about = "Runs the local block processor node")]
struct Cli {
    /// The IP the node binds its TCP listener to
    #[arg(long, default_value_t = String::from("0.0.0.0"))]
    listen_address: String,

    /// This is the port of the IP address
    #[arg(long, default_value_t = 30303)]
    listen_port: u16,

    /// Accept the values trace, debug, info, warn, and error.
    #[arg(long, default_value_t = String::from("info"))]
    log_level: String,

    /// Chain id of the blockchain
    #[arg(long, default_value_t = 1337)]
    chain_id: u64,

    /// The node generates this many pre-funded accounts on startup
    #[arg(long, default_value_t = 5)]
    genesis_account_count: u8,

    /// Genesis balance of accounts created in wei. Every genesis account starts with this balance.
    #[arg(long, default_value_t = 1_000_000_000_000_000_000_000)]
    genesis_balance: u128,

    /// Genesis Timestamp of the genesis block
    #[arg(long, default_value_t = 0)]
    genesis_timestamp: u64,

    /// If set to true, the node initialises everything, logs the genesis configuration, and exits without binding any ports or spawning any tasks
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let level = match cli.log_level.as_str() {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        other => {
            eprintln!("invalid log level: {other}");
            eprintln!("expected one of: trace, debug, info, warn, error");
            std::process::exit(1);
        }
    };

    tracing_subscriber::fmt()
        .with_max_level(level)
        .compact()
        .init();

    let dummy_tx = Transaction::Legacy {
        nonce: 0,
        gas_price: 1_000_000_000,
        gas_limit: 21_000,
        to: None,
        value: 0,
        data: vec![],
    };

    let mut genesis_accounts: Vec<GenesisAccount> = vec![];

    for i in 0..cli.genesis_account_count {
        let private_key: [u8; 32] = [i + 1; 32];
        let signed_transaction = sign(&dummy_tx, &private_key, cli.chain_id)?;
        let address = recover_sender(&signed_transaction, cli.chain_id)?;
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

    let mut accounts: HashMap<Address, Account> = HashMap::new();
    let mut provider = InMemoryProvider::default();

    tracing::info!("Block Processor node setup");
    tracing::info!("Version: 0.1.0");
    tracing::info!("Chain ID: {}", genesis_config.chain_id);
    tracing::info!("Listen Port: {}", cli.listen_port);
    tracing::info!("Listen Address: {}", cli.listen_address);
    tracing::info!("Initial Accounts listed below");
    for genesis_account in genesis_config.accounts.clone() {
        tracing::info!(
            "Address: {}, Balance: {} ETH",
            genesis_account.address,
            genesis_account.balance / (1e18 as u128)
        );
        let account = Account {
            balance: genesis_account.balance,
            ..Account::default()
        };
        provider.set_account(genesis_account.address, account.clone());
        accounts.insert(genesis_account.address, account);
    }

    let state_root = compute_state_root(&provider)?;
    tracing::info!("Genesis State Root: {}", state_root);

    let genesis_header = genesis_config.genesis_header_with_state_root(state_root);
    let genesis_block = Block {
        header: genesis_header.clone(),
        transactions: vec![],
    };

    let genesis_hash = hash_header(&genesis_header)?;

    tracing::info!("Genesis Block Root: {}", genesis_hash);

    if cli.dry_run {
        return Ok(());
    }

    let chain_head = ChainHead {
        number: 0,
        hash: genesis_hash,
        total_difficulty: 0,
    };

    let shared_head = Arc::new(RwLock::new(chain_head));

    let processor = BlockProcessor::new(
        genesis_block,
        accounts,
        genesis_config.chain_id,
        shared_head.clone(),
    )
    .await?;

    let metrics = Arc::clone(&processor.metrics);
    let shared_head = Arc::clone(&processor.shared_head);

    let (processor_tx, processor_rx) = mpsc::channel::<ProcessorMessage>(256);
    let (event_tx, event_rx) = mpsc::channel::<PeerEvent>(256);

    let (shutdown_tx, shutdown_rx) = broadcast::channel::<()>(16);
    let listener_shutdown_rx = shutdown_tx.subscribe();
    let mut main_shutdown_rx = shutdown_tx.subscribe();

    let processor_handle = tokio::spawn(async move {
        processor.run(processor_rx).await;
    });
    tracing::debug!("Block processor has been spawned");

    let (network_event_tx, mut network_event_rx) = mpsc::channel::<NetworkEvent>(256);
    let processor_tx_for_adapter = processor_tx.clone();
    let event_tx_for_adapter = event_tx.clone();
    let shared_head_for_adapter = Arc::clone(&shared_head);

    tokio::spawn(async move {
        while let Some(event) = network_event_rx.recv().await {
            match event {
                NetworkEvent::NewBlock { block, peer_id } => {
                    let msg = ProcessorMessage::NewBlock { block, peer_id };

                    if processor_tx_for_adapter.send(msg).await.is_err() {
                        tracing::error!("processor task exited; dropping new block");
                    }
                }
                NetworkEvent::GetAccountState { address, peer_id } => {
                    let (response_tx, response_rx) =
                        oneshot::channel::<Result<Account, ProcessorError>>();
                    let msg = ProcessorMessage::GetAccountState {
                        address,
                        response_tx,
                    };

                    if processor_tx_for_adapter.send(msg).await.is_err() {
                        tracing::error!(
                            peer_id = %peer_id,
                            %address,
                            "processor task exited; dropping account state query"
                        );
                        continue;
                    }

                    match response_rx.await {
                        Ok(Ok(account)) => {
                            let message = Message::AccountState {
                                address,
                                nonce: account.nonce,
                                balance: account.balance,
                            };

                            if event_tx_for_adapter
                                .send(PeerEvent::SendMessage { peer_id, message })
                                .await
                                .is_err()
                            {
                                tracing::debug!(
                                    peer_id = %peer_id,
                                    %address,
                                    "manager task exited; dropping account state response"
                                );
                            }
                        }
                        Ok(Err(error)) => {
                            tracing::warn!(
                                peer_id = %peer_id,
                                %address,
                                error = %error,
                                "account state query failed"
                            );
                        }
                        Err(_) => {
                            tracing::debug!(
                                peer_id = %peer_id,
                                %address,
                                "processor dropped account state response channel"
                            );
                        }
                    }
                }
                NetworkEvent::GetChainHead { peer_id } => {
                    let head = shared_head_for_adapter.read().await.clone();
                    let message = Message::ChainHead {
                        number: head.number,
                        hash: head.hash,
                        total_difficulty: head.total_difficulty,
                    };

                    if event_tx_for_adapter
                        .send(PeerEvent::SendMessage { peer_id, message })
                        .await
                        .is_err()
                    {
                        tracing::debug!(
                            peer_id = %peer_id,
                            "manager task exited; dropping chain head response"
                        );
                    }
                }
            }
        }
    });

    let manager_head_clone = shared_head.clone();
    let manager_handle = tokio::spawn(async move {
        manage(event_rx, shutdown_rx, manager_head_clone, network_event_tx).await;
    });
    tracing::debug!("Manager task has been spawned");

    tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                tracing::info!("Graceful shutdown triggered. Shutting down the node...");
                if shutdown_tx.send(()).is_err() {
                    tracing::debug!("Sending shutdown signal to receiver fails");
                }
            }
            Err(e) => tracing::error!("Unable to listen for shutdown signal: {}", e),
        }
    });

    let event_tx_clone = event_tx.clone();
    let addr = format!("{}:{}", cli.listen_address, cli.listen_port);
    let listener_head_clone = shared_head.clone();
    let listener_addr = addr.clone();
    let listener_handle = tokio::spawn(async move {
        if let Err(e) = listen(
            &listener_addr,
            event_tx_clone,
            listener_shutdown_rx,
            listener_head_clone,
            cli.chain_id,
        )
        .await
        {
            tracing::error!("Listening error: {e}");
        }
    });

    tracing::debug!("Listener task has been spawned");

    let _ = main_shutdown_rx.recv().await;
    tracing::info!("shutdown signal received in main");

    listener_handle.await?;
    manager_handle.await?;

    if processor_tx.send(ProcessorMessage::Shutdown).await.is_err() {
        tracing::debug!("processor task already stopped before shutdown message");
    }
    processor_handle.await?;

    let final_head = shared_head.read().await;
    tracing::info!(
        blocks_received = metrics.blocks_received.load(Ordering::Relaxed),
        blocks_committed = metrics.blocks_committed.load(Ordering::Relaxed),
        blocks_rejected_validation = metrics.blocks_rejected_validation.load(Ordering::Relaxed),
        blocks_rejected_execution = metrics.blocks_rejected_execution.load(Ordering::Relaxed),
        transactions_committed = metrics.transactions_committed.load(Ordering::Relaxed),
        total_gas_committed = metrics.total_gas_committed.load(Ordering::Relaxed),
        "shutdown metrics summary"
    );
    tracing::info!(
        final_head_number = final_head.number,
        final_head_hash = %final_head.hash,
        "node stopped cleanly"
    );

    Ok(())
}
