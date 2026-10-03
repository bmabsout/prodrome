use std::collections::BTreeMap;
use std::fmt::Debug;

use crate::event::Hash;
use crate::fold::Frontier;
use crate::literal::ProdromeError;
use crate::registers::Stamp;
use crate::schema::Schema;

/// An entity's registers: a product of semilattices, one per register, each
/// joined by the writes its schema routes to it. Joined componentwise, the
/// product is itself a semilattice, so folding a stream into it is a monoid
/// action whatever order the writes arrive in (§6.6).
///
/// The registers a write supersedes in are [`Frontier`]s, named by
/// [`Schema::Register`]; a register that only accumulates (the todo's
/// tendings, a [`crate::fold::GrowSet`]) is a component too, and is joined by
/// union. A component reads itself under its [`crate::fold::RegisterType`].
///
/// One product is one schema's, so the schema is an associated type.
pub trait Product<'a>: Default + Clone + Debug + PartialEq {
    type Schema: Schema;

    /// The registers a write supersedes in that this product holds, in the
    /// order a reading lists them: a Rust schema's every one, a declared
    /// schema's those of the declaration its writes were made at (none,
    /// before any was joined: an empty product has nothing to read).
    fn registers(&self) -> Vec<<Self::Schema as Schema>::Register>;

    /// The route: `stamp` joined into every register its event writes.
    fn join(&mut self, stamp: &'a Stamp<Self::Schema>);

    /// The frontier of a register a write supersedes in.
    fn frontier(&self, register: <Self::Schema as Schema>::Register)
        -> &Frontier<'a, Self::Schema>;

    /// The same, to join a write into that register alone: how the fold
    /// builds each register's earliest writes (§6.4).
    fn frontier_mut(
        &mut self,
        register: <Self::Schema as Schema>::Register,
    ) -> &mut Frontier<'a, Self::Schema>;

    /// A register's READING, by name (design §3): the writes in its frontier
    /// whose values are maximal under its type's order, each value named by
    /// the least write carrying it. One write is a value; more is a conflict.
    /// [`Frontier::reading`] under the register's [`crate::fold::RegisterType`],
    /// which only the product knows, so a reader that holds a register's name
    /// alone can read any schema.
    fn reading(&self, register: <Self::Schema as Schema>::Register)
        -> Vec<&'a Stamp<Self::Schema>>;

    /// The append's refusal (design §3.1): where `event` writes a register
    /// whose type is inflationary, its value must stand at or above that
    /// register's reading, which it supersedes. A schema with no inflationary
    /// register answers `Ok`.
    ///
    /// # Errors
    ///
    /// A write whose value falls below, or beside, the reading it supersedes.
    fn grows(&self, event: &Self::Schema) -> Result<(), ProdromeError>;

    /// Every register with more than one write in its frontier, each named
    /// by its writes: what a human settles with the next one.
    fn conflicts(&self) -> BTreeMap<<Self::Schema as Schema>::Register, Vec<Hash>> {
        self.registers()
            .into_iter()
            .filter(|register| self.frontier(*register).is_conflict())
            .map(|register| (register, self.frontier(register).names()))
            .collect()
    }
}
