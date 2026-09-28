use crate::event::{
    event_from_value, field, hashes, hashes_value, sorted_distinct, Hash, TodoEvent,
};
use crate::literal::{Call, ProdromeError, Value};
use crate::payload::{required, string_field, Payload};

/// `Change(genesis, deps, event)`: an event named by its prodrome and the
/// writes it supersedes, never by where it was written.
#[derive(Debug, Clone, PartialEq)]
pub struct Change<P> {
    pub genesis: Hash,
    pub deps: Vec<Hash>,
    pub event: TodoEvent<P>,
}

pub fn mk_change<P>(
    genesis: Hash,
    deps: Vec<Hash>,
    event: TodoEvent<P>,
) -> Result<Change<P>, ProdromeError> {
    Ok(Change {
        genesis,
        deps: sorted_distinct("Change.deps", "dep", deps)?,
        event,
    })
}

impl<P: Payload> Change<P> {
    pub fn to_value(&self) -> Value {
        Value::call(
            "Change",
            vec![
                field("genesis", Value::str(self.genesis.as_str())),
                field("deps", hashes_value(&self.deps)),
                field("event", self.event.to_value()),
            ],
        )
    }

    pub fn from_call(call: &Call) -> Result<Change<P>, ProdromeError> {
        mk_change(
            Hash::new(string_field(call, "genesis")?)?,
            hashes(call, "deps")?,
            event_from_value(required(call, "event")?)?,
        )
    }
}
