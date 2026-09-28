use std::collections::BTreeSet;

/// A join-semilattice: joining the same writes in any order is one value.
pub trait Register: Default {
    type Write;

    fn join(&mut self, write: Self::Write);
}

/// A grow-only set, joined by union: the tendings, which never conflict.
pub type GrowSet<T> = BTreeSet<T>;

impl<T: Ord> Register for GrowSet<T> {
    type Write = T;

    fn join(&mut self, write: T) {
        self.insert(write);
    }
}
