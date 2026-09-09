//! THE REFERENCE PAYLOAD — the record shape `conformance/*.json` was taken
//! with, and an example of [`crate::payload::Payload`] written out in full.
//!
//! `Authored(todo, at, actor, kind, created, body, spec, rationale, category,
//! waiting_on, detail, source, subtodos, notes, note)`: one deployment's
//! decorations of a fulfillment curve — a todo's text in two rendering
//! languages, the message it came from, a checklist, comment runs — frozen here
//! because the vectors' bytes are frozen. Every `Authored(...)` print in
//! `conformance/*.json` parses and prints back byte for byte under this
//! payload; that is the whole of what this module is for.
//!
//! IT IS NOT THE FORMAT. A host defines its own [`Payload`] and gets its own
//! record kind; nothing in `event`, `store`, `fold`, `registers`, `view` or
//! `breaks` knows a field named here. Behind the default `reference` feature so
//! a host that wants none of it can turn it off.
//!
//! `MarkupSource` and `StringSource` are opaque text in this host's two
//! rendering languages; the database stores and prints them and never
//! interprets them.

use crate::event::{mk_record, newtype_str, Name, TodoEvent};
use crate::fpl::Term;
use crate::literal::{Call, Datetime, ProdromeError, Value};
use crate::payload::{
    bool_field, datetime_field, expect_call, spec_field, string_field, string_or_empty, strings,
    tuple_field, tuple_or_empty, Payload,
};

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

/// Where a [`Note`] may attach: a field of the record, or the block as a whole.
/// A closed set, so a note about `waiting_on` cannot be read as one about
/// `detail`.
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

/// A comment run attached to one field of a record.
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

/// THE PAYLOAD: everything this host attaches to a todo, beyond the `todo`,
/// `at` and `actor` the core owns.
#[derive(Debug, Clone, PartialEq)]
pub struct Todo {
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

// --- smart constructors ------------------------------------------------------

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

/// Every field of this payload, in declared order. Long by construction: the
/// shipped field set is frozen, so a builder that let one be forgotten would be
/// a way to print a different object under the same name.
#[allow(clippy::too_many_arguments)]
pub fn mk_todo(
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
) -> Result<Todo, ProdromeError> {
    if body.trim().is_empty() {
        return Err(ProdromeError::invalid("Authored.body cannot be empty"));
    }
    Ok(Todo {
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
    })
}

/// The whole record: the core's three fields and this payload's twelve — the
/// constructor every test and every vector in this repository is written
/// against.
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
) -> Result<TodoEvent<Todo>, ProdromeError> {
    mk_record(
        todo,
        at,
        actor,
        mk_todo(
            kind, created, body, spec, rationale, category, waiting_on, detail, source, subtodos,
            notes, note,
        )?,
    )
}

// --- the literal round trip --------------------------------------------------

fn text(value: impl Into<String>) -> Value {
    Value::Str(value.into())
}

fn optional_text(value: Option<&Name>) -> Value {
    Value::Str(
        value
            .map(|name| name.as_str().to_owned())
            .unwrap_or_default(),
    )
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
                field("sender", text(self.sender.as_str())),
                field("subject", text(self.subject.as_str())),
                field("date", Value::Datetime(self.date)),
                field("hash", text(self.hash.as_str())),
                field("thread_id", text(self.thread_id.as_str())),
                field("message_id", text(self.message_id.as_str())),
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
    mk_subtodo(&string_field(call, "body")?, bool_field(call, "done")?)
}

impl Payload for Todo {
    const KIND: &'static str = "Authored";

    const FIELDS: &'static [&'static str] = &[
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
    ];

    const VOCABULARY: &'static [(&'static str, &'static [&'static str])] = &[
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

    fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("kind", text(self.kind.as_str())),
            ("created", Value::Datetime(self.created)),
            ("body", text(self.body.as_str())),
            (
                "spec",
                self.spec
                    .as_ref()
                    .map_or(Value::None, |spec| spec.to_value()),
            ),
            (
                "rationale",
                tuple_of(&self.rationale, |line| text(line.clone())),
            ),
            ("category", optional_text(self.category.as_ref())),
            ("waiting_on", text(self.waiting_on.as_str())),
            ("detail", text(self.detail.as_str())),
            (
                "source",
                self.source.as_ref().map_or(Value::None, Source::to_value),
            ),
            ("subtodos", tuple_of(&self.subtodos, SubTodo::to_value)),
            ("notes", tuple_of(&self.notes, Note::to_value)),
            ("note", text(self.note.clone())),
        ]
    }

    fn from_fields(call: &Call) -> Result<Todo, ProdromeError> {
        mk_todo(
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
        )
    }

    fn spec(&self) -> Option<&Term> {
        self.spec.as_ref()
    }

    fn checklist_len(&self) -> usize {
        self.subtodos.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EVENT_SIGNATURES;
    use crate::fpl::TERM_SIGNATURES;
    use crate::payload::record_signature;

    #[test]
    fn the_smart_constructors_are_the_only_door() {
        assert!(mk_note("nowhere", vec!["x".into()]).is_err());
        assert!(mk_note("block", vec![]).is_err());
        assert!(mk_subtodo("  ", false).is_err());
        let at = Datetime::new(2026, 9, 6, 7, 3, 0, 0).expect("a real instant");
        assert!(mk_todo(
            "todo",
            at,
            "   ",
            None,
            vec![],
            "",
            "",
            "",
            None,
            vec![],
            vec![],
            ""
        )
        .is_err());
    }

    /// The stored shape: `Authored(todo, at, actor, <the twelve>)`, and the
    /// print is in exactly that order.
    #[test]
    fn the_record_signature_is_the_cores_three_then_this_payloads_twelve() {
        assert_eq!(
            record_signature::<Todo>(),
            vec![
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
            ]
        );
        let at = Datetime::new(2026, 9, 6, 7, 3, 0, 0).expect("a real instant");
        let todo = mk_todo(
            "todo",
            at,
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
        let named: Vec<&str> = todo.fields().into_iter().map(|(name, _)| name).collect();
        assert_eq!(
            named,
            Todo::FIELDS,
            "fields() answers with FIELDS, in order"
        );
    }

    /// §2's whitelist is a UNION, so a payload that reused one of the core's
    /// names would be unreachable rather than in force. This payload does not.
    #[test]
    fn the_payloads_names_do_not_collide_with_the_cores() {
        let core: Vec<&str> = TERM_SIGNATURES
            .iter()
            .chain(EVENT_SIGNATURES.iter())
            .map(|(name, _)| *name)
            .collect();
        for (name, _) in Todo::VOCABULARY {
            assert!(!core.contains(name), "{name} is one of the core's names");
        }
        assert!(!core.contains(&Todo::KIND));
    }
}
