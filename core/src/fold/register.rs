use std::collections::BTreeSet;

use crate::fold::Inflationary;

/// A join-semilattice: joining the same writes in any order is one value.
pub trait Register: Default {
    type Write;

    fn join(&mut self, write: Self::Write);
}

/// A grow-only set, joined by union: the tendings, which never conflict.
pub type GrowSet<T> = BTreeSet<T>;

/// Union never shrinks a set.
impl<T: Ord> Inflationary for GrowSet<T> {}

impl<T: Ord> Register for GrowSet<T> {
    type Write = T;

    fn join(&mut self, write: T) {
        self.insert(write);
    }
}
