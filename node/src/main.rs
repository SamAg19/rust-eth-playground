use std::{collections::HashMap, sync::Arc};

use clap::Parser;
use execution::{InMemoryProvider, executor::compute_state_root};
use processor::block_processor::BlockProcessor;
use rlp_codec::{hash_header, signing::{recover_sender, sign}};
use tokio::sync::RwLock;
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

    tracing_subscriber::fmt().with_max_level(level).compact().init();

    let dummy_tx = Transaction::Legacy { 
        nonce: 0, 
        gas_price: 1_000_000_000, 
        gas_limit: 21_000, 
        to: None, 
        value: 0, 
        data: vec![] 
    };

    let mut genesis_accounts: Vec<GenesisAccount> = vec![];

    for i in 0..cli.genesis_account_count {
        let private_key: [u8; 32] = [i+1; 32];
        let signed_transaction = sign(&dummy_tx, &private_key, cli.chain_id)?;
        let address = recover_sender(&signed_transaction, cli.chain_id)?;
        genesis_accounts.push(GenesisAccount { address, balance: cli.genesis_balance });
    }

    let genesis_config = GenesisConfig {
        chain_id: cli.chain_id,
        accounts: genesis_accounts,
        genesis_timestamp: cli.genesis_timestamp
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
        tracing::info!("Address: {}, Balance: {} ETH", genesis_account.address, genesis_account.balance/(1e18 as u128));
        let mut account = Account::default();
        account.balance = genesis_account.balance;
        provider.set_account(genesis_account.address, account.clone());
        accounts.insert(genesis_account.address, account);
    }

    let state_root = compute_state_root(&provider)?;
    tracing::info!("Genesis State Root: {}", state_root);

    let genesis_header = genesis_config.genesis_header_with_state_root(state_root);
    let genesis_block = Block {
        header: genesis_header.clone(),
        transactions: vec![]
    };

    let genesis_hash = hash_header(&genesis_header)?;

    tracing::info!("Genesis Block Root: {}", genesis_hash);

    if cli.dry_run {
        return Ok(());
    }

    let chain_head = ChainHead {
        number: 0,
        hash: genesis_hash,
        total_difficulty: 0
    };

    let shared_head = Arc::new(RwLock::new(chain_head));

    let _processor = BlockProcessor::new(genesis_block, accounts, genesis_config.chain_id, shared_head.clone()).await?;

    Ok(())
}
