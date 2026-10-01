use std::collections::BTreeSet;

/// A partial order on a register's values: reflexive, antisymmetric and
/// transitive (§9). It decides which concurrent values are redundant, never
/// which write supersedes which; ancestry alone does that.
pub trait Order {
    fn le(&self, other: &Self) -> bool;
}

/// Equal or incomparable: the default, under which every distinct value is
/// its own candidate. `PartialEq` and not `Eq`, since an event holds floats;
/// the smart constructors keep them finite, so `==` is an equivalence on
/// every value a register holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Discrete<T>(pub T);

impl<T: PartialEq> Order for Discrete<T> {
    fn le(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

/// A total order: a register under it always reads one value, the greatest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Total<T>(pub T);

impl<T: Ord> Order for Total<T> {
    fn le(&self, other: &Self) -> bool {
        self.0 <= other.0
    }
}

/// Sets under inclusion: the grow-only set's order.
impl<T: Ord> Order for BTreeSet<T> {
    fn le(&self, other: &Self) -> bool {
        self.is_subset(other)
    }
}

/// The free completion of the order: the items whose values are maximal,
/// one per value, the first that carries it. One is a value; more is a
/// conflict. Under [`Discrete`] it is every distinct value.
pub fn maximal<T: Copy, V: Order>(items: &[T], value: impl Fn(T) -> V) -> Vec<T> {
    let mut out: Vec<T> = Vec::with_capacity(items.len());
    for &item in items {
        let v = value(item);
        let dominated = items.iter().any(|&other| {
            let w = value(other);
            v.le(&w) && !w.le(&v)
        });
        let held = out.iter().any(|&kept| {
            let w = value(kept);
            v.le(&w) && w.le(&v)
        });
        if !dominated && !held {
            out.push(item);
        }
    }
    out
}
