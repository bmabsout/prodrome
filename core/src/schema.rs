//! A schema: what a store's events are, and what each one writes
//! (`docs/design-register-types.md` §5).
//!
//! THE EVENT TYPE IS THE SCHEMA. Its values are the events the schema names,
//! a closed sum of constructors with frozen fields, so an event outside the
//! vocabulary is not a value of the type: §2's parse refuses it at the
//! boundary. The impl says the rest: the entity each event is about, the
//! registers it writes, and which events a policy is asked about.
//!
//! A schema is the STORE'S type (`EventStore<S>`), as the payload was. No
//! object carries it: a stored print is parsed against the schema the store
//! was opened at, so `Genesis(label, nonce)` stays frozen.
//!
//! [`crate::event::TodoEvent`] is the reference schema ([`crate::todo`]):
//! §4's six kinds and the record kind, and every legacy envelope is read at it.

use std::fmt::Debug;

use crate::event::Actor;
use crate::literal::{Datetime, ProdromeError, Value, Vocabulary};

/// A store's events and what they mean.
///
/// # The contract
///
/// - [`Schema::Vocabulary`], [`Schema::to_value`] and [`Schema::from_value`]
///   are the STORED SHAPE: every constructor an event may print, with its
///   declared field order, the print in that order, and the parse that refuses
///   what a smart constructor would. None of the vocabulary's names may be an
///   envelope's (`Sealed`, `Woven`, `Genesis`, `Change`, `Snapshot`), which are
///   looked up first.
/// - [`Schema::key`], [`Schema::at`] and [`Schema::actor`] are what every
///   event says: about what, when its writer thought it was (data, never an
///   order), and who.
/// - [`Schema::writes`] is the ROUTE's names: the registers whose frontier
///   this event joins and whose writes it therefore supersedes (§6). A register
///   that only accumulates, such as the todo's tendings, is not among them.
/// - [`Schema::asks`] is the policy's question: an event it answers `false`
///   for binds whoever wrote it (§5).
pub trait Schema: Clone + PartialEq + Debug + Send + Sync + 'static {
    /// §2's whitelist for this schema's events.
    type Vocabulary: Vocabulary + Default;

    /// What an event is about: a todo, a review.
    type Key: Clone + Ord + Debug + Send + Sync;

    /// A register's name, among those a write supersedes in.
    type Register: Copy + Ord + Debug + Send + Sync + 'static;

    /// Every register a write supersedes in, in the order a reading lists
    /// them.
    const REGISTERS: &'static [Self::Register];

    /// The event as a literal: every field, in declared order.
    fn to_value(&self) -> Value;

    /// The parse: a literal into an event, through every smart constructor.
    ///
    /// # Errors
    ///
    /// A constructor the vocabulary does not name, or a field a smart
    /// constructor refuses.
    fn from_value(value: &Value) -> Result<Self, ProdromeError>;

    fn key(&self) -> &Self::Key;

    fn at(&self) -> Datetime;

    fn actor(&self) -> &Actor;

    /// The registers this event writes, each once.
    fn writes(&self) -> impl Iterator<Item = Self::Register>;

    /// Does the policy decide whether this event binds?
    fn asks(&self) -> bool {
        true
    }
}
