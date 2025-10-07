# PEVM
A minimal implementation of a parallel EVM (Ethereum Virtual Machine) for research and experimentation.

## Quick start

## Generate Synthetic Transactions
```
cargo run -- --generate --n-tx 3 --out block.json
```

## Test Maximal Independent Set (MIS) and Conflict Graph
```
cargo test
```


## Design Overview

### Toy EVM State & Execution
- The EVM is simplified to use a register model (not a stack machine).
- Word size is u64; overflow is not handled.
- Supported instructions: SLOAD, SSTORE, ADD, and NOOP.
- A global accumulator (acc) simulates state changes:
    - SLOAD: acc += SLOAD(key)
    - SSTORE: set(key) = acc
    - ADD: acc += immediate

###  Per-tx program
- For each transaction, a program is generated:
    - For each read key: SLOAD + ADD(txid)
    - For each write key: ADD + SSTORE + SLOAD
- Transaction schema:
``
{
  "txid": ...,
  "reads": [{address, slot}],
  "writes": [{address, slot}],
  "program": [...]
}
``


### Synthetic Block & Tx Model
- Blocks are generated with random transactions.
- Each transaction contains randomly generated read and write sets.
- The generator partially supports configuration of key space, conflict ratio, cold ratio, and random seed.

## Code Structure
- `main.rs`: CLI, block/tx generation, execution logic.
- `types.rs`: Core types (`Tx`, `Key`, `MicroOp`, etc.).
- `db.rs`: State database trait and implementation.
- `mis.rs`: Conflict graph and maximal independent set logic.

## Features
- [x] Synthetic Block & Tx Model
- [x] Toy-EVM State & Execution (most simplified)
- [ ] Access-List Builder
- [ ] Conflict Graph & Scheduler
    - [x] Conflict Graph
- [ ] Correctness & Determinism
- [ ] Fees / Gas (Lightweight)
- [ ] Metrics & Outpu