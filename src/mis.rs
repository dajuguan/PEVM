use std::collections::{BTreeMap, BTreeSet};

use crate::types::*;

pub struct TxRWSet {
    pub id: u64,
    pub reads: BTreeSet<FlatKey>,
    pub writes: BTreeSet<FlatKey>,
}

fn build_conflict_graph(txs: &Vec<TxRWSet>) -> BTreeMap<usize, BTreeSet<usize>> {
    let mut readers: BTreeMap<FlatKey, BTreeSet<usize>> = BTreeMap::new();
    let mut writers: BTreeMap<FlatKey, BTreeSet<usize>> = BTreeMap::new();
    for tx in txs {
        for k in &tx.reads {
            readers.entry(k.clone()).or_default().insert(tx.id as usize);
        }
        for k in &tx.writes {
            writers.entry(k.clone()).or_default().insert(tx.id as usize);
        }
    }

    let mut graph: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (key, ws) in &writers {
        // WW conflicts: pairwise connections among all writers
        for (i, a) in ws.iter().enumerate() {
            for b in ws.iter().skip(i + 1) {
                let (a, b) = (*a, *b);
                graph.entry(a).or_default().insert(b);
                graph.entry(b).or_default().insert(a);
            }
        }

        // WR + RW conflicts: writers <-> readers
        if let Some(rs) = readers.get(key) {
            for &w in ws {
                for &r in rs {
                    if w != r {
                        graph.entry(r).or_default().insert(w);
                    }
                }
            }
        }
    }

    // add others
    for i in 0..txs.len() {
        if !graph.contains_key(&i) {
            graph.insert(i, BTreeSet::new());
        }
    }
    return graph;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn set<T: Ord + Clone>(keys: &[T]) -> BTreeSet<T> {
        keys.iter().cloned().collect()
    }

    #[test]
    fn test_conflict_graph_multiple_cases() {
        struct TestCase {
            name: &'static str,
            txs: Vec<TxRWSet>,
            expected: Vec<Vec<usize>>,
        }

        let test_cases = vec![
            TestCase {
                name: "no_conflict_disjoint_keys",
                txs: vec![
                    TxRWSet {
                        id: 0,
                        reads: set(&[1]),
                        writes: set(&[2]),
                    },
                    TxRWSet {
                        id: 1,
                        reads: set(&[3]),
                        writes: set(&[4]),
                    },
                    TxRWSet {
                        id: 2,
                        reads: set(&[5]),
                        writes: set(&[6]),
                    },
                ],
                expected: vec![
                    vec![], // no conflicts
                    vec![],
                    vec![],
                ],
            },
            TestCase {
                name: "basic_rw_chain",
                txs: vec![
                    TxRWSet {
                        id: 0,
                        reads: set(&[0xa, 0xb]),
                        writes: set(&[0xc]),
                    },
                    TxRWSet {
                        id: 1,
                        reads: set(&[0xc]),
                        writes: set(&[0xd]),
                    },
                    TxRWSet {
                        id: 2,
                        reads: set(&[0xd]),
                        writes: BTreeSet::new(),
                    },
                ],
                expected: vec![
                    vec![],  // tx0 -> isolated
                    vec![0], // tx1 -> tx0
                    vec![1], // tx2 -> tx1
                ],
            },
            TestCase {
                name: "ww_conflict_cycle",
                txs: vec![
                    TxRWSet {
                        id: 0,
                        reads: set(&[]),
                        writes: set(&[1]),
                    },
                    TxRWSet {
                        id: 1,
                        reads: set(&[]),
                        writes: set(&[1]),
                    },
                ],
                expected: vec![
                    vec![1], // tx0 -> tx1
                    vec![0], // tx1 -> tx0
                ],
            },
            TestCase {
                name: "mixed_ww_wr_rw",
                txs: vec![
                    TxRWSet {
                        id: 0,
                        reads: set(&[]),
                        writes: set(&[10]),
                    },
                    TxRWSet {
                        id: 1,
                        reads: set(&[10]),
                        writes: set(&[11]),
                    },
                    TxRWSet {
                        id: 2,
                        reads: set(&[11]),
                        writes: set(&[10]),
                    },
                ],
                expected: vec![
                    vec![2],    // tx0 -> tx2
                    vec![0, 2], // tx1 -> tx0, tx2
                    vec![0, 1], // tx2 -> tx0,.tx1
                ],
            },
        ];

        for tcase in test_cases {
            let g = build_conflict_graph(&tcase.txs);

            // Convert graph to Vec<Vec<usize>>
            let mut got: Vec<Vec<usize>> = Vec::new();
            for id in 0..g.len() {
                let mut neighbors: Vec<_> = g
                    .get(&id)
                    .map(|s| s.iter().cloned().collect())
                    .unwrap_or_default();
                neighbors.sort();
                got.push(neighbors);
            }

            assert_eq!(
                got, tcase.expected,
                "Conflict graph mismatch for case: {}",
                tcase.name
            );
        }
    }
}
