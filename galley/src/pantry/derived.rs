//! Every derived value is reachable from a key some live book names, or it
//! is gone.
//!
//! ```text
//! store.insert(key, value)            the value is a function of the key
//! store.get(&key)          -> Some(v) a hit reads nothing else
//! store.keep_live(|key| …)            everything unnamed is dropped
//! store.resident_bytes(weigh) -> 4_112   one key per row, plus the value
//! ```
//!
//! A key names everything its value is a function of, so nothing here is ever
//! invalidated: an input that moved simply misses. That is what makes a sweep
//! the only eviction rule a derived store needs — reachability, never age.

use core::hash::Hash;
use core::ops::Index;

use rustc_hash::FxHashMap;

/// One kind of derived value, keyed by what it is a function of.
pub struct Store<K, V> {
    entries: FxHashMap<K, V>,
}

impl<K, V> Default for Store<K, V> {
    fn default() -> Self {
        Self {
            entries: FxHashMap::default(),
        }
    }
}

impl<K: Eq + Hash, V> Store<K, V> {
    pub fn get(&self, key: &K) -> Option<&V> {
        self.entries.get(key)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.entries.get_mut(key)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.entries.insert(key, value)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.entries.remove(key)
    }

    /// The vacant/occupied door, for a caller that fills a miss in place.
    pub fn entry(&mut self, key: K) -> std::collections::hash_map::Entry<'_, K, V> {
        self.entries.entry(key)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.values()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// THE LIVENESS PREDICATE: keeps exactly the keys the caller still names.
    ///
    /// The one eviction rule here — a value whose key nothing names cannot be
    /// asked for again, so dropping it is not a policy but arithmetic.
    pub fn keep_live(&mut self, live: impl FnMut(&K) -> bool) {
        let mut live = live;
        self.entries.retain(|key, _| live(key));
    }

    /// THE BYTE ACCOUNTING: one key's worth per row plus whatever `weigh`
    /// says the value hangs off the heap.
    pub fn resident_bytes(&self, weigh: impl Fn(&V) -> usize) -> usize {
        self.entries
            .values()
            .map(|value| size_of::<K>() + weigh(value))
            .sum()
    }
}

impl<K: Eq + Hash, V> Index<&K> for Store<K, V> {
    type Output = V;

    fn index(&self, key: &K) -> &V {
        &self.entries[key]
    }
}
