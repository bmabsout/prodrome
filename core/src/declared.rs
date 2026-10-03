//! A SCHEMA AS DATA (`docs/design-register-types.md` §5, §6.0): a schema
//! that arrives at run time, as text, and is admitted only when its laws
//! hold.
//!
//! A Rust schema is a type ([`crate::schema::Schema`]); its vocabulary is a
//! unit, since the type says everything. A DECLARED schema is a value of
//! one type, [`Declaration`]: its event constructors and their typed
//! fields, its key, and its registers, each with an order from a closed
//! vocabulary (discrete, total over an integer, a finite state machine given
//! by its covering relation, sets under inclusion) and whether it is
//! inflationary. No valuation: a price is Rust's (design §4).
//!
//! [`Declared`], an event of a declared schema, implements `Schema` BY
//! INTERPRETATION, with the declaration as its vocabulary: the parse reads
//! its constructors, the route its writes, and a register is read under its
//! order by the completion every register is read by ([`crate::fold::maximal`]),
//! and climbs by [`crate::fold::grows`] where declared inflationary. So an
//! `EventStore<Declared>` opened at a declaration (`EventStore::at`) reads,
//! appends, syncs, nests and memoises by the code a Rust schema's store
//! runs, and by no other.
//!
//! THE TEXT FORM is the objects' own §2 literal grammar under the schema
//! language's closed vocabulary ([`Form`]), not JSON: one grammar, one
//! printer and one whitelist walker for objects and schemas alike, a
//! canonical print by construction, and so a CONTENT NAME, the hash of the
//! text, exactly as an object has one. A schema's sets (its events, its
//! registers, an enum's alternatives, a machine's covers, a register's
//! writes) print sorted, so the text is a function of the schema and never
//! of the order it was written in.
//!
//! ```
//! use prodrome::declared::{Declaration, Law};
//!
//! let text = "Schema(key='door', events=(Event(name='Moved', fields=(\
//!     Field(name='door', type=Text()), Field(name='at', type=Instant()), \
//!     Field(name='actor', type=Text()), \
//!     Field(name='to', type=Enum(alternatives=('open', 'shut'))))),), \
//!     registers=(Register(name='state', order=Discrete(), inflationary=False, \
//!     writes=(Write(event='Moved', field='to'),)),))";
//! let door = Declaration::admit(text).expect("admitted");
//! assert_eq!(door.key(), "door");
//!
//! // A machine whose covers go round is not a partial order.
//! let cycle = text
//!     .replace("Discrete()", "Machine(covers=(('open', 'shut'), ('shut', 'open')))");
//! assert_eq!(Declaration::admit(&cycle).unwrap_err().law, Law::PartialOrder);
//! ```

mod admit;
mod event;
mod language;
mod order;

use std::fmt;
use std::sync::Arc;

pub use admit::{Law, Refusal};
pub use event::{Datum, Declared, Key, Registers};
pub use language::{Event, Field, Form, Poset, Register, Type, Write};
pub use order::Valued;

use crate::event::Hash;
use crate::literal::{Signature, Vocabulary};
use admit::{Admitted, Reg, Shape};

/// A declared schema, ADMITTED: every law held (see [`Law`]). The only way
/// to one is [`Declaration::admit`], so a declaration that exists is lawful.
///
/// Cheap to clone (one shared value), and equal exactly when its name is:
/// the hash of its canonical text.
#[derive(Clone)]
pub struct Declaration(Arc<Admitted>);

/// A register of a declared schema: its place in the declaration, which
/// [`Declaration::register_name`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot(usize);

impl Slot {
    #[must_use]
    pub fn index(self) -> usize {
        self.0
    }
}

impl Declaration {
    /// `text`, admitted as a schema.
    ///
    /// # Errors
    ///
    /// The first law, in [`Law`]'s order, that the text does not keep.
    pub fn admit(text: &str) -> Result<Declaration, Refusal> {
        admit::admit(text).map(|admitted| Declaration(Arc::new(admitted)))
    }

    /// The canonical text it was admitted as.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.0.text
    }

    /// Its content name: the hash of its text.
    #[must_use]
    pub fn name(&self) -> &Hash {
        &self.0.name
    }

    /// The field every event names its entity by.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.0.key
    }

    /// Every register, in the order a reading lists them (by name).
    pub fn registers(&self) -> impl Iterator<Item = Slot> {
        (0..self.0.registers.len()).map(Slot)
    }

    /// The register named `name`.
    #[must_use]
    pub fn register(&self, name: &str) -> Option<Slot> {
        self.0
            .registers
            .iter()
            .position(|register| register.name == name)
            .map(Slot)
    }

    /// The name of the register `slot`.
    ///
    /// # Panics
    ///
    /// A slot of another declaration, past this one's registers.
    #[must_use]
    pub fn register_name(&self, slot: Slot) -> &str {
        &self.reg(slot).name
    }

    /// Is the register `slot` declared inflationary?
    ///
    /// # Panics
    ///
    /// As [`Declaration::register_name`].
    #[must_use]
    pub fn inflationary(&self, slot: Slot) -> bool {
        self.reg(slot).inflationary
    }

    /// `datum` under the order of the register `slot`: what that register's
    /// completion compares.
    ///
    /// # Panics
    ///
    /// As [`Declaration::register_name`].
    #[must_use]
    pub fn value<'d>(&'d self, slot: Slot, datum: &'d Datum) -> Valued<'d> {
        Valued {
            order: &self.reg(slot).order,
            datum,
        }
    }

    fn reg(&self, slot: Slot) -> &Reg {
        &self.0.registers[slot.0]
    }

    fn shape(&self, event: usize) -> &Shape {
        &self.0.events[event]
    }

    fn event(&self, name: &str) -> Option<usize> {
        self.0.events.iter().position(|shape| shape.name == name)
    }
}

impl PartialEq for Declaration {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.name == other.0.name
    }
}

impl Eq for Declaration {}

/// Its name, not its text: a declaration is its hash.
impl fmt::Debug for Declaration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Declaration")
            .field(&self.0.name.as_str())
            .finish()
    }
}

/// Its constructors, each with its declared field order: §2's whitelist
/// for its events.
impl Vocabulary for Declaration {
    fn signature(&self, name: &str) -> Option<Signature<'_>> {
        self.event(name)
            .map(|event| Signature::Owned(&self.shape(event).names))
    }
}
