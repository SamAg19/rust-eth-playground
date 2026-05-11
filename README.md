# Mini ETH Node

A small Ethereum-like execution node built from first principles in Rust. The project implements the core pieces needed to accept signed value-transfer blocks over a TCP protocol, validate parent/head relationships, execute transactions against in-memory state, update the chain head, expose basic account/head queries, and drive the node with a deterministic test client.

The code is intentionally compact and educational, but the architecture follows real client boundaries: protocol messages stay in the networking layer, the processor owns canonical state, execution lives in its own crate, and the block builder only creates signed block envelopes from node-provided state.

## Scope

Implemented:

- Ethereum-style primitive types: addresses, hashes, headers, blocks, accounts, transactions, signed transactions, genesis config, and chain head.
- RLP encoding/decoding for core types and networking messages.
- Keccak/RLP header hashing.
- ECDSA transaction signing and sender recovery.
- Merkle Patricia Trie state-root calculation for account state.
- In-memory provider with block, transaction, receipt, account, and storage maps.
- Journal rollback for execution state changes.
- Value-transfer execution pipeline with validation, receipts, gas accounting, nonce checks, and balance updates.
- Block processor with pending queue, out-of-order buffering, parent/head validation, sender recovery, execution, block commitment, metrics, and stale-block rejection.
- Async TCP networking with handshake, framed messages, peer manager, ping/pong, block forwarding, account-state query, and chain-head query.
- Node binary that wires processor, networking manager, listener, shared chain head, metrics, tracing, and graceful shutdown.
- Test client binary that reconstructs deterministic genesis keys, handshakes with the node, resumes from the node's current head, queries account state, builds signed blocks, sends them one at a time, and waits for processing before continuing.

Not implemented:

- Persistent database storage.
- Fork choice or competing chain handling.
- Full Ethereum consensus validation.
- EVM bytecode execution.
- Contract storage changes through transactions.
- Transaction pool.
- Real devp2p discovery/encryption.

## Workspace Layout

```text
mini-eth-node/
├── block-builder/        # Builds signed blocks from deterministic keys and node account snapshots
├── core/
│   ├── execution/        # Provider traits, in-memory provider, executor, validator, pipeline
│   ├── networking/       # TCP protocol, codec, connection tasks, peer manager
│   ├── rlp-codec/        # RLP, signing, hashing, trie
│   └── types/            # Domain types shared by all crates
├── node/                 # Node binary and test-client binary
└── processor/            # BlockProcessor, ProcessorMessage, metrics, processor errors
```

## Architecture

The node runs three long-lived async tasks:

- Listener: accepts TCP connections and spawns per-peer connection tasks.
- Manager: tracks peers, routes inbound network messages, handles pings, and sends targeted outbound messages.
- Processor: owns canonical block/state execution through its pipeline provider.

The networking crate does not depend on the processor crate. Instead, the node binary acts as an adapter:

```text
peer connection
  -> networking manager
  -> NetworkEvent
  -> node adapter
  -> ProcessorMessage
  -> BlockProcessor
```

Responses such as account state and chain head go back through the manager using `PeerEvent::SendMessage`.

The processor owns authoritative state. Networking and the test client do not access the provider directly. Account nonces and balances are queried through protocol messages:

```text
GetAccountState -> AccountState
GetChainHead    -> ChainHead
```

The block builder does not execute transactions or predict state. It signs transactions using account snapshots returned by the node and maintains only local block envelope state: current number, current hash, deterministic timestamp, chain ID, and signing keys.

## Running

Build everything:

```sh
cargo build --workspace
```

Run all tests:

```sh
cargo test --workspace
```

Start the node:

```sh
cargo run --bin node
```

Run with debug logs:

```sh
cargo run --bin node -- --log-level debug
```

Inspect node configuration without starting networking:

```sh
cargo run --bin node -- --dry-run
```

In another terminal, run the test client:

```sh
cargo run --bin test-client
```

The default client sends 10 blocks with 3 signed value-transfer transactions per block. It queries the node for each sender's account state before building a block, sends the block, then waits until sender nonces advance before sending the next block.

You can run the client again without restarting the node. It queries the current chain head and resumes from there, so a second default run sends blocks 11 through 20.

Stop the node with `Ctrl-C`. The node logs a shutdown metrics summary including received blocks, committed blocks, validation rejections, execution rejections, committed transactions, total gas, and final chain head.

## Useful Commands

Node help:

```sh
cargo run --bin node -- --help
```

Test client help:

```sh
cargo run --bin test-client -- --help
```

Run only processor tests:

```sh
cargo test -p processor
```

Run only networking tests:

```sh
cargo test -p networking --lib
```

Run only block builder tests:

```sh
cargo test -p block-builder
```

## Current Protocol Messages

The TCP protocol uses a 4-byte big-endian frame length, a 1-byte message tag, and an RLP-encoded payload.

Supported messages include:

- `Ping`
- `Pong`
- `Status { chain_id, head_hash, total_difficulty }`
- `Transactions { txs }`
- `GetBlockHeaders { start_hash, count }`
- `NewBlock { block, td }`
- `NewBlockHashes { new_blocks }`
- `BlockHeaders { headers }`
- `Disconnect { reason }`
- `GetAccountState { address }`
- `AccountState { address, nonce, balance }`
- `GetChainHead`
- `ChainHead { number, hash, total_difficulty }`

## Design Rules

- Implementation code should return `Result` and avoid `unwrap`.
- Tests may use `unwrap` where failure should abort the test immediately.
- Processor state is authoritative.
- Networking remains protocol/routing focused and does not import processor types.
- The block builder does not duplicate execution logic.
- Generated blocks currently use placeholder header roots for state and transactions; execution output logs the computed state root after processing.
