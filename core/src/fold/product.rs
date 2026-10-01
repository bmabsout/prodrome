use std::fmt::Debug;

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
pub trait Product<'a, E: Schema>: Default + Clone + Debug + PartialEq {
    /// The route: `stamp` joined into every register its event writes.
    fn join(&mut self, stamp: &'a Stamp<E>);

    /// The frontier of a register a write supersedes in.
    fn frontier(&self, register: E::Register) -> &Frontier<'a, E>;

    /// The append's refusal (design §3.1): where `event` writes a register
    /// whose type is inflationary, its value must stand at or above that
    /// register's reading, which it supersedes. A schema with no inflationary
    /// register answers `Ok`.
    ///
    /// # Errors
    ///
    /// A write whose value falls below, or beside, the reading it supersedes.
    fn grows(&self, event: &E) -> Result<(), ProdromeError>;
}
