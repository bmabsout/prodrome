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
