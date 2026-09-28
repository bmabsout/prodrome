use crate::event::Hash;
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

    /// The distinct events: agreeing twins are one candidate, named by the
    /// least of them.
    pub fn candidates(&self) -> Vec<&'a Stamp<P>> {
        let mut out: Vec<&'a Stamp<P>> = Vec::with_capacity(self.0.len());
        for stamp in &self.0 {
            if !out.iter().any(|held| held.event == stamp.event) {
                out.push(stamp);
            }
        }
        out
    }
}
