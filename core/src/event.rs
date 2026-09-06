//! §4 — the events, and §3's envelopes and hashing.
//!
//! The closed kinds of `suzatary/prodrome/events.py`, as Rust types. The
//! schema-evolution rule is the reference's, word for word: the field set of a
//! SHIPPED kind is frozen forever, because a new field changes what the
//! canonical printer emits and would orphan every stored object from its hash.
//! Evolution is a NEW KIND.
//!
//! Names that mean different things are different types even when they are all
//! strings (§1): [`Hash`], [`TodoId`], [`Actor`], [`Name`], [`MarkupSource`],
//! [`StringSource`], [`NoteSite`]. The `mk_*` constructors are the only path
//! from parameters to a value, and they are also the PARSE boundary — a stored
//! literal arrives as bare strings and comes out of [`Envelope::from_value`]
//! having been through them, which is why nothing downstream re-checks.
//!
//! "" MEANS ABSENT on a shipped string field, on purpose and forever: 239
//! objects already print `category=''`, and absence with two spellings is
//! worse than a sentinel with one meaning. Where absence is NOT a shipped
//! string — `Sealed.prev` at genesis, `Authored.spec`, `Authored.source`,
//! `Woven.event` — it is an `Option`, and the printer puts the `''`/`None`
//! back.

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use crate::fpl::{Term, TERM_SIGNATURES};
use crate::literal::{parse_literal, print_literal, Call, Datetime, ProdromeError, Table, Value};

// --- the names, as types ----------------------------------------------------

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
    /// An identifier in the DOCUMENT's vocabulary (`Authored.kind`,
    /// `Authored.category`): `^[A-Za-z][A-Za-z0-9_]*$`. Which values exist is
    /// the document's business, not the engine's.
    Name
}

newtype_str! {
    /// Typst MARKUP source — the inside of a `[..]` block. A different type
    /// from [`StringSource`] because `]` is structure in one and `"` in the
    /// other, so a value of one language in a field of the other is a bug the
    /// reparse would catch late.
    MarkupSource
}

newtype_str! {
    /// Typst STRING source — the inside of a `".."` literal.
    StringSource
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
    /// A required identifier — `Authored.kind`.
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

    /// An identifier that may be absent — `Authored.category`, where `""` is
    /// "not recorded" and nothing else.
    pub fn optional(field: &str, text: impl Into<String>) -> Result<Option<Name>, ProdromeError> {
        let text = text.into();
        if text.is_empty() {
            Ok(None)
        } else {
            Name::new(field, text).map(Some)
        }
    }
}

impl MarkupSource {
    pub fn new(text: impl Into<String>) -> MarkupSource {
        MarkupSource(text.into())
    }
}

impl StringSource {
    pub fn new(text: impl Into<String>) -> StringSource {
        StringSource(text.into())
    }
}

/// Where a [`Note`] may attach: a field of [`Authored`], or the block as a
/// whole. A closed set, so a note about `waiting_on` cannot be read as one
/// about `detail`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NoteSite {
    Block,
    Tail,
    Todo,
    Created,
    Category,
    WaitingOn,
    Source,
    Detail,
    Subtodos,
}

impl NoteSite {
    pub const ALL: [NoteSite; 9] = [
        NoteSite::Block,
        NoteSite::Tail,
        NoteSite::Todo,
        NoteSite::Created,
        NoteSite::Category,
        NoteSite::WaitingOn,
        NoteSite::Source,
        NoteSite::Detail,
        NoteSite::Subtodos,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            NoteSite::Block => "block",
            NoteSite::Tail => "tail",
            NoteSite::Todo => "todo",
            NoteSite::Created => "created",
            NoteSite::Category => "category",
            NoteSite::WaitingOn => "waiting_on",
            NoteSite::Source => "source",
            NoteSite::Detail => "detail",
            NoteSite::Subtodos => "subtodos",
        }
    }

    /// The parse boundary: a stored literal arrives as a bare string.
    pub fn parse(text: &str) -> Result<NoteSite, ProdromeError> {
        NoteSite::ALL
            .into_iter()
            .find(|site| site.as_str() == text)
            .ok_or_else(|| {
                let mut known: Vec<&str> = NoteSite::ALL.iter().map(|site| site.as_str()).collect();
                known.sort_unstable();
                ProdromeError::invalid(format!("Note.on must be one of {known:?}, got {text:?}"))
            })
    }
}

// --- the spec a repricing carries -------------------------------------------
//
// A `SpecRevised` and an `Authored` carry a §7 `Term`, and `fpl::Term` IS that
// type: one parser, one printer, one evaluator. The field is `fpl::Term` and
// not a wrapper, because a wrapper would be a second name for the same value
// and a place for a second reading to grow. §7's per-field bounds arrive with
// it (`Flat`'s 0..1, `Conj`'s `p`, `Decay`'s lead-up), so a stored spec that
// parses is a spec that evaluates — which the placeholder this replaced could
// not promise.

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

/// The message a todo came from. `date` is a day; the three identifiers are
/// `""` when the era that authored the todo did not record them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub sender: StringSource,
    pub subject: StringSource,
    pub date: Datetime,
    pub hash: StringSource,
    pub thread_id: StringSource,
    pub message_id: StringSource,
}

/// A comment run attached to one field of an [`Authored`] record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub on: NoteSite,
    pub lines: Vec<String>,
}

/// A checklist item inside a todo: no id, no history of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubTodo {
    pub body: String,
    pub done: bool,
}

/// A todo's CONTENT, in full, as of `at`. Snapshot semantics, like a git blob:
/// an edit is a new `Authored` for the same id and the fold keeps the latest.
#[derive(Debug, Clone, PartialEq)]
pub struct Authored {
    pub todo: TodoId,
    pub at: Datetime,
    pub actor: Actor,
    pub kind: Name,
    pub created: Datetime,
    pub body: MarkupSource,
    pub spec: Option<Term>,
    pub rationale: Vec<String>,
    pub category: Option<Name>,
    pub waiting_on: StringSource,
    pub detail: MarkupSource,
    pub source: Option<Source>,
    pub subtodos: Vec<SubTodo>,
    pub notes: Vec<Note>,
    pub note: String,
}

/// The closed kinds of §4.
#[derive(Debug, Clone, PartialEq)]
pub enum TodoEvent {
    Created(Created),
    Completed(Lifecycle),
    Cancelled(Lifecycle),
    Reopened(Lifecycle),
    SpecRevised(SpecRevised),
    /// Boxed: an `Authored` record carries a todo's whole content and is an
    /// order of magnitude wider than a lifecycle event, and a log is mostly
    /// lifecycle.
    Authored(Box<Authored>),
}

impl TodoEvent {
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

    /// The constructor name this kind prints as.
    pub fn kind_name(&self) -> &'static str {
        match self {
            TodoEvent::Created(_) => "Created",
            TodoEvent::Completed(_) => "Completed",
            TodoEvent::Cancelled(_) => "Cancelled",
            TodoEvent::Reopened(_) => "Reopened",
            TodoEvent::SpecRevised(_) => "SpecRevised",
            TodoEvent::Authored(_) => "Authored",
        }
    }
}

/// The stored object: one envelope or the other (§3). ONE parent (or none) is a
/// `Sealed`, two or more a `Woven`, and there is no second spelling of either —
/// which is what lets [`parents_of`] be total and lets `verify` call a
/// one-parent `Woven` malformed rather than ambiguous.
#[derive(Debug, Clone, PartialEq)]
pub enum Envelope {
    /// The chain envelope. `prev` is `None` at genesis — `Sealed.prev == ""` is
    /// an absence, not a name, and the type says so.
    Sealed {
        prev: Option<Hash>,
        event: TodoEvent,
    },
    /// The MERGE envelope: two or more parents, sorted and distinct, and an
    /// OPTIONAL event, because a merge is structure and not a fact about a
    /// todo.
    Woven {
        parents: Vec<Hash>,
        event: Option<TodoEvent>,
    },
}

impl Envelope {
    pub fn event(&self) -> Option<&TodoEvent> {
        match self {
            Envelope::Sealed { event, .. } => Some(event),
            Envelope::Woven { event, .. } => event.as_ref(),
        }
    }
}

/// The objects this one rests on. Genesis has none, and returning an empty
/// slice for it is what lets every reader treat "no parents" as one condition
/// instead of two.
pub fn parents_of(envelope: &Envelope) -> Vec<Hash> {
    match envelope {
        Envelope::Sealed { prev: None, .. } => Vec::new(),
        Envelope::Sealed {
            prev: Some(prev), ..
        } => vec![prev.clone()],
        Envelope::Woven { parents, .. } => parents.clone(),
    }
}

// --- smart constructors ------------------------------------------------------

pub fn mk_created(
    todo: &str,
    at: Datetime,
    actor: &str,
    text: &str,
    note: &str,
) -> Result<TodoEvent, ProdromeError> {
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

pub fn mk_completed(
    todo: &str,
    at: Datetime,
    actor: &str,
    note: &str,
) -> Result<TodoEvent, ProdromeError> {
    Ok(TodoEvent::Completed(mk_lifecycle(todo, at, actor, note)?))
}

pub fn mk_cancelled(
    todo: &str,
    at: Datetime,
    actor: &str,
    note: &str,
) -> Result<TodoEvent, ProdromeError> {
    Ok(TodoEvent::Cancelled(mk_lifecycle(todo, at, actor, note)?))
}

pub fn mk_reopened(
    todo: &str,
    at: Datetime,
    actor: &str,
    note: &str,
) -> Result<TodoEvent, ProdromeError> {
    Ok(TodoEvent::Reopened(mk_lifecycle(todo, at, actor, note)?))
}

pub fn mk_spec_revised(
    todo: &str,
    at: Datetime,
    actor: &str,
    spec: Term,
    note: &str,
) -> Result<TodoEvent, ProdromeError> {
    Ok(TodoEvent::SpecRevised(SpecRevised {
        todo: TodoId::new(todo)?,
        at,
        actor: Actor::new(actor)?,
        spec,
        note: note.to_owned(),
    }))
}

pub fn mk_source(
    sender: &str,
    subject: &str,
    date: Datetime,
    hash: &str,
    thread_id: &str,
    message_id: &str,
) -> Source {
    Source {
        sender: StringSource::new(sender),
        subject: StringSource::new(subject),
        date,
        hash: StringSource::new(hash),
        thread_id: StringSource::new(thread_id),
        message_id: StringSource::new(message_id),
    }
}

pub fn mk_note(on: &str, lines: Vec<String>) -> Result<Note, ProdromeError> {
    if lines.is_empty() {
        return Err(ProdromeError::invalid(
            "Note.lines must be nonempty — an empty note is not a note",
        ));
    }
    Ok(Note {
        on: NoteSite::parse(on)?,
        lines,
    })
}

pub fn mk_subtodo(body: &str, done: bool) -> Result<SubTodo, ProdromeError> {
    if body.trim().is_empty() {
        return Err(ProdromeError::invalid("SubTodo.body cannot be empty"));
    }
    Ok(SubTodo {
        body: body.to_owned(),
        done,
    })
}

/// Every field of §4's widest kind, in declared order. Long by construction:
/// the shipped field set is frozen, so a builder that let one be forgotten
/// would be a way to print a different object under the same name.
#[allow(clippy::too_many_arguments)]
pub fn mk_authored(
    todo: &str,
    at: Datetime,
    actor: &str,
    kind: &str,
    created: Datetime,
    body: &str,
    spec: Option<Term>,
    rationale: Vec<String>,
    category: &str,
    waiting_on: &str,
    detail: &str,
    source: Option<Source>,
    subtodos: Vec<SubTodo>,
    notes: Vec<Note>,
    note: &str,
) -> Result<TodoEvent, ProdromeError> {
    if body.trim().is_empty() {
        return Err(ProdromeError::invalid("Authored.body cannot be empty"));
    }
    Ok(TodoEvent::Authored(Box::new(Authored {
        todo: TodoId::new(todo)?,
        at,
        actor: Actor::new(actor)?,
        kind: Name::new("Authored.kind", kind)?,
        created,
        body: MarkupSource::new(body),
        spec,
        rationale,
        category: Name::optional("Authored.category", category)?,
        waiting_on: StringSource::new(waiting_on),
        detail: MarkupSource::new(detail),
        source,
        subtodos,
        notes,
        note: note.to_owned(),
    })))
}

pub fn mk_sealed(prev: Option<Hash>, event: TodoEvent) -> Envelope {
    Envelope::Sealed { prev, event }
}

/// The parse boundary for a merge. Two parents at least (one is a `Sealed`,
/// none is genesis), each named once — a duplicate would be a second edge to
/// the same history, which says nothing and would let two different objects
/// mean one merge. SORTED, so the merge of a parent set is one object with one
/// hash wherever it is made.
pub fn mk_woven(parents: Vec<Hash>, event: Option<TodoEvent>) -> Result<Envelope, ProdromeError> {
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

/// §4's constructors, name and declared field order — the envelope kinds, the
/// event kinds and the records they hold.
pub const EVENT_SIGNATURES: &[(&str, &[&str])] = &[
    ("Sealed", &["prev", "event"]),
    ("Woven", &["parents", "event"]),
    ("Created", &["todo", "at", "actor", "text", "note"]),
    ("Completed", &["todo", "at", "actor", "note"]),
    ("Cancelled", &["todo", "at", "actor", "note"]),
    ("Reopened", &["todo", "at", "actor", "note"]),
    ("SpecRevised", &["todo", "at", "actor", "spec", "note"]),
    (
        "Authored",
        &[
            "todo",
            "at",
            "actor",
            "kind",
            "created",
            "body",
            "spec",
            "rationale",
            "category",
            "waiting_on",
            "detail",
            "source",
            "subtodos",
            "notes",
            "note",
        ],
    ),
    (
        "Source",
        &[
            "sender",
            "subject",
            "date",
            "hash",
            "thread_id",
            "message_id",
        ],
    ),
    ("Note", &["on", "lines"]),
    ("SubTodo", &["body", "done"]),
];

/// The whole vocabulary a stored artifact may use: §7's terms (a spec can nest
/// anywhere in a `SpecRevised` or an `Authored`) plus §4's kinds. `datetime`
/// and `timedelta` are the grammar's own and need no entry.
pub struct EventVocabulary;

impl crate::literal::Vocabulary for EventVocabulary {
    fn signature(&self, name: &str) -> Option<crate::literal::Signature> {
        // §7's half is `fpl`'s, imported rather than restated: the layer that
        // knows what a `Conj` MEANS is the one that declares its fields.
        Table(TERM_SIGNATURES)
            .signature(name)
            .or_else(|| Table(EVENT_SIGNATURES).signature(name))
    }
}
/// The one whitelist a stored object is read against.
pub const EVENT_VOCABULARY: EventVocabulary = EventVocabulary;

// --- the literal round trip --------------------------------------------------

fn text(value: impl Into<String>) -> Value {
    Value::Str(value.into())
}

fn optional_text(value: Option<&Name>) -> Value {
    Value::Str(value.map(|name| name.0.clone()).unwrap_or_default())
}

fn tuple_of<T>(items: &[T], each: impl Fn(&T) -> Value) -> Value {
    Value::Tuple(items.iter().map(each).collect())
}

fn field(name: &str, value: Value) -> (String, Value) {
    (name.to_owned(), value)
}

impl Source {
    pub fn to_value(&self) -> Value {
        Value::call(
            "Source",
            vec![
                field("sender", text(self.sender.0.clone())),
                field("subject", text(self.subject.0.clone())),
                field("date", Value::Datetime(self.date)),
                field("hash", text(self.hash.0.clone())),
                field("thread_id", text(self.thread_id.0.clone())),
                field("message_id", text(self.message_id.0.clone())),
            ],
        )
    }
}

impl Note {
    pub fn to_value(&self) -> Value {
        Value::call(
            "Note",
            vec![
                field("on", text(self.on.as_str())),
                field("lines", tuple_of(&self.lines, |line| text(line.clone()))),
            ],
        )
    }
}

impl SubTodo {
    pub fn to_value(&self) -> Value {
        Value::call(
            "SubTodo",
            vec![
                field("body", text(self.body.clone())),
                field("done", Value::Bool(self.done)),
            ],
        )
    }
}

impl TodoEvent {
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
            TodoEvent::Authored(e) => Value::call(
                "Authored",
                vec![
                    field("todo", text(e.todo.0.clone())),
                    field("at", Value::Datetime(e.at)),
                    field("actor", text(e.actor.0.clone())),
                    field("kind", text(e.kind.0.clone())),
                    field("created", Value::Datetime(e.created)),
                    field("body", text(e.body.0.clone())),
                    field(
                        "spec",
                        e.spec.as_ref().map_or(Value::None, |spec| spec.to_value()),
                    ),
                    field(
                        "rationale",
                        tuple_of(&e.rationale, |line| text(line.clone())),
                    ),
                    field("category", optional_text(e.category.as_ref())),
                    field("waiting_on", text(e.waiting_on.0.clone())),
                    field("detail", text(e.detail.0.clone())),
                    field(
                        "source",
                        e.source.as_ref().map_or(Value::None, Source::to_value),
                    ),
                    field("subtodos", tuple_of(&e.subtodos, SubTodo::to_value)),
                    field("notes", tuple_of(&e.notes, Note::to_value)),
                    field("note", text(e.note.clone())),
                ],
            ),
        }
    }
}

impl Envelope {
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
    pub fn from_value(value: &Value) -> Result<Envelope, ProdromeError> {
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

fn required<'a>(call: &'a Call, name: &str) -> Result<&'a Value, ProdromeError> {
    call.field(name)
        .ok_or_else(|| ProdromeError::invalid(format!("{}(...) is missing {name}", call.name)))
}

fn as_string(value: &Value, context: &str) -> Result<String, ProdromeError> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ProdromeError::invalid(format!("{context} must be a string, got {value:?}")))
}

fn string_field(call: &Call, name: &str) -> Result<String, ProdromeError> {
    as_string(required(call, name)?, &format!("{}.{name}", call.name))
}

/// A shipped string field where `""` means absent: a missing field reads as
/// `""` too, which is how a hand-written literal with the trailing defaults
/// omitted parses the same as the canonical print of the same value.
fn string_or_empty(call: &Call, name: &str) -> Result<String, ProdromeError> {
    match call.field(name) {
        None => Ok(String::new()),
        Some(value) => as_string(value, &format!("{}.{name}", call.name)),
    }
}

fn datetime_field(call: &Call, name: &str) -> Result<Datetime, ProdromeError> {
    match required(call, name)? {
        Value::Datetime(at) => Ok(*at),
        other => Err(ProdromeError::invalid(format!(
            "{}.{name} must be a datetime, got {other:?}",
            call.name
        ))),
    }
}

fn tuple_field<'a>(call: &'a Call, name: &str) -> Result<&'a [Value], ProdromeError> {
    required(call, name)?
        .as_tuple()
        .ok_or_else(|| ProdromeError::invalid(format!("{}.{name} must be a tuple", call.name)))
}

fn tuple_or_empty<'a>(call: &'a Call, name: &str) -> Result<&'a [Value], ProdromeError> {
    match call.field(name) {
        None => Ok(&[]),
        Some(value) => value
            .as_tuple()
            .ok_or_else(|| ProdromeError::invalid(format!("{}.{name} must be a tuple", call.name))),
    }
}

fn strings(items: &[Value], context: &str) -> Result<Vec<String>, ProdromeError> {
    items.iter().map(|item| as_string(item, context)).collect()
}

fn spec_field(call: &Call, name: &str) -> Result<Option<Term>, ProdromeError> {
    match required(call, name)? {
        Value::None => Ok(None),
        other => Ok(Some(Term::from_value(other)?)),
    }
}

fn event_from_value(value: &Value) -> Result<TodoEvent, ProdromeError> {
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
        "Authored" => mk_authored(
            &todo()?,
            at()?,
            &actor()?,
            &string_field(call, "kind")?,
            datetime_field(call, "created")?,
            &string_field(call, "body")?,
            spec_field(call, "spec")?,
            strings(tuple_or_empty(call, "rationale")?, "Authored.rationale")?,
            &string_or_empty(call, "category")?,
            &string_or_empty(call, "waiting_on")?,
            &string_or_empty(call, "detail")?,
            match call.field("source") {
                None | Some(Value::None) => None,
                Some(other) => Some(source_from_value(other)?),
            },
            tuple_or_empty(call, "subtodos")?
                .iter()
                .map(subtodo_from_value)
                .collect::<Result<Vec<_>, _>>()?,
            tuple_or_empty(call, "notes")?
                .iter()
                .map(note_from_value)
                .collect::<Result<Vec<_>, _>>()?,
            &string_or_empty(call, "note")?,
        ),
        other => Err(ProdromeError::invalid(format!(
            "{other} is not one of SPEC §4's kinds"
        ))),
    }
}

fn source_from_value(value: &Value) -> Result<Source, ProdromeError> {
    let call = expect_call(value, "Source")?;
    Ok(mk_source(
        &string_field(call, "sender")?,
        &string_field(call, "subject")?,
        datetime_field(call, "date")?,
        &string_or_empty(call, "hash")?,
        &string_or_empty(call, "thread_id")?,
        &string_or_empty(call, "message_id")?,
    ))
}

fn note_from_value(value: &Value) -> Result<Note, ProdromeError> {
    let call = expect_call(value, "Note")?;
    mk_note(
        &string_field(call, "on")?,
        strings(tuple_field(call, "lines")?, "Note.lines")?,
    )
}

fn subtodo_from_value(value: &Value) -> Result<SubTodo, ProdromeError> {
    let call = expect_call(value, "SubTodo")?;
    let done = match required(call, "done")? {
        Value::Bool(done) => *done,
        other => {
            return Err(ProdromeError::invalid(format!(
                "SubTodo.done must be a bool, got {other:?}"
            )))
        }
    };
    mk_subtodo(&string_field(call, "body")?, done)
}

fn expect_call<'a>(value: &'a Value, name: &str) -> Result<&'a Call, ProdromeError> {
    match value.as_call() {
        Some(call) if call.name == name => Ok(call),
        other => Err(ProdromeError::invalid(format!(
            "expected a {name}(...), got {other:?}"
        ))),
    }
}

// --- hashing, printing, trust ------------------------------------------------

/// `print_literal` of an event — what every fold that needs a deterministic
/// tiebreak sorts by, and what the reference memoises for the same reason.
pub fn canonical(event: &TodoEvent) -> String {
    print_literal(&event.to_value())
}

/// The canonical print of an envelope. This is the object's BYTES: the name is
/// the sha256 of exactly this, and the file holds exactly this.
pub fn canonical_envelope(envelope: &Envelope) -> String {
    print_literal(&envelope.to_value())
}

/// The hash of the canonical print — used at APPEND time to name the object, at
/// which moment it is by construction also the hash of the stored bytes.
/// Verification hashes the STORED BYTES instead (`store::EventStore::load`),
/// never a reprint: git hashes bytes and not semantics, so that a printer
/// change cannot false-alarm the whole store as tampered.
pub fn seal_hash(envelope: &Envelope) -> Hash {
    Hash::of_bytes(canonical_envelope(envelope).as_bytes())
}

/// Read one stored object's text into an envelope, through the closed
/// vocabulary and every `mk_*` rule.
pub fn parse_envelope(text: &str) -> Result<Envelope, ProdromeError> {
    Envelope::from_value(&parse_literal(text, &EVENT_VOCABULARY)?)
}

/// THE trust rule, stated once (§5): may this event change what the system
/// believes? An untrusted actor's lifecycle and repricing events are
/// PROVISIONAL — stored, shown as claims, never folded — because that actor
/// reads attacker-controlled mail. Its `Authored` records DO bind: writing
/// content is what the agent is for, and a fold that hid its writes would not
/// be containment, it would be an outage that reports success.
pub fn binds(event: &TodoEvent, untrusted: &BTreeSet<Actor>) -> bool {
    matches!(event, TodoEvent::Authored(_)) || !untrusted.contains(event.actor())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at() -> Datetime {
        Datetime::new(2026, 9, 6, 7, 3, 0, 0).expect("a real instant")
    }

    #[test]
    fn genesis_has_no_parents_and_prints_an_empty_prev() {
        let event = mk_created("alpha", at(), "bassel", "", "").expect("valid");
        let genesis = mk_sealed(None, event);
        assert!(parents_of(&genesis).is_empty());
        assert_eq!(
            canonical_envelope(&genesis),
            "Sealed(prev='', event=Created(todo='alpha', at=datetime(2026, 9, 6, 7, 3, 0), actor='bassel', text='', note=''))"
        );
    }

    #[test]
    fn the_constructors_are_the_only_door() {
        assert!(mk_created("Alpha", at(), "bassel", "", "").is_err());
        assert!(mk_created("alpha", at(), "Bassel", "", "").is_err());
        assert!(mk_created("", at(), "bassel", "", "").is_err());
        assert!(mk_note("nowhere", vec!["x".into()]).is_err());
        assert!(mk_note("block", vec![]).is_err());
        assert!(mk_subtodo("  ", false).is_err());
        assert!(Hash::new("nothex").is_err());
    }

    #[test]
    fn a_woven_sorts_its_parents_and_refuses_fewer_than_two() {
        let a = Hash::new("a".repeat(64)).expect("hex");
        let b = Hash::new("b".repeat(64)).expect("hex");
        assert!(mk_woven(vec![a.clone()], None).is_err());
        assert!(mk_woven(vec![a.clone(), a.clone()], None).is_err());
        let woven = mk_woven(vec![b.clone(), a.clone()], None).expect("two distinct parents");
        assert_eq!(parents_of(&woven), vec![a, b]);
        assert!(woven.event().is_none());
    }

    #[test]
    fn a_spec_must_be_one_of_the_terms() {
        let flat = parse_literal("Flat(value=0.5)", &EVENT_VOCABULARY).expect("parses");
        assert!(Term::from_value(&flat).is_ok());
        let not_a_term =
            parse_literal("Note(on='block', lines=('x',))", &EVENT_VOCABULARY).expect("parses");
        assert!(Term::from_value(&not_a_term).is_err());
        assert!(Term::from_value(&Value::None).is_err());
        // And §7's BOUNDS now travel with the kind, which the placeholder this
        // replaced could not check: a term that parses is a term that evaluates.
        let out_of_range = parse_literal("Flat(value=1.5)", &EVENT_VOCABULARY).expect("parses");
        assert!(Term::from_value(&out_of_range).is_err());
    }

    #[test]
    fn trust_is_a_set_of_names_and_authored_always_binds() {
        let untrusted: BTreeSet<Actor> =
            [Actor::new("triage").expect("valid")].into_iter().collect();
        let completed = mk_completed("alpha", at(), "triage", "").expect("valid");
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
            &mk_completed("alpha", at(), "bassel", "").expect("valid"),
            &untrusted
        ));
    }
}
