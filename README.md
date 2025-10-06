# PEVM
A minimal implementation of parallel evm


## Design
Synthetic txs:
- contract: 
    - read simple slot
    - read map slots
    - read array slots
- tx: sender, to, calldata
- simplest: isolate address which write the same to(contract)
    - we don't know if they will access which slot on just given calldata