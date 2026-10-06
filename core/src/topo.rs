use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

/// The crate's one causal order: Kahn's walk with a min-heap on the key,
/// so every key comes after the keys it rests on among `rests_on`'s keys,
/// and keys no order relates come in key order. It is a function of the
/// relation alone, not of the order it was given in. A key resting on one
/// that is not among them is passed over; `Err` names the least key a
/// cycle leaves unplaced.
pub fn linear<'k, K: Ord + ?Sized>(
    rests_on: &BTreeMap<&'k K, Vec<&'k K>>,
) -> Result<Vec<&'k K>, &'k K> {
    let mut above: BTreeMap<&K, Vec<&K>> = BTreeMap::new();
    let mut waiting: BTreeMap<&K, usize> = BTreeMap::new();
    for (&key, below) in rests_on {
        let count = waiting.entry(key).or_default();
        for &under in below {
            if rests_on.contains_key(under) {
                above.entry(under).or_default().push(key);
                *count += 1;
            }
        }
    }
    let mut ready: BinaryHeap<Reverse<&K>> = waiting
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(key, _)| Reverse(*key))
        .collect();
    let mut order = Vec::with_capacity(waiting.len());
    while let Some(Reverse(key)) = ready.pop() {
        order.push(key);
        for &over in above.get(key).into_iter().flatten() {
            let count = waiting.get_mut(over).expect("every key is counted");
            *count -= 1;
            if *count == 0 {
                ready.push(Reverse(over));
            }
        }
    }
    if order.len() == waiting.len() {
        Ok(order)
    } else {
        Err(waiting
            .iter()
            .find(|(_, count)| **count > 0)
            .map(|(key, _)| *key)
            .expect("a short order left a key waiting"))
    }
}

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

#[cfg(test)]
mod tests {
    use super::linear;
    use std::collections::BTreeMap;

    #[test]
    fn parents_first_and_ties_by_key() {
        // c rests on d; a and b on nothing: a, b, d, then c.
        let rests_on: BTreeMap<&str, Vec<&str>> = BTreeMap::from([
            ("c", vec!["d"]),
            ("a", vec![]),
            ("d", vec!["x"]),
            ("b", vec![]),
        ]);
        assert_eq!(linear(&rests_on), Ok(vec!["a", "b", "d", "c"]));
    }

    #[test]
    fn a_cycle_names_its_least_key() {
        let rests_on: BTreeMap<&str, Vec<&str>> =
            BTreeMap::from([("a", vec![]), ("b", vec!["c"]), ("c", vec!["b"])]);
        assert_eq!(linear(&rests_on), Err("b"));
    }
}
