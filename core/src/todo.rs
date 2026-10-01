//! The todo schema, the reference instance of [`Schema`]: §4's six kinds and
//! the record kind, about a todo, routed into its state, spec and content
//! registers and its tendings (§6).
//!
//! Every legacy envelope belongs to it, and every conformance vector was taken
//! at it.

use std::marker::PhantomData;

use crate::event::{event_from_value, Actor, TodoEvent, TodoId};
use crate::fold::{Discrete, Kind, RegisterType, Registers, Write};
use crate::literal::{Datetime, ProdromeError, Signature, Table, Value, Vocabulary};
use crate::payload::{record_signature, Payload};
use crate::schema::Schema;
use crate::term::schema::signatures;

/// The todo's six kinds, name and declared field order: lifecycle, care and
/// price, whose fields are the database's semantics.
pub const TODO_SIGNATURES: &[(&str, &[&str])] = &[
    ("Created", &["todo", "at", "actor", "text", "note"]),
    ("Completed", &["todo", "at", "actor", "note"]),
    ("Cancelled", &["todo", "at", "actor", "note"]),
    ("Reopened", &["todo", "at", "actor", "note"]),
    ("Tended", &["todo", "at", "actor", "note"]),
    ("SpecRevised", &["todo", "at", "actor", "spec", "note"]),
];

/// The todo schema's whitelist: §7's terms (a spec nests in a `SpecRevised`
/// or a record), the six kinds, and the PAYLOAD's record kind, whose fields
/// are `todo, at, actor` and then `P::FIELDS`, with the constructors those
/// fields nest. The core's names win: a payload that named its record
/// `Created` would be unreachable, not in force.
pub struct TodoVocabulary<P> {
    /// `todo, at, actor` then `P::FIELDS`, owned: the one signature that is
    /// not a compile-time table.
    record: Vec<&'static str>,
    payload: PhantomData<P>,
}

impl<P: Payload> Default for TodoVocabulary<P> {
    fn default() -> TodoVocabulary<P> {
        TodoVocabulary {
            record: record_signature::<P>(),
            payload: PhantomData,
        }
    }
}

impl<P: Payload> Vocabulary for TodoVocabulary<P> {
    fn signature(&self, name: &str) -> Option<Signature<'_>> {
        if let Some(signature) = signatures().signature(name) {
            return Some(signature);
        }
        if let Some(signature) = Table(TODO_SIGNATURES).find(name) {
            return Some(signature);
        }
        if name == P::KIND {
            return Some(Signature::Fields(&self.record));
        }
        Table(P::VOCABULARY).find(name)
    }
}

impl<P: Payload> Schema for TodoEvent<P> {
    type Vocabulary = TodoVocabulary<P>;
    type Key = TodoId;
    type Register = Kind;

    const REGISTERS: &'static [Kind] = &[Kind::State, Kind::Spec, Kind::Content];

    type Registers<'a> = Registers<'a, P>;

    fn to_value(&self) -> Value {
        TodoEvent::to_value(self)
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        event_from_value(value)
    }

    fn key(&self) -> &TodoId {
        self.todo()
    }

    fn at(&self) -> Datetime {
        TodoEvent::at(self)
    }

    fn actor(&self) -> &Actor {
        TodoEvent::actor(self)
    }

    /// A lifecycle event writes the state, a repricing the spec, a record its
    /// content and, when it carries one, its spec. A tending accumulates and
    /// supersedes nothing; `Created` writes nothing.
    fn writes(&self) -> impl Iterator<Item = Kind> {
        Write::of(self).filter_map(|write| write.kind())
    }

    /// A content record binds whoever wrote it (§6.2, §6.3): writing content
    /// is what a writer the policy does not stand behind is for.
    fn asks(&self) -> bool {
        !matches!(self, TodoEvent::Authored(_))
    }
}

/// The state register: a todo's lifecycle, discrete.
pub struct State;

/// The spec register: a todo's authored price, discrete.
pub struct Spec;

/// The content register: a todo's record, discrete.
pub struct Content;

/// A discrete register's value is the WHOLE EVENT that wrote it, not the
/// field it reads: two events that agree on a spec stay two candidates, as
/// they always were.
fn whole<P: Payload>(event: &TodoEvent<P>, kind: Kind) -> Option<Discrete<&TodoEvent<P>>> {
    Schema::writes(event)
        .any(|written| written == kind)
        .then_some(Discrete(event))
}

impl<P: Payload> RegisterType<TodoEvent<P>> for State {
    type Value<'e> = Discrete<&'e TodoEvent<P>>;

    fn value(event: &TodoEvent<P>) -> Option<Discrete<&TodoEvent<P>>> {
        whole(event, Kind::State)
    }
}

impl<P: Payload> RegisterType<TodoEvent<P>> for Spec {
    type Value<'e> = Discrete<&'e TodoEvent<P>>;

    fn value(event: &TodoEvent<P>) -> Option<Discrete<&TodoEvent<P>>> {
        whole(event, Kind::Spec)
    }
}

impl<P: Payload> RegisterType<TodoEvent<P>> for Content {
    type Value<'e> = Discrete<&'e TodoEvent<P>>;

    fn value(event: &TodoEvent<P>) -> Option<Discrete<&TodoEvent<P>>> {
        whole(event, Kind::Content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{mk_completed, mk_created, mk_tended};
    use crate::reference::{mk_authored, Todo};

    fn at() -> Datetime {
        Datetime::new(2026, 9, 6, 7, 3, 0, 0).expect("a real instant")
    }

    #[test]
    fn a_todo_event_writes_the_registers_its_kind_routes_it_to() {
        let created: TodoEvent<Todo> = mk_created("alpha", at(), "bassel", "", "").expect("valid");
        let done: TodoEvent<Todo> = mk_completed("alpha", at(), "bassel", "").expect("valid");
        let tended: TodoEvent<Todo> = mk_tended("alpha", at(), "bassel", "").expect("valid");
        let record: TodoEvent<Todo> = mk_authored(
            "alpha",
            at(),
            "triage",
            "todo",
            at(),
            "body",
            Some(crate::fpl::mk_flat(0.5).expect("valid")),
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
        assert_eq!(created.writes().count(), 0);
        assert_eq!(tended.writes().count(), 0, "a tending supersedes nothing");
        assert_eq!(done.writes().collect::<Vec<_>>(), [Kind::State]);
        assert_eq!(
            record.writes().collect::<Vec<_>>(),
            [Kind::Content, Kind::Spec]
        );
        assert!(done.asks() && !record.asks());
        assert_eq!(
            <TodoEvent<Todo> as Schema>::from_value(&Schema::to_value(&record)),
            Ok(record)
        );
    }
}
