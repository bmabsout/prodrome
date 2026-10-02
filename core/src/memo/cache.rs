//! The cache as a store (design §6.2): a relation from keys to values, joined
//! by union, read as a partial function.
//!
//! A [`Key`] names one computation: a function's name and its argument's.
//! Each key is a register in the FLAT order: unknown (no entry), then one
//! value. Two caches merge by union of their pairs, which is associative,
//! commutative and idempotent, so caches sync as replicas do (§6.1) by
//! [`Cache::join`]. Because the function is pure, two caches that computed
//! one key hold one value there; a key that comes to hold two is a
//! [`Finding`] (the function was not pure, or its key left out something it
//! read, or a writer lied), held as evidence and never chosen between.
//!
//! An entry may be dropped at any time: nothing rests on it, and a fold that
//! needs it computes it again. Which entries go is an [`Evict`] policy's
//! business; [`Keep`] drops none and [`Lru`] keeps a bounded number, the
//! least recently used going first.
//!
//! This is not an [`crate::store::Replica`], on purpose. An object in a
//! history is never deleted, is verified by rehashing and belongs in a
//! checkpoint; an entry here is deleted freely, is verified by recomputing
//! and stays out of every checkpoint. The two never share a type, so no
//! entry can be received into a history, or an object evicted from one.

use std::collections::{BTreeMap, HashMap};

use crate::event::Hash;

/// What an entry tabulates: the function's name (its code and everything
/// that code reads) and its argument's name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    /// The function's name.
    pub function: Hash,
    /// The argument's name.
    pub argument: Hash,
}

/// A key holding more than one value: the function was not pure, or its key
/// left out something it read, or a writer forged an entry. Reported, never
/// resolved by picking one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The key that holds the values.
    pub key: Key,
}

/// What a key holds.
#[derive(Debug, PartialEq, Eq)]
pub enum Lookup<'a, V> {
    /// No entry: the bottom of the flat order.
    Miss,
    /// One value.
    Hit(&'a V),
    /// Two or more distinct values, each as it arrived: a [`Finding`].
    Finding(&'a [V]),
}

/// Which entries a cache drops, told of every key it touches and asked for
/// a victim after every insert. A policy decides what is remembered, never
/// what anything means: dropping an entry moves the cache down the
/// information order.
pub trait Evict {
    /// `key` was read or written.
    fn touched(&mut self, key: &Key);
    /// `key` is gone, whoever dropped it.
    fn forgot(&mut self, key: &Key);
    /// The next key to drop from a cache of `held` keys, or `None` to stop.
    fn victim(&mut self, held: usize) -> Option<Key>;
}

/// The policy that drops nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct Keep;

impl Evict for Keep {
    fn touched(&mut self, _: &Key) {}
    fn forgot(&mut self, _: &Key) {}
    fn victim(&mut self, _: usize) -> Option<Key> {
        None
    }
}

/// At most `capacity` keys, the least recently touched dropped first.
#[derive(Debug, Clone)]
pub struct Lru {
    capacity: usize,
    clock: u64,
    last: HashMap<Key, u64>,
    order: BTreeMap<u64, Key>,
}

impl Lru {
    /// A policy holding at most `capacity` keys.
    #[must_use]
    pub fn new(capacity: usize) -> Lru {
        Lru {
            capacity,
            clock: 0,
            last: HashMap::new(),
            order: BTreeMap::new(),
        }
    }
}

impl Evict for Lru {
    fn touched(&mut self, key: &Key) {
        self.clock += 1;
        if let Some(was) = self.last.insert(key.clone(), self.clock) {
            self.order.remove(&was);
        }
        self.order.insert(self.clock, key.clone());
    }

    fn forgot(&mut self, key: &Key) {
        if let Some(was) = self.last.remove(key) {
            self.order.remove(&was);
        }
    }

    fn victim(&mut self, held: usize) -> Option<Key> {
        if held > self.capacity {
            self.order.first_key_value().map(|(_, key)| key.clone())
        } else {
            None
        }
    }
}

/// A set of (key, value) pairs, read as a partial function where it is one.
#[derive(Debug, Clone)]
pub struct Cache<V, E = Keep> {
    entries: HashMap<Key, Vec<V>>,
    evict: E,
}

impl<V> Default for Cache<V> {
    fn default() -> Self {
        Cache {
            entries: HashMap::new(),
            evict: Keep,
        }
    }
}

impl<V: Clone + Eq, E: Evict> Cache<V, E> {
    /// An empty cache under `evict`.
    pub fn new(evict: E) -> Self {
        Cache {
            entries: HashMap::new(),
            evict,
        }
    }

    /// What `key` holds. Reading touches it, for the policy.
    pub fn get(&mut self, key: &Key) -> Lookup<'_, V> {
        match self.entries.get(key) {
            None => Lookup::Miss,
            Some(values) => {
                self.evict.touched(key);
                match values.as_slice() {
                    [value] => Lookup::Hit(value),
                    values => Lookup::Finding(values),
                }
            }
        }
    }

    /// Adds the pair `(key, value)`. The pair is held either way; `Err`
    /// says the key now holds more than one value. The policy may then drop
    /// entries, this one among them.
    ///
    /// # Errors
    ///
    /// A [`Finding`] when `key` already held a different value.
    pub fn insert(&mut self, key: Key, value: V) -> Result<(), Finding> {
        self.evict.touched(&key);
        let held = self.add(key, value);
        self.shrink();
        held
    }

    /// The union of this cache and `other`, into this one: every pair of
    /// `other` added. Answers every key that holds two values after the
    /// join and did not before.
    pub fn join<F>(&mut self, other: &Cache<V, F>) -> Vec<Finding> {
        let mut findings = Vec::new();
        for (key, values) in &other.entries {
            for value in values {
                if let Err(finding) = self.add(key.clone(), value.clone()) {
                    if !findings.contains(&finding) {
                        findings.push(finding);
                    }
                }
            }
            self.evict.touched(key);
        }
        self.shrink();
        findings
    }

    /// Drops `key` and every value it holds. Answers whether it held any.
    pub fn remove(&mut self, key: &Key) -> bool {
        self.evict.forgot(key);
        self.entries.remove(key).is_some()
    }

    /// Every key held, in no particular order.
    pub fn keys(&self) -> impl Iterator<Item = &Key> {
        self.entries.keys()
    }

    /// Every pair held, in no particular order: what a sync sends.
    pub fn pairs(&self) -> impl Iterator<Item = (&Key, &V)> {
        self.entries
            .iter()
            .flat_map(|(key, values)| values.iter().map(move |value| (key, value)))
    }

    /// Every key holding more than one value.
    pub fn findings(&self) -> impl Iterator<Item = Finding> + '_ {
        self.entries
            .iter()
            .filter(|(_, values)| values.len() > 1)
            .map(|(key, _)| Finding { key: key.clone() })
    }

    /// How many keys are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no key is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Adds a pair without the policy, answering a finding where the key
    /// now holds a second value for the first time.
    fn add(&mut self, key: Key, value: V) -> Result<(), Finding> {
        let values = self.entries.entry(key.clone()).or_default();
        if values.contains(&value) {
            return Ok(());
        }
        values.push(value);
        if values.len() == 2 {
            Err(Finding { key })
        } else {
            Ok(())
        }
    }

    fn shrink(&mut self) {
        while let Some(victim) = self.evict.victim(self.entries.len()) {
            if !self.remove(&victim) {
                break;
            }
        }
    }
}
