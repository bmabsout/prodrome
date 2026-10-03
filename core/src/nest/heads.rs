//! A register whose value is a history (design §6.3), held by its heads.

use std::collections::BTreeSet;
use std::fmt;
use std::marker::PhantomData;

use crate::event::{hashes, hashes_value, sorted_distinct, Hash};
use crate::fold::{read, Order, Product};
use crate::literal::{Call, ProdromeError, Value};
use crate::policy::Everything;
use crate::registers::Stamp;
use crate::schema::Schema;

/// THE VALUE OF A REGISTER THAT HOLDS A HISTORY of the schema `I`: the
/// history's heads, as a git tree holds a subtree by its name. The history
/// is everything they rest on, in a replica of `I`'s objects; the inner
/// schema is the value's type, so a register is declared to hold histories
/// of one schema by a [`crate::fold::RegisterType`] whose values are
/// `&Heads<I>`, and never by a field of the object, which carries only the
/// names (a tuple, printed sorted as a change's deps are).
///
/// ORDERED BY INCLUSION OF THE HEADS. One set of heads within another names
/// a history within the other's, so the order is sound; a pair it cannot
/// compare (one history's heads beneath the other's, or two branches) is two
/// candidates, and a pointer is never read as one of them: its reading is
/// the UNION of their histories ([`pointer`]), the history's own join (§1),
/// which needs the inner objects to compute and so is not the order's. Not
/// inflationary: a write moves the pointer to whatever its writer saw, and
/// a writer that saw less is not refused, since what the outer history
/// holds of the inner one is everything any of its writes named.
pub struct Heads<I> {
    names: BTreeSet<Hash>,
    inner: PhantomData<fn() -> I>,
}

impl<I> Heads<I> {
    pub fn new(names: impl IntoIterator<Item = Hash>) -> Heads<I> {
        Heads {
            names: names.into_iter().collect(),
            inner: PhantomData,
        }
    }

    /// The heads, each an object of `I`.
    #[must_use]
    pub fn names(&self) -> &BTreeSet<Hash> {
        &self.names
    }

    /// The field's print: a tuple of the names, sorted.
    #[must_use]
    pub fn to_value(&self) -> Value {
        hashes_value(&self.names.iter().cloned().collect::<Vec<_>>())
    }

    /// The field `name` of `call`, refused where it names a head twice.
    ///
    /// # Errors
    ///
    /// Not a tuple of names, or one named twice.
    pub fn from_call(call: &Call, name: &str) -> Result<Heads<I>, ProdromeError> {
        let names = sorted_distinct(
            &format!("{}.{name}", call.name),
            "head",
            hashes(call, name)?,
        )?;
        Ok(Heads::new(names))
    }
}

impl<I> Clone for Heads<I> {
    fn clone(&self) -> Self {
        Heads::new(self.names.iter().cloned())
    }
}

impl<I> PartialEq for Heads<I> {
    fn eq(&self, other: &Self) -> bool {
        self.names == other.names
    }
}

impl<I> Eq for Heads<I> {}

impl<I> fmt::Debug for Heads<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Heads").field(&self.names).finish()
    }
}

/// Inclusion of the heads.
impl<I> Order for &Heads<I> {
    fn le(&self, other: &Self) -> bool {
        self.names.is_subset(&other.names)
    }
}

/// A SCHEMA SOME OF WHOSE REGISTERS HOLD HISTORIES: what a nest reads of
/// it ([`super::Level`]). Each held register is a register type whose values
/// are [`Heads`] of its inner schema; this names them, gives each a segment
/// of the paths its histories sit at, and reads the heads an event writes,
/// whatever the inner schema.
///
/// A held register's history sits at the path `key/segment` of the entity
/// that holds it, and its objects each at that path then their own key, so
/// a nest's keys are paths and an entity's key is a segment.
pub trait Nests: Schema<Key: AsRef<str>> {
    /// The registers whose values are histories.
    const HELD: &'static [Self::Register];

    /// A held register's name in a path.
    fn segment(register: Self::Register) -> &'static str;

    /// The heads this event writes to the held `register`, if it writes it.
    fn heads(&self, register: Self::Register) -> Option<&BTreeSet<Hash>>;
}

/// A held register's READING: the heads of its maximal writes, united, which
/// name the history it holds now. Empty where it is unwritten.
pub fn pointer<E: Nests>(stream: &[Stamp<E>], register: E::Register) -> BTreeSet<Hash> {
    read(stream, None, &Everything)
        .reading(register)
        .into_iter()
        .filter_map(|stamp| stamp.event.heads(register))
        .flatten()
        .cloned()
        .collect()
}
