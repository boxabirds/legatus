//! Deterministic iteration: entries are visited in sorted key order (PRX-TEST-008).
//! No hash-order helper is exported from this crate.
use std::collections::BTreeMap;

pub fn sorted_keys<K: Ord + Clone, V>(map: &BTreeMap<K, V>) -> Vec<K> {
    map.keys().cloned().collect()
}
