use std::collections::BTreeMap;

/// A depth-first walk that settles each key after what it expands into, with
/// the keys being expanded as the cycle witness.
pub struct Topo<K, V> {
    path: Vec<K>,
    settled: BTreeMap<K, V>,
    order: Vec<K>,
}

impl<K, V> Default for Topo<K, V> {
    fn default() -> Self {
        Topo {
            path: Vec::new(),
            settled: BTreeMap::new(),
            order: Vec::new(),
        }
    }
}

impl<K: Ord + Clone, V: Clone> Topo<K, V> {
    /// `key`'s value, expanded once. A key met again on its own path is a
    /// cycle, named from that key with it repeated at the end.
    pub fn settle<E>(
        &mut self,
        key: &K,
        cycle: impl FnOnce(Vec<K>) -> E,
        expand: impl FnOnce(&mut Self) -> Result<V, E>,
    ) -> Result<V, E> {
        if let Some(value) = self.settled.get(key) {
            return Ok(value.clone());
        }
        if let Some(from) = self.path.iter().position(|on| on == key) {
            let mut path = self.path[from..].to_vec();
            path.push(key.clone());
            return Err(cycle(path));
        }
        self.path.push(key.clone());
        let value = expand(self);
        self.path.pop();
        let value = value?;
        self.settled.insert(key.clone(), value.clone());
        self.order.push(key.clone());
        Ok(value)
    }

    /// Every settled key, each after what it expanded into.
    pub fn into_order(self) -> Vec<K> {
        self.order
    }
}
