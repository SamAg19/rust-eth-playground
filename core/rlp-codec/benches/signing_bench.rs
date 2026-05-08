use criterion::{Criterion, criterion_group, criterion_main};
use rlp_codec::signing::{recover_sender, sign};
use types::{Address, Transaction};

const TEST_PRIVATE_KEY: [u8; 32] = [
    0x4c, 0x0c, 0x4d, 0x14, 0x6c, 0x46, 0xed, 0x91, 0xf6, 0xa9, 0x35, 0x09, 0x46, 0xa3, 0x69, 0x9e,
    0xb1, 0xfb, 0xc1, 0x9c, 0x91, 0xc8, 0x10, 0xe6, 0xb6, 0xa7, 0x8b, 0x0c, 0xa6, 0x06, 0x65, 0x6f,
];

const CHAIN_ID: u64 = 1;

fn legacy_tx() -> Transaction {
    Transaction::Legacy {
        nonce: 7,
        gas_price: 1_000_000_000,
        gas_limit: 21_000,
        to: Some(Address::new([0x22; 20])),
        value: 12_345,
        data: vec![],
    }
}

fn signing_bench(c: &mut Criterion) {
    let tx = legacy_tx();
    let signed = sign(&tx, &TEST_PRIVATE_KEY, CHAIN_ID).expect("benchmark tx should sign");

    let mut group = c.benchmark_group("signing");
    group.bench_function("sign_legacy", |b| {
        b.iter(|| sign(&tx, &TEST_PRIVATE_KEY, CHAIN_ID).expect("benchmark tx should sign"))
    });
    group.bench_function("recover_sender_legacy", |b| {
        b.iter(|| recover_sender(&signed, CHAIN_ID).expect("benchmark sender should recover"))
    });
    group.finish();
}

criterion_group!(benches, signing_bench);
criterion_main!(benches);
