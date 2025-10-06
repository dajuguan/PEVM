use clap::Parser;
use hex::{decode, encode};
use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::BufReader;
use std::time::Instant;

pub type Address = [u8; 20];
pub type Slot = [u8; 32];
pub type FlatKey = u64;
pub type FlatValue = u64;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub address: Address,
    pub slot: Slot,
}

impl Serialize for Key {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Key", 2)?;
        s.serialize_field("address", &format!("0x{}", encode(self.address)))?;
        s.serialize_field("slot", &format!("0x{}", encode(self.slot)))?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct KeyHelper {
            address: String,
            slot: String,
        }

        let helper = KeyHelper::deserialize(deserializer)?;

        // strip "0x"
        let addr_bytes =
            decode(helper.address.trim_start_matches("0x")).map_err(D::Error::custom)?;
        let slot_bytes = decode(helper.slot.trim_start_matches("0x")).map_err(D::Error::custom)?;

        if addr_bytes.len() != 20 {
            return Err(D::Error::custom("address must be 20 bytes"));
        }
        if slot_bytes.len() != 32 {
            return Err(D::Error::custom("slot must be 32 bytes"));
        }

        let mut address = [0u8; 20];
        address.copy_from_slice(&addr_bytes);

        let mut slot = [0u8; 32];
        slot.copy_from_slice(&slot_bytes);

        Ok(Key { address, slot })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tx {
    pub id: u64,
    pub reads: Vec<Key>,
    pub writes: Vec<Key>,
    pub gas_hint: u64,
    pub metadata: Option<String>,
    pub program: Vec<MicroOp>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MicroOp {
    SLOAD { key: Key },
    SSTORE { key: Key },
    ADD { imm: FlatValue },
    NOOP,
}

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

#[derive(Debug)]
pub struct TxExecResult {
    pub id: u64,
    pub reads: HashSet<FlatKey>,
    pub writes: HashSet<FlatKey>,
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

fn exec_tx(tx: &Tx, state: &mut HashMap<FlatKey, FlatValue>) -> TxExecResult {
    let mut reads = HashSet::new();
    let mut writes = HashSet::new();
    let mut acc = 0;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    for op in tx.program.iter() {
        match op {
            MicroOp::SLOAD { key } => {
                key.hash(&mut hasher);
                let h = hasher.finish();

                let v = state.get(&h).cloned().unwrap_or_else(|| 0);
                acc += v;
                reads.insert(h);
            }
            MicroOp::SSTORE { key } => {
                key.hash(&mut hasher);
                let h = hasher.finish();

                state.insert(h, acc);
                writes.insert(h);
            }
            MicroOp::ADD { imm } => {
                acc += *imm;
            }
            MicroOp::NOOP => {}
        }
    }

    TxExecResult {
        id: tx.id,
        reads,
        writes,
    }
}

fn serial_execute(txs: &[Tx]) -> (HashMap<FlatKey, FlatValue>, Vec<TxExecResult>) {
    let mut state: HashMap<FlatKey, FlatValue> = HashMap::new();
    let mut results: Vec<TxExecResult> = Vec::new();

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
