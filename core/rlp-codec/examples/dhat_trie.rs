use rlp_codec::trie::MerkleTrie;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _profiler = dhat::Profiler::new_heap();

    let mut trie = MerkleTrie::new();
    for i in 0usize..10_000 {
        trie.insert(&i.to_be_bytes(), i.to_be_bytes().to_vec())?;
    }
    let _root = trie.root_hash()?;
    Ok(())
}

// Day 9 allocation profiling note:
// Run with:
//   cargo run -p rlp-codec --example dhat_trie --release
//
// First run result:
//   Total:    3,636,330 bytes in 149,759 blocks
//   t-gmax:     595,289 bytes in 10,677 blocks
//   t-end:            0 bytes in 0 blocks
//
// The expected allocation hotspots are key/value Vec creation in the loop,
// nibble conversion during insert, and owned Vec fields stored inside trie
// nodes.
