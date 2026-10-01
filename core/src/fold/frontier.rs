use crate::event::{Hash, TodoEvent};
use crate::fold::order::{maximal, Discrete, Order};
use crate::fold::Register;
use crate::registers::Stamp;

/// The writes to one register that no other write descends from, sorted by
/// name. Empty is unwritten; more than one is a conflict.
#[derive(Debug, Clone, PartialEq)]
pub struct Frontier<'a, P>(Vec<&'a Stamp<P>>);

impl<P> Default for Frontier<'_, P> {
    fn default() -> Self {
        Frontier(Vec::new())
    }
}

impl<'a, P> Register for Frontier<'a, P> {
    type Write = &'a Stamp<P>;

    /// The maximal writes of the union, whatever order they arrive in.
    fn join(&mut self, write: &'a Stamp<P>) {
        if self
            .0
            .iter()
            .any(|held| held.name == write.name || held.descends(write))
        {
            return;
        }
        self.0.retain(|held| !write.descends(held));
        let at = self.0.partition_point(|held| held.name < write.name);
        self.0.insert(at, write);
    }
}

impl<'a, P: PartialEq> Frontier<'a, P> {
    pub fn writes(&self) -> &[&'a Stamp<P>] {
        &self.0
    }

    pub fn names(&self) -> Vec<Hash> {
        self.0.iter().map(|stamp| stamp.name.clone()).collect()
    }

    pub fn is_conflict(&self) -> bool {
        self.0.len() > 1
    }

    /// The register's reading: the writes whose values are maximal under
    /// the values' order, each value named by the least write carrying it.
    pub fn read<V: Order>(&self, value: impl Fn(&'a Stamp<P>) -> V) -> Vec<&'a Stamp<P>> {
        maximal(&self.0, value)
    }

    /// The reading under the discrete order on events, the todo registers'
    /// own: agreeing twins are one candidate. The value is the whole event,
    /// not the field the register reads, so two events that agree on a spec
    /// stay two candidates as they always were.
    pub fn candidates(&self) -> Vec<&'a Stamp<P>> {
        self.read(|stamp| Discrete::<&TodoEvent<P>>(&stamp.event))
    }
}
