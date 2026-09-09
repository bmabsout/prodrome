//! §4 — the events, and §3's envelopes and hashing.
//!
//! The closed kinds of §4, as Rust types. The schema-evolution rule is
//! absolute: the field set of a SHIPPED kind is frozen forever, because a new
//! field changes what the canonical printer emits and would orphan every
//! stored object from its hash. Evolution is a NEW KIND.
//!
//! FIVE KINDS ARE THE DATABASE'S AND ONE IS THE HOST'S. `Created`, `Completed`,
//! `Cancelled`, `Reopened` and `SpecRevised` are lifecycle and price — the
//! semantics §5 and §6 are written about — and their fields are here. The
//! RECORD kind is a todo's CONTENT, and content is a deployment's: it is
//! `KIND(todo, at, actor, <the host's fields>)`, where a [`Payload`] supplies
//! the constructor name, the fields, their parse and their print. See
//! [`crate::payload`]; the payload the conformance vectors were taken with is
//! [`crate::reference`].
//!
//! Names that mean different things are different types even when they are all
//! strings (§1): [`Hash`], [`TodoId`], [`Actor`], [`Name`]. The `mk_*`
//! constructors are the only path from parameters to a value, and they are also
//! the PARSE boundary — a stored literal arrives as bare strings and comes out
//! of [`Envelope::from_value`] having been through them, which is why nothing
//! downstream re-checks.
//!
//! "" MEANS ABSENT on a shipped string field, on purpose and forever: absence
//! with two spellings is worse than a sentinel with one meaning. Where absence
//! is NOT a shipped string — `Sealed.prev` at genesis, `Woven.event` — it is an
//! `Option`, and the printer puts the `''`/`None` back.

use std::collections::BTreeSet;
use std::marker::PhantomData;

use sha2::{Digest, Sha256};

use crate::fpl::{Term, TERM_SIGNATURES};
use crate::literal::{
    parse_literal, print_literal, Datetime, ProdromeError, Signature, Table, Value, Vocabulary,
};
use crate::payload::{
    as_string, datetime_field, record_signature, required, string_field, string_or_empty,
    tuple_field, Payload,
};

// --- the names, as types ----------------------------------------------------

#[macro_export]
macro_rules! newtype_str {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

/// Lent to [`crate::reference`], which declares two string newtypes of its
/// own. `#[allow]` because a core built without that feature has no other
/// user for it.
#[allow(unused_imports)]
pub(crate) use newtype_str;

newtype_str! {
    /// An object's name: sha256 of its canonical print, 64 lowercase hex.
    Hash
}

newtype_str! {
    /// A todo's identifier: `^[a-z0-9_-]+$`.
    TodoId
}

newtype_str! {
    /// Who wrote an event. SYNTACTIC only (`^[a-z][a-z0-9_-]*$`): WHICH actors
    /// exist, and which are trusted, is instance data — the engine knows the
    /// mechanism, never the roster.
    Actor
}

newtype_str! {
    /// An identifier in a DOCUMENT's vocabulary — a payload's kind or category
    /// field: `^[A-Za-z][A-Za-z0-9_]*$`. Which values exist is the document's
    /// business, not the engine's.
    Name
}

fn matches_todo(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn matches_actor(text: &str) -> bool {
    let mut bytes = text.bytes();
    match bytes.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn matches_name(text: &str) -> bool {
    let mut bytes = text.bytes();
    match bytes.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

impl Hash {
    /// 64 lowercase hex, checked once so that every later reader — a parent
    /// name, a ref filename, a `prev` — is a name and not a hope.
    pub fn new(text: impl Into<String>) -> Result<Hash, ProdromeError> {
        let text = text.into();
        if text.len() == 64
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            Ok(Hash(text))
        } else {
            Err(ProdromeError::invalid(format!(
                "object name must be 64 lowercase hex, got {text:?}"
            )))
        }
    }

    fn of_bytes(bytes: &[u8]) -> Hash {
        Hash(
            Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        )
    }
}

impl TodoId {
    pub fn new(text: impl Into<String>) -> Result<TodoId, ProdromeError> {
        let text = text.into();
        if matches_todo(&text) {
            Ok(TodoId(text))
        } else {
            Err(ProdromeError::invalid(format!(
                "todo id must match ^[a-z0-9_-]+$ (nonempty), got {text:?}"
            )))
        }
    }
}

impl Actor {
    pub fn new(text: impl Into<String>) -> Result<Actor, ProdromeError> {
        let text = text.into();
        if matches_actor(&text) {
            Ok(Actor(text))
        } else {
            Err(ProdromeError::invalid(format!(
                "actor must match ^[a-z][a-z0-9_-]*$, got {text:?}"
            )))
        }
    }
}

impl Name {
    /// A required identifier.
    pub fn new(field: &str, text: impl Into<String>) -> Result<Name, ProdromeError> {
        let text = text.into();
        if matches_name(&text) {
            Ok(Name(text))
        } else {
            Err(ProdromeError::invalid(format!(
                "{field} must be an identifier, got {text:?}"
            )))
        }
    }

    /// An identifier that may be absent, where `""` is "not recorded" and
    /// nothing else.
    pub fn optional(field: &str, text: impl Into<String>) -> Result<Option<Name>, ProdromeError> {
        let text = text.into();
        if text.is_empty() {
            Ok(None)
        } else {
            Name::new(field, text).map(Some)
        }
    }
}

// --- the spec a repricing carries -------------------------------------------
//
// A `SpecRevised` and a record carry a §7 `Term`, and `fpl::Term` IS that type:
// one parser, one printer, one evaluator. The field is `fpl::Term` and not a
// wrapper, because a wrapper would be a second name for the same value and a
// place for a second reading to grow. §7's per-field bounds arrive with it
// (`Flat`'s 0..1, `Conj`'s `p`, `Decay`'s lead-up), so a stored spec that
// parses is a spec that evaluates.

// --- the records ------------------------------------------------------------

/// `Created(todo, at, actor, text, note)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    pub todo: TodoId,
    pub at: Datetime,
    pub actor: Actor,
    pub text: String,
    pub note: String,
}

/// The three lifecycle kinds that share a shape: `Completed`, `Cancelled`,
/// `Reopened`, each `(todo, at, actor, note)`. They are DIFFERENT kinds — the
/// enum below keeps them apart — and one record type is what stops the three
/// from drifting apart field by field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lifecycle {
    pub todo: TodoId,
    pub at: Datetime,
    pub actor: Actor,
    pub note: String,
}

/// `SpecRevised(todo, at, actor, spec, note)` — a new authored price, effective
/// at `at`. No env effect: the query layer picks the latest spec.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecRevised {
    pub todo: TodoId,
    pub at: Datetime,
    pub actor: Actor,
    pub spec: Term,
    pub note: String,
}

/// A todo's CONTENT, in full, as of `at`. Snapshot semantics, like a git blob:
/// an edit is a new record for the same id and the fold keeps the latest.
///
/// THE THREE FIELDS ARE THE CORE'S AND THE PAYLOAD IS THE HOST'S. Every event
/// in this database says who, about what, and when its writer thought it was;
/// what a record additionally SAYS is [`Payload`], and the core reads exactly
/// two things out of it (`spec`, `checklist_len`).
#[derive(Debug, Clone, PartialEq)]
pub struct Authored<P> {
    pub todo: TodoId,
    pub at: Datetime,
    pub actor: Actor,
    pub payload: P,
}

/// The closed kinds of §4.
#[derive(Debug, Clone, PartialEq)]
pub enum TodoEvent<P> {
    Created(Created),
    Completed(Lifecycle),
    Cancelled(Lifecycle),
    Reopened(Lifecycle),
    SpecRevised(SpecRevised),
    /// Boxed: a content record carries a todo's whole content and is an order
    /// of magnitude wider than a lifecycle event, and a log is mostly
    /// lifecycle.
    Authored(Box<Authored<P>>),
}

impl<P: Payload> TodoEvent<P> {
    pub fn todo(&self) -> &TodoId {
        match self {
            TodoEvent::Created(e) => &e.todo,
            TodoEvent::Completed(e) | TodoEvent::Cancelled(e) | TodoEvent::Reopened(e) => &e.todo,
            TodoEvent::SpecRevised(e) => &e.todo,
            TodoEvent::Authored(e) => &e.todo,
        }
    }

    /// The instant the writer stamped. DATA, never an order (§1): no clock
    /// decides who wins.
    pub fn at(&self) -> Datetime {
        match self {
            TodoEvent::Created(e) => e.at,
            TodoEvent::Completed(e) | TodoEvent::Cancelled(e) | TodoEvent::Reopened(e) => e.at,
            TodoEvent::SpecRevised(e) => e.at,
            TodoEvent::Authored(e) => e.at,
        }
    }

    pub fn actor(&self) -> &Actor {
        match self {
            TodoEvent::Created(e) => &e.actor,
            TodoEvent::Completed(e) | TodoEvent::Cancelled(e) | TodoEvent::Reopened(e) => &e.actor,
            TodoEvent::SpecRevised(e) => &e.actor,
            TodoEvent::Authored(e) => &e.actor,
        }
    }

    /// The constructor name this kind prints as — the payload's own for a
    /// record.
    pub fn kind_name(&self) -> &'static str {
        match self {
            TodoEvent::Created(_) => "Created",
            TodoEvent::Completed(_) => "Completed",
            TodoEvent::Cancelled(_) => "Cancelled",
            TodoEvent::Reopened(_) => "Reopened",
            TodoEvent::SpecRevised(_) => "SpecRevised",
            TodoEvent::Authored(_) => P::KIND,
        }
    }
}

/// The stored object: one envelope or the other (§3). ONE parent (or none) is a
/// `Sealed`, two or more a `Woven`, and there is no second spelling of either —
/// which is what lets [`parents_of`] be total and lets `verify` call a
/// one-parent `Woven` malformed rather than ambiguous.
#[derive(Debug, Clone, PartialEq)]
pub enum Envelope<P> {
    /// The chain envelope. `prev` is `None` at genesis — `Sealed.prev == ""` is
    /// an absence, not a name, and the type says so.
    Sealed {
        prev: Option<Hash>,
        event: TodoEvent<P>,
    },
    /// The MERGE envelope: two or more parents, sorted and distinct, and an
    /// OPTIONAL event, because a merge is structure and not a fact about a
    /// todo.
    Woven {
        parents: Vec<Hash>,
        event: Option<TodoEvent<P>>,
    },
}

impl<P> Envelope<P> {
    pub fn event(&self) -> Option<&TodoEvent<P>> {
        match self {
            Envelope::Sealed { event, .. } => Some(event),
            Envelope::Woven { event, .. } => event.as_ref(),
        }
    }
}

/// The objects this one rests on. Genesis has none, and returning an empty
/// slice for it is what lets every reader treat "no parents" as one condition
/// instead of two.
pub fn parents_of<P>(envelope: &Envelope<P>) -> Vec<Hash> {
    match envelope {
        Envelope::Sealed { prev: None, .. } => Vec::new(),
        Envelope::Sealed {
            prev: Some(prev), ..
        } => vec![prev.clone()],
        Envelope::Woven { parents, .. } => parents.clone(),
    }
}

// --- smart constructors ------------------------------------------------------

pub fn mk_created<P: Payload>(
    todo: &str,
    at: Datetime,
    actor: &str,
    text: &str,
    note: &str,
) -> Result<TodoEvent<P>, ProdromeError> {
    Ok(TodoEvent::Created(Created {
        todo: TodoId::new(todo)?,
        at,
        actor: Actor::new(actor)?,
        text: text.to_owned(),
        note: note.to_owned(),
    }))
}

fn mk_lifecycle(
    todo: &str,
    at: Datetime,
    actor: &str,
    note: &str,
) -> Result<Lifecycle, ProdromeError> {
    Ok(Lifecycle {
        todo: TodoId::new(todo)?,
        at,
        actor: Actor::new(actor)?,
        note: note.to_owned(),
    })
}

pub fn mk_completed<P: Payload>(
    todo: &str,
    at: Datetime,
    actor: &str,
    note: &str,
) -> Result<TodoEvent<P>, ProdromeError> {
    Ok(TodoEvent::Completed(mk_lifecycle(todo, at, actor, note)?))
}

pub fn mk_cancelled<P: Payload>(
    todo: &str,
    at: Datetime,
    actor: &str,
    note: &str,
) -> Result<TodoEvent<P>, ProdromeError> {
    Ok(TodoEvent::Cancelled(mk_lifecycle(todo, at, actor, note)?))
}

pub fn mk_reopened<P: Payload>(
    todo: &str,
    at: Datetime,
    actor: &str,
    note: &str,
) -> Result<TodoEvent<P>, ProdromeError> {
    Ok(TodoEvent::Reopened(mk_lifecycle(todo, at, actor, note)?))
}

pub fn mk_spec_revised<P: Payload>(
    todo: &str,
    at: Datetime,
    actor: &str,
    spec: Term,
    note: &str,
) -> Result<TodoEvent<P>, ProdromeError> {
    Ok(TodoEvent::SpecRevised(SpecRevised {
        todo: TodoId::new(todo)?,
        at,
        actor: Actor::new(actor)?,
        spec,
        note: note.to_owned(),
    }))
}

/// A CONTENT RECORD: the core's three fields, and the host's payload.
///
/// The payload arrived through its own smart constructor, so this checks what
/// the core owns and nothing else — which is the whole of the seam.
pub fn mk_record<P: Payload>(
    todo: &str,
    at: Datetime,
    actor: &str,
    payload: P,
) -> Result<TodoEvent<P>, ProdromeError> {
    Ok(TodoEvent::Authored(Box::new(Authored {
        todo: TodoId::new(todo)?,
        at,
        actor: Actor::new(actor)?,
        payload,
    })))
}

pub fn mk_sealed<P: Payload>(prev: Option<Hash>, event: TodoEvent<P>) -> Envelope<P> {
    Envelope::Sealed { prev, event }
}

/// The parse boundary for a merge. Two parents at least (one is a `Sealed`,
/// none is genesis), each named once — a duplicate would be a second edge to
/// the same history, which says nothing and would let two different objects
/// mean one merge. SORTED, so the merge of a parent set is one object with one
/// hash wherever it is made.
pub fn mk_woven<P: Payload>(
    parents: Vec<Hash>,
    event: Option<TodoEvent<P>>,
) -> Result<Envelope<P>, ProdromeError> {
    if parents.len() < 2 {
        return Err(ProdromeError::invalid(format!(
            "Woven.parents must name at least two parents (one parent is a Sealed), got {}",
            parents.len()
        )));
    }
    let mut sorted = parents;
    sorted.sort();
    let distinct: BTreeSet<&Hash> = sorted.iter().collect();
    if distinct.len() != sorted.len() {
        return Err(ProdromeError::invalid(format!(
            "Woven.parents names a parent twice: {:?}",
            sorted.iter().map(Hash::as_str).collect::<Vec<_>>()
        )));
    }
    Ok(Envelope::Woven {
        parents: sorted,
        event,
    })
}

// --- the vocabulary ----------------------------------------------------------

/// §4's constructors that are the DATABASE's, name and declared field order —
/// the envelope kinds and the five kinds whose fields are its own semantics.
/// The record kind is the payload's and is not here.
pub const EVENT_SIGNATURES: &[(&str, &[&str])] = &[
    ("Sealed", &["prev", "event"]),
    ("Woven", &["parents", "event"]),
    ("Created", &["todo", "at", "actor", "text", "note"]),
    ("Completed", &["todo", "at", "actor", "note"]),
    ("Cancelled", &["todo", "at", "actor", "note"]),
    ("Reopened", &["todo", "at", "actor", "note"]),
    ("SpecRevised", &["todo", "at", "actor", "spec", "note"]),
];

/// The whole vocabulary a stored artifact may use: §7's terms (a spec can nest
/// anywhere in a `SpecRevised` or a record) plus §4's own kinds plus the
/// PAYLOAD's — its record kind, whose fields are `todo, at, actor` and then
/// `P::FIELDS`, and the constructors those fields nest. `datetime` and
/// `timedelta` are the grammar's own and need no entry.
///
/// STILL A WHITELIST, and the core's half of it wins: a payload that named its
/// record `Created` would not shadow §4's, it would be unreachable.
pub struct EventVocabulary<P: Payload> {
    /// `todo, at, actor` then `P::FIELDS` — owned, because it is the one
    /// signature in the grammar that is not a compile-time table.
    record: Vec<&'static str>,
    payload: PhantomData<P>,
}

impl<P: Payload> EventVocabulary<P> {
    pub fn new() -> EventVocabulary<P> {
        EventVocabulary {
            record: record_signature::<P>(),
            payload: PhantomData,
        }
    }
}

impl<P: Payload> Default for EventVocabulary<P> {
    fn default() -> EventVocabulary<P> {
        EventVocabulary::new()
    }
}

impl<P: Payload> Vocabulary for EventVocabulary<P> {
    fn signature(&self, name: &str) -> Option<Signature<'_>> {
        // §7's half is `fpl`'s, imported rather than restated: the layer that
        // knows what a `Conj` MEANS is the one that declares its fields.
        if let Some(signature) = Table(TERM_SIGNATURES).find(name) {
            return Some(signature);
        }
        if let Some(signature) = Table(EVENT_SIGNATURES).find(name) {
            return Some(signature);
        }
        if name == P::KIND {
            return Some(Signature::Fields(&self.record));
        }
        Table(P::VOCABULARY).find(name)
    }
}

// --- the literal round trip --------------------------------------------------

fn text(value: impl Into<String>) -> Value {
    Value::Str(value.into())
}

fn tuple_of<T>(items: &[T], each: impl Fn(&T) -> Value) -> Value {
    Value::Tuple(items.iter().map(each).collect())
}

fn field(name: &str, value: Value) -> (String, Value) {
    (name.to_owned(), value)
}

impl<P: Payload> TodoEvent<P> {
    /// The event as a literal — EVERY field, in declared order, keyword form,
    /// which is what makes the print total and order-stable however the value
    /// was built.
    pub fn to_value(&self) -> Value {
        match self {
            TodoEvent::Created(e) => Value::call(
                "Created",
                vec![
                    field("todo", text(e.todo.0.clone())),
                    field("at", Value::Datetime(e.at)),
                    field("actor", text(e.actor.0.clone())),
                    field("text", text(e.text.clone())),
                    field("note", text(e.note.clone())),
                ],
            ),
            TodoEvent::Completed(e) | TodoEvent::Cancelled(e) | TodoEvent::Reopened(e) => {
                Value::call(
                    self.kind_name(),
                    vec![
                        field("todo", text(e.todo.0.clone())),
                        field("at", Value::Datetime(e.at)),
                        field("actor", text(e.actor.0.clone())),
                        field("note", text(e.note.clone())),
                    ],
                )
            }
            TodoEvent::SpecRevised(e) => Value::call(
                "SpecRevised",
                vec![
                    field("todo", text(e.todo.0.clone())),
                    field("at", Value::Datetime(e.at)),
                    field("actor", text(e.actor.0.clone())),
                    field("spec", e.spec.to_value()),
                    field("note", text(e.note.clone())),
                ],
            ),
            TodoEvent::Authored(e) => e.to_value(),
        }
    }
}

impl<P: Payload> Authored<P> {
    /// A content record as a literal: the core's three fields, then the
    /// payload's own, in `P::FIELDS`'s order.
    ///
    /// Split out of [`TodoEvent::to_value`] because the CONTENT fold (§6.3)
    /// hands back records and their print is what a consumer compares — one
    /// printer, reached from either shape.
    pub fn to_value(&self) -> Value {
        let payload = self.payload.fields();
        let mut fields = Vec::with_capacity(3 + payload.len());
        fields.push(field("todo", text(self.todo.0.clone())));
        fields.push(field("at", Value::Datetime(self.at)));
        fields.push(field("actor", text(self.actor.0.clone())));
        fields.extend(payload.into_iter().map(|(name, value)| field(name, value)));
        debug_assert!(
            fields
                .iter()
                .skip(3)
                .map(|(name, _)| name.as_str())
                .eq(P::FIELDS.iter().copied()),
            "{}::fields() must answer with FIELDS, in order",
            P::KIND
        );
        Value::call(P::KIND, fields)
    }
}

impl<P: Payload> Envelope<P> {
    pub fn to_value(&self) -> Value {
        match self {
            Envelope::Sealed { prev, event } => Value::call(
                "Sealed",
                vec![
                    field(
                        "prev",
                        text(prev.as_ref().map(|hash| hash.0.clone()).unwrap_or_default()),
                    ),
                    field("event", event.to_value()),
                ],
            ),
            Envelope::Woven { parents, event } => Value::call(
                "Woven",
                vec![
                    field("parents", tuple_of(parents, |hash| text(hash.0.clone()))),
                    field(
                        "event",
                        event.as_ref().map_or(Value::None, TodoEvent::to_value),
                    ),
                ],
            ),
        }
    }

    /// The parse boundary for a stored object: a literal in, a validated
    /// envelope out, every `mk_*` rule applied on the way.
    pub fn from_value(value: &Value) -> Result<Envelope<P>, ProdromeError> {
        let call = value
            .as_call()
            .ok_or_else(|| ProdromeError::invalid("an object must be a Sealed or a Woven"))?;
        match call.name.as_str() {
            "Sealed" => {
                let prev = string_field(call, "prev")?;
                let prev = if prev.is_empty() {
                    None
                } else {
                    Some(Hash::new(prev)?)
                };
                Ok(mk_sealed(prev, event_from_value(required(call, "event")?)?))
            }
            "Woven" => {
                let parents = tuple_field(call, "parents")?
                    .iter()
                    .map(|item| Hash::new(as_string(item, "Woven.parents")?))
                    .collect::<Result<Vec<_>, _>>()?;
                let event = match call.field("event") {
                    None | Some(Value::None) => None,
                    Some(other) => Some(event_from_value(other)?),
                };
                mk_woven(parents, event)
            }
            other => Err(ProdromeError::invalid(format!(
                "object is not a chain envelope: {other}"
            ))),
        }
    }
}

fn event_from_value<P: Payload>(value: &Value) -> Result<TodoEvent<P>, ProdromeError> {
    let call = value.as_call().ok_or_else(|| {
        ProdromeError::invalid(format!(
            "an event must be a constructor call, got {value:?}"
        ))
    })?;
    let todo = || string_field(call, "todo");
    let at = || datetime_field(call, "at");
    let actor = || string_field(call, "actor");
    match call.name.as_str() {
        "Created" => mk_created(
            &todo()?,
            at()?,
            &actor()?,
            &string_or_empty(call, "text")?,
            &string_or_empty(call, "note")?,
        ),
        "Completed" => mk_completed(&todo()?, at()?, &actor()?, &string_or_empty(call, "note")?),
        "Cancelled" => mk_cancelled(&todo()?, at()?, &actor()?, &string_or_empty(call, "note")?),
        "Reopened" => mk_reopened(&todo()?, at()?, &actor()?, &string_or_empty(call, "note")?),
        "SpecRevised" => mk_spec_revised(
            &todo()?,
            at()?,
            &actor()?,
            Term::from_value(required(call, "spec")?)?,
            &string_or_empty(call, "note")?,
        ),
        // THE PAYLOAD'S KIND IS TRIED LAST, so a payload cannot name its record
        // after one of §4's own and take its place.
        name if name == P::KIND => mk_record(&todo()?, at()?, &actor()?, P::from_fields(call)?),
        other => Err(ProdromeError::invalid(format!(
            "{other} is not one of SPEC §4's kinds"
        ))),
    }
}

// --- hashing, printing, trust ------------------------------------------------

/// `print_literal` of an event — what every fold that needs a deterministic
/// tiebreak sorts by.
pub fn canonical<P: Payload>(event: &TodoEvent<P>) -> String {
    print_literal(&event.to_value())
}

/// The canonical print of an envelope. This is the object's BYTES: the name is
/// the sha256 of exactly this, and the file holds exactly this.
pub fn canonical_envelope<P: Payload>(envelope: &Envelope<P>) -> String {
    print_literal(&envelope.to_value())
}

/// The hash of the canonical print — used at APPEND time to name the object, at
/// which moment it is by construction also the hash of the stored bytes.
/// Verification hashes the STORED BYTES instead (`store::EventStore::load`),
/// never a reprint: git hashes bytes and not semantics, so that a printer
/// change cannot false-alarm the whole store as tampered.
pub fn seal_hash<P: Payload>(envelope: &Envelope<P>) -> Hash {
    Hash::of_bytes(canonical_envelope(envelope).as_bytes())
}

/// Read one stored object's text into an envelope, through the closed
/// vocabulary — the core's kinds and the payload's — and every `mk_*` rule.
pub fn parse_envelope<P: Payload>(text: &str) -> Result<Envelope<P>, ProdromeError> {
    let vocabulary = EventVocabulary::<P>::new();
    Envelope::from_value(&parse_literal(text, &vocabulary)?)
}

/// Read ONE event's canonical print back — [`canonical`]'s inverse, through the
/// same closed vocabulary. An envelope is the stored object and `parse_envelope`
/// is how a store reads one; this is for the callers that hold a bare event
/// print instead (a fold's input, a binding's argument), and it exists here
/// rather than at those call sites because the vocabulary and the smart
/// constructors are this module's, not theirs.
pub fn parse_event<P: Payload>(text: &str) -> Result<TodoEvent<P>, ProdromeError> {
    let vocabulary = EventVocabulary::<P>::new();
    event_from_value(&parse_literal(text, &vocabulary)?)
}

/// THE trust rule, stated once (§5): may this event change what the system
/// believes? An untrusted actor's lifecycle and repricing events are
/// PROVISIONAL — stored, shown as claims, never folded — because that actor
/// reads attacker-controlled input. Its CONTENT records DO bind: writing
/// content is what such a writer is for, and a fold that hid its writes would
/// not be containment, it would be an outage that reports success.
pub fn binds<P: Payload>(event: &TodoEvent<P>, untrusted: &BTreeSet<Actor>) -> bool {
    matches!(event, TodoEvent::Authored(_)) || !untrusted.contains(event.actor())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::{mk_authored, Todo};

    type Event = TodoEvent<Todo>;

    fn at() -> Datetime {
        Datetime::new(2026, 9, 6, 7, 3, 0, 0).expect("a real instant")
    }

    #[test]
    fn genesis_has_no_parents_and_prints_an_empty_prev() {
        let event: Event = mk_created("alpha", at(), "bassel", "", "").expect("valid");
        let genesis = mk_sealed(None, event);
        assert!(parents_of(&genesis).is_empty());
        assert_eq!(
            canonical_envelope(&genesis),
            "Sealed(prev='', event=Created(todo='alpha', at=datetime(2026, 9, 6, 7, 3, 0), actor='bassel', text='', note=''))"
        );
    }

    #[test]
    fn the_constructors_are_the_only_door() {
        assert!(mk_created::<Todo>("Alpha", at(), "bassel", "", "").is_err());
        assert!(mk_created::<Todo>("alpha", at(), "Bassel", "", "").is_err());
        assert!(mk_created::<Todo>("", at(), "bassel", "", "").is_err());
        assert!(Hash::new("nothex").is_err());
    }

    #[test]
    fn a_woven_sorts_its_parents_and_refuses_fewer_than_two() {
        let a = Hash::new("a".repeat(64)).expect("hex");
        let b = Hash::new("b".repeat(64)).expect("hex");
        assert!(mk_woven::<Todo>(vec![a.clone()], None).is_err());
        assert!(mk_woven::<Todo>(vec![a.clone(), a.clone()], None).is_err());
        let woven =
            mk_woven::<Todo>(vec![b.clone(), a.clone()], None).expect("two distinct parents");
        assert_eq!(parents_of(&woven), vec![a, b]);
        assert!(woven.event().is_none());
    }

    #[test]
    fn a_spec_must_be_one_of_the_terms() {
        let vocabulary = EventVocabulary::<Todo>::new();
        let flat = parse_literal("Flat(value=0.5)", &vocabulary).expect("parses");
        assert!(Term::from_value(&flat).is_ok());
        let not_a_term =
            parse_literal("Note(on='block', lines=('x',))", &vocabulary).expect("parses");
        assert!(Term::from_value(&not_a_term).is_err());
        assert!(Term::from_value(&Value::None).is_err());
        // And §7's BOUNDS travel with the kind: a term that parses is a term
        // that evaluates.
        let out_of_range = parse_literal("Flat(value=1.5)", &vocabulary).expect("parses");
        assert!(Term::from_value(&out_of_range).is_err());
    }

    #[test]
    fn trust_is_a_set_of_names_and_a_record_always_binds() {
        let untrusted: BTreeSet<Actor> =
            [Actor::new("triage").expect("valid")].into_iter().collect();
        let completed: Event = mk_completed("alpha", at(), "triage", "").expect("valid");
        assert!(!binds(&completed, &untrusted));
        let authored = mk_authored(
            "alpha",
            at(),
            "triage",
            "todo",
            at(),
            "body",
            None,
            vec![],
            "",
            "",
            "",
            None,
            vec![],
            vec![],
            "",
        )
        .expect("valid");
        assert!(binds(&authored, &untrusted));
        assert!(binds(
            &mk_completed::<Todo>("alpha", at(), "bassel", "").expect("valid"),
            &untrusted
        ));
    }
}
