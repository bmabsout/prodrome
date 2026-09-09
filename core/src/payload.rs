//! §4's record kind, as a parameter: what a HOST attaches to a todo.
//!
//! An event of a lifecycle kind (`Created`, `Completed`, `Cancelled`,
//! `Reopened`, `SpecRevised`) is the database's own — its fields are the
//! database's semantics and are frozen here. The RECORD kind is not: it is a
//! todo's content, and content is the host's. `kind`, `body`, `category`,
//! `waiting_on`, a checklist, a mail message a todo came from — those are
//! DECORATIONS of a fulfillment curve, and a database that named them would be
//! one deployment's schema pretending to be a format.
//!
//! So the record is `KIND(todo, at, actor, <the host's fields>)`: the core owns
//! the three fields every event has, and a [`Payload`] owns the rest. The core
//! reads exactly two things out of one (§6): the spec it carries, and how many
//! checklist items it has. Everything else about a payload — what it means,
//! how it renders, which of its fields a person may edit — is the host's, and
//! the database never asks.
//!
//! THE STORED BYTES ARE STILL FROZEN (§1). A payload's `KIND`, its `FIELDS` and
//! their order are what the canonical printer emits and what the hash is taken
//! of, so they are as unchangeable as any shipped field: evolution is a new
//! constructor, never a new field on a shipped one.
//!
//! The reference payload — the one `conformance/*.json` was taken with — is
//! [`crate::reference`].

use std::fmt::Debug;

use crate::fpl::Term;
use crate::literal::{Call, Datetime, ProdromeError, Value};

/// What a host attaches to a todo: the fields of §4's record kind, beyond the
/// `todo`, `at` and `actor` every event carries.
///
/// A TYPE PARAMETER AND NEVER A `dyn`. The payload is inside every event, every
/// envelope, every register and every fold, and one store holds one kind of
/// record — a chain whose records are sometimes one shape and sometimes another
/// is not a thing this format can express, because the vocabulary a stored
/// object is parsed against is closed (§2) and is this payload's.
///
/// # The contract
///
/// - [`Payload::KIND`] and [`Payload::FIELDS`] are the STORED SHAPE. The
///   printer emits `KIND(todo=…, at=…, actor=…, <FIELDS in order>)`, and
///   [`Payload::fields`] must answer with exactly `FIELDS`, in that order, for
///   every value. A disagreement is a print that does not parse back.
/// - [`Payload::VOCABULARY`] is every OTHER constructor a payload's fields may
///   nest, with its declared field order. §2's whitelist is the core's names
///   (§4's lifecycle kinds, §7's terms) plus these, so it is still a whitelist.
///   None of these names may be one of the core's.
/// - [`Payload::from_fields`] is the PARSE BOUNDARY, the way `mk_*` is for the
///   core's kinds: it refuses what a smart constructor would refuse, and its
///   error is the parse error.
/// - [`Payload::spec`] and [`Payload::checklist_len`] are the ONLY readings §6
///   takes. `flatten` (§6.4) reads both; `specs_at` (§6.2) and the spec
///   register (§6.6) read the first; nothing else in the core looks inside a
///   payload at all.
///
/// `PartialEq` and not `Eq`: a spec is an FPL [`Term`], and a term holds
/// floats.
pub trait Payload: Clone + PartialEq + Debug + Send + Sync + 'static {
    /// The record constructor's name — the reference payload's is `"Authored"`.
    const KIND: &'static str;

    /// The payload's fields, in DECLARED ORDER, following the core's `todo`,
    /// `at` and `actor`. This is the stored field order and it is frozen.
    const FIELDS: &'static [&'static str];

    /// Every other constructor the payload's fields use, with its own declared
    /// field order — the reference payload's `Source`, `SubTodo` and `Note`.
    /// Empty for a payload of plain values.
    const VOCABULARY: &'static [(&'static str, &'static [&'static str])];

    /// The payload as literal values, in [`Payload::FIELDS`]'s order. What the
    /// canonical printer emits after the three fields the core owns.
    fn fields(&self) -> Vec<(&'static str, Value)>;

    /// The parse: a stored record's call, validated into a payload. `call`
    /// carries `todo`, `at` and `actor` too — the core has already read those —
    /// and the helpers in this module are the same ones the core's own kinds
    /// are parsed with.
    fn from_fields(call: &Call) -> Result<Self, ProdromeError>;

    /// The spec this record prices its todo by, if it carries one (§6.2, §6.4).
    fn spec(&self) -> Option<&Term>;

    /// How many checklist items the record holds (§6.4). Zero for a payload
    /// with no such notion, which prices by its spec alone.
    fn checklist_len(&self) -> usize;
}

/// The fields of §4's record kind that are the CORE's, in order, before any
/// payload's. Frozen: every event in this database says who, about what, and
/// when the writer thought it was.
pub const RECORD_PREFIX: &[&str] = &["todo", "at", "actor"];

/// The record kind's whole declared field order — [`RECORD_PREFIX`] then
/// `P::FIELDS`. What the §2 parser binds a stored record against.
pub fn record_signature<P: Payload>() -> Vec<&'static str> {
    let mut fields = RECORD_PREFIX.to_vec();
    fields.extend_from_slice(P::FIELDS);
    fields
}

// --- reading a stored call --------------------------------------------------
//
// The helpers every `from_fields` wants, public because a HOST writes one:
// implementing `Payload` outside this crate should not mean reimplementing the
// refusals the core's own kinds are parsed with.

/// A field that must be there.
pub fn required<'a>(call: &'a Call, name: &str) -> Result<&'a Value, ProdromeError> {
    call.field(name)
        .ok_or_else(|| ProdromeError::invalid(format!("{}(...) is missing {name}", call.name)))
}

pub fn as_string(value: &Value, context: &str) -> Result<String, ProdromeError> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ProdromeError::invalid(format!("{context} must be a string, got {value:?}")))
}

pub fn string_field(call: &Call, name: &str) -> Result<String, ProdromeError> {
    as_string(required(call, name)?, &format!("{}.{name}", call.name))
}

/// A shipped string field where `""` means absent: a missing field reads as
/// `""` too, which is how a hand-written literal with the trailing defaults
/// omitted parses the same as the canonical print of the same value.
pub fn string_or_empty(call: &Call, name: &str) -> Result<String, ProdromeError> {
    match call.field(name) {
        None => Ok(String::new()),
        Some(value) => as_string(value, &format!("{}.{name}", call.name)),
    }
}

pub fn datetime_field(call: &Call, name: &str) -> Result<Datetime, ProdromeError> {
    match required(call, name)? {
        Value::Datetime(at) => Ok(*at),
        other => Err(ProdromeError::invalid(format!(
            "{}.{name} must be a datetime, got {other:?}",
            call.name
        ))),
    }
}

pub fn bool_field(call: &Call, name: &str) -> Result<bool, ProdromeError> {
    match required(call, name)? {
        Value::Bool(flag) => Ok(*flag),
        other => Err(ProdromeError::invalid(format!(
            "{}.{name} must be a bool, got {other:?}",
            call.name
        ))),
    }
}

pub fn tuple_field<'a>(call: &'a Call, name: &str) -> Result<&'a [Value], ProdromeError> {
    required(call, name)?
        .as_tuple()
        .ok_or_else(|| ProdromeError::invalid(format!("{}.{name} must be a tuple", call.name)))
}

pub fn tuple_or_empty<'a>(call: &'a Call, name: &str) -> Result<&'a [Value], ProdromeError> {
    match call.field(name) {
        None => Ok(&[]),
        Some(value) => value
            .as_tuple()
            .ok_or_else(|| ProdromeError::invalid(format!("{}.{name} must be a tuple", call.name))),
    }
}

pub fn strings(items: &[Value], context: &str) -> Result<Vec<String>, ProdromeError> {
    items.iter().map(|item| as_string(item, context)).collect()
}

/// A field that is a §7 term or `None` — an optional spec, the one payload
/// field the core reads.
pub fn spec_field(call: &Call, name: &str) -> Result<Option<Term>, ProdromeError> {
    match required(call, name)? {
        Value::None => Ok(None),
        other => Ok(Some(Term::from_value(other)?)),
    }
}

/// A nested constructor call of a named kind — a payload's own records.
pub fn expect_call<'a>(value: &'a Value, name: &str) -> Result<&'a Call, ProdromeError> {
    match value.as_call() {
        Some(call) if call.name == name => Ok(call),
        other => Err(ProdromeError::invalid(format!(
            "expected a {name}(...), got {other:?}"
        ))),
    }
}
