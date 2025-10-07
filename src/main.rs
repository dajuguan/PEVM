use clap::Parser;
use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use std::collections::BTreeSet;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::BufReader;
use std::time::Instant;

mod db;
mod types;
use db::*;
use types::*;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    generate: bool,

    #[arg(long)]
    exec: bool,

    #[arg(long, default_value = "block.json")]
    in_file: String,

    #[arg(long, default_value = "block.json")]
    out: String,

    #[arg(long, default_value_t = 2)]
    n_tx: usize,

    #[arg(long, default_value_t = 1000)]
    key_space: usize,

    #[arg(long, default_value_t = 0.2)]
    conflict_ratio: f64,

    #[arg(long, default_value_t = 0.1)]
    cold_ratio: f64,

    #[arg(long, default_value_t = 42)]
    seed: u64,
}

fn random_address<R: Rng>(rng: &mut R) -> Address {
    let mut a = [0u8; 20];
    rng.fill_bytes(&mut a);
    a
}

fn random_slot<R: Rng>(rng: &mut R) -> Slot {
    let mut s = [0u8; 32];
    rng.fill_bytes(&mut s);
    s
}

fn key_from_idx(idx: usize, addr_pool: &[Address]) -> Key {
    let mut slot = [0u8; 32];
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    idx.hash(&mut hasher);
    let h = hasher.finish();
    for i in 0..8 {
        slot[i] = ((h >> (i * 8)) & 0xff) as u8;
    }
    let address = addr_pool[idx % addr_pool.len()];
    Key { address, slot }
}

/// Program: only SLOAD, SSTORE, ADD, KECCAK, NOOP.
/// First write = txid, subsequent writes increment by 1.
fn build_program_for_tx(txid: u64, reads: &[Key], writes: &[Key]) -> Vec<MicroOp> {
    let mut prog = Vec::new();

    // Read keys: load, then add something
    for r in reads {
        prog.push(MicroOp::SLOAD { key: r.clone() });
        prog.push(MicroOp::ADD { imm: txid });
    }

    // Write keys: first write txid, then increment by 1 each time
    for (i, w) in writes.iter().enumerate() {
        // simulate using ADD to adjust stack value to this target
        prog.push(MicroOp::ADD { imm: i as u64 });
        prog.push(MicroOp::SSTORE { key: w.clone() });
        prog.push(MicroOp::SLOAD { key: w.clone() });
    }

    prog.push(MicroOp::NOOP);
    prog
}

fn generate_block(args: &Args) -> Vec<Tx> {
    let mut rng = ChaCha20Rng::seed_from_u64(args.seed);
    let addr_pool: Vec<Address> = (0..10).map(|_| random_address(&mut rng)).collect();

    let hot_size = ((args.conflict_ratio * args.key_space as f64).max(1.0)) as usize;
    let hot_indices: Vec<usize> = (0..hot_size).collect();
    let neutral_indices: Vec<usize> = (hot_size..args.key_space).collect();

    let mut txs: Vec<Tx> = Vec::new();

    for i in 0..args.n_tx {
        let n_reads = 1 + (rng.next_u32() % 20);
        let n_writes = 1 + (rng.next_u32() % 20);
        let mut reads: Vec<Key> = Vec::new();
        let mut writes: Vec<Key> = Vec::new();

        for _ in 0..n_reads {
            let pick_idx = if rng.gen_bool(args.cold_ratio) {
                rng.gen_range(0..args.key_space)
            } else if rng.gen_bool(args.conflict_ratio) && !hot_indices.is_empty() {
                *hot_indices.choose(&mut rng).unwrap()
            } else if !neutral_indices.is_empty() {
                *neutral_indices.choose(&mut rng).unwrap()
            } else {
                rng.gen_range(0..args.key_space)
            };
            reads.push(key_from_idx(pick_idx, &addr_pool));
        }

        for _ in 0..n_writes {
            let pick_idx = if rng.gen_bool(args.cold_ratio) {
                rng.gen_range(0..args.key_space)
            } else if rng.gen_bool(args.conflict_ratio) && !hot_indices.is_empty() {
                *hot_indices.choose(&mut rng).unwrap()
            } else if !neutral_indices.is_empty() {
                *neutral_indices.choose(&mut rng).unwrap()
            } else {
                rng.gen_range(0..args.key_space)
            };
            writes.push(key_from_idx(pick_idx, &addr_pool));
        }

        let program = build_program_for_tx(i as u64, &reads, &writes);

        txs.push(Tx {
            id: i as u64,
            reads,
            writes,
            gas_hint: (n_reads + n_writes) as u64 * 10,
            metadata: None,
            program,
        });
    }

    txs
}

fn exec_tx(tx: &Tx, state: &mut impl StateDB) -> TxRWSet {
    let mut reads = BTreeSet::new();
    let mut writes = BTreeSet::new();
    let mut acc = 0;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    for op in tx.program.iter() {
        match op {
            MicroOp::SLOAD { key } => {
                key.hash(&mut hasher);
                let h = hasher.finish();

                let v = state.get_state(&h).unwrap();
                acc += v;
                reads.insert(h);
            }
            MicroOp::SSTORE { key } => {
                key.hash(&mut hasher);
                let h = hasher.finish();

                state.set_state(h, acc);
                writes.insert(h);
            }
            MicroOp::ADD { imm } => {
                acc += *imm;
            }
            MicroOp::NOOP => {}
        }
    }

    TxRWSet {
        id: tx.id,
        reads,
        writes,
    }
}

fn serial_execute(txs: &[Tx]) -> (MapState, Vec<TxRWSet>) {
    let mut state = MapState::new();
    let mut results: Vec<TxRWSet> = Vec::new();

    for tx in txs.iter() {
        let res = exec_tx(tx, &mut state);
        results.push(res);
    }
    (state, results)
}

fn main() {
    let args = Args::parse();

    if args.generate {
        let txs = generate_block(&args);
        let f = File::create(&args.out).expect("failed to create out file");
        serde_json::to_writer_pretty(f, &txs).expect("failed to write json");
        println!("Generated {} txs -> {}", txs.len(), args.out);
        return;
    }

    if args.exec {
        let f = File::open(&args.in_file).expect("failed to open in file");
        let reader = BufReader::new(f);
        let txs: Vec<Tx> = serde_json::from_reader(reader).expect("failed to parse json");
        println!("Loaded {} txs. Running serial execution...", txs.len());
        let t0 = Instant::now();
        let (state, results) = serial_execute(&txs[0..1]);
        let dt = t0.elapsed();
        println!(
            "Serial execution took: {:?} final state size={}",
            dt,
            state.len()
        );
        for r in results.iter() {
            println!(
                "tx {}: reads={} writes={}",
                r.id,
                r.reads.len(),
                r.writes.len()
            );
        }
        return;
    }

    println!("No action specified. Use --generate or --exec.");
}
