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
//! A schema MAY price its entities ([`Valuation`]); one that does not has no
//! price at all, which is not FPL's `Absent`.

use std::fmt::Debug;

use crate::event::Actor;
use crate::fold::Product;
use crate::fpl::{Env, FplError};
use crate::literal::{Datetime, ProdromeError, Value, Vocabulary};
use crate::policy::Policy;
use crate::term::Term;

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
/// - [`Schema::Registers`] is an entity's REGISTERS, a [`Product`] of
///   semilattices: its `join` is the route from an event to the writes it
///   makes, each register a [`crate::fold::Frontier`] or an accumulating set,
///   read under its [`crate::fold::RegisterType`].
/// - [`Schema::writes`] is the route's NAMES: the registers whose frontier
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

    /// An entity's registers: a product of semilattices.
    type Registers<'a>: Product<'a, Schema = Self>;

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

/// A schema's PRICE (design §4): a map from an entity's reading to a §7 term,
/// and what a priced row shows (§6.7). Optional: [`crate::view::entries`]
/// asks for it, and nothing else does.
///
/// A reading in conflict holds more than one WORLD, one candidate from each
/// register. [`Valuation::worlds`] prices each, and the core prices the
/// reading as `Least` over them ([`crate::fold::price`]): the meet in the
/// fulfillment order, "price a conflict as its most urgent world". It is not
/// a register's join, and FPL's `Conj`, which aggregates different entities,
/// is neither.
///
/// An entity's key is the name a `Ref` gives it, so it reads as a string.
pub trait Valuation: Schema<Key: AsRef<str>> {
    /// What a priced row shows of an entity's registers.
    type Reading: Clone + PartialEq + std::fmt::Debug;

    fn reading(registers: &Self::Registers<'_>) -> Self::Reading;

    /// Does the CLAIMED reading, under every writer, dispute the confirmed
    /// one where a reader should be told?
    fn disputes(confirmed: &Self::Reading, claimed: &Self::Reading) -> bool;

    /// Does the reading show a write the policy binds and does not confirm?
    fn unconfirmed(registers: &Self::Registers<'_>, policy: &impl Policy<Self>) -> bool;

    /// What FPL's terms read of an entity (§6.1), recorded in `env`.
    fn bind(key: &Self::Key, registers: &Self::Registers<'_>, env: &mut Env);

    /// The price of each world `now` holds, a register unwritten yet read as
    /// it was first written (`first`), and `head` where a world prices
    /// nothing else. Empty where the entity has no price there.
    ///
    /// # Errors
    ///
    /// A term a smart constructor refuses.
    fn worlds(
        now: &Self::Registers<'_>,
        first: &Self::Registers<'_>,
        head: Option<&Term>,
    ) -> Result<Vec<Term>, FplError>;
}
