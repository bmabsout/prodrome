//! The JSON shapes that cross the boundary, and nothing else.
//!
//! `lib.rs` is the six exported functions; this is what their arguments and
//! their answers LOOK like, written down once so that a page's own API module's
//! mirror has a single thing to mirror. Everything is JSON text: a string in,
//! a string out. That is deliberate — `serde-wasm-bindgen` would let a JS
//! object cross directly and cost a second serialisation format to keep in
//! step with the wire the server already speaks, and this crate's whole reason
//! to exist is that there is ONE reading of the data.
//!
//! ONE INSTANT SHAPE IN AND OUT: `fpl::iso` (`2026-09-06T11:32:07`), what
//! `datetime.isoformat()` prints and what every `at` on the JSON API carries.
//! So `fold`'s `env` can be handed straight back to [`crate::fulfillment`],
//! and `fold`'s `history` straight back to [`crate::series_knots`], with
//! nothing rewritten in between — a translation step is where a second reading
//! grows.
//!
//! A RECORD'S FIELDS ARE THE HOST'S, so this file does not name one. §4's
//! record kind is `KIND(todo, at, actor, <the payload's fields>)`, and
//! [`json_record`] serialises exactly that: the three the core owns, then the
//! payload's own `fields()` as a JSON object keyed by field name, each a §2
//! literal mapped by [`json_literal`]. A browser therefore reads the same
//! names its store holds, whatever payload the module was built with — and
//! this crate stops having an opinion about what a body is.
//!
//! `json_entry`'s `at` is `isoformat(" ")` and used to be the page's to spell;
//! since §6.7 it is `Entry::at`, in the core — one spelling, in one place.

use std::collections::BTreeMap;

use prodrome::event::{Authored, Hash, TodoEvent, TodoId};
use prodrome::fold::{Binding, Env};
use prodrome::fpl::{self, Instant, Outcome, Term};
use prodrome::literal;
use prodrome::payload::Payload;
use prodrome::policy::Untrusted;
use prodrome::view::Entry;
use serde::Deserialize;
use serde_json::{json, Map, Value};

/// One stored object as the browser holds it: the name it claims and the exact
/// bytes that name is a hash of. `/api/chain`'s `hash` and `literal`.
#[derive(Debug, Deserialize)]
pub struct ObjectIn {
    pub hash: String,
    pub text: String,
}

/// Everything this crate refuses with. A `String` and not an enum: every one
/// of these ends up as a JS exception message read by a person, the callers
/// never branch on the reason, and an enum whose only observation is its
/// `Display` is a ceremony with no reader.
pub type Refusal = String;

pub fn parse_objects(json_text: &str) -> Result<Vec<ObjectIn>, Refusal> {
    serde_json::from_str(json_text).map_err(|e| format!("objects: expected [{{hash, text}}] ({e})"))
}

/// The deployment's STANDING policy (§5), as the browser was told it. It
/// arrives from `/api/chain` rather than being assumed here: WHICH actors a
/// host stands behind is instance knowledge, and a core that guessed it would
/// fold a different chain than the server did while claiming to fold the same
/// one.
///
/// THE WIRE IS UNCHANGED. `untrusted` is still a JSON array of actor names;
/// since 0.3 the core takes a `policy::Policy` rather than that array, and this
/// is where the array becomes one — `Untrusted`, the reference policy, which is
/// the rule this argument always meant.
pub fn parse_untrusted(json_text: &str) -> Result<Untrusted, Refusal> {
    let names: Vec<String> = serde_json::from_str(json_text)
        .map_err(|e| format!("untrusted: expected a list of actor names ({e})"))?;
    let mut actors = Vec::with_capacity(names.len());
    for name in names {
        actors.push(prodrome::event::Actor::new(name).map_err(|e| format!("untrusted: {e}"))?);
    }
    Ok(Untrusted::of(actors))
}

pub fn parse_instant(field: &str, text: &str) -> Result<Instant, Refusal> {
    fpl::parse_iso(text).map_err(|e| format!("{field}: {e}"))
}

pub fn parse_moment(field: &str, text: Option<String>) -> Result<Option<Instant>, Refusal> {
    text.map(|t| parse_instant(field, &t)).transpose()
}

pub fn parse_term(field: &str, json_text: &str) -> Result<Term, Refusal> {
    let value: Value =
        serde_json::from_str(json_text).map_err(|e| format!("{field}: not JSON ({e})"))?;
    fpl::from_json(&value).map_err(|e| format!("{field}: {e}"))
}

/// `{"<name>": {"kind": "Completed" | "Cancelled", "at": "<iso>"}}` — §7's
/// environment, and byte for byte the `env` of `conformance/fpl.json`, so a
/// vector file is a legal argument without translation.
pub fn parse_env(json_text: &str) -> Result<fpl::Env, Refusal> {
    let raw: BTreeMap<String, EnvEntry> =
        serde_json::from_str(json_text).map_err(|e| format!("env: {e}"))?;
    let mut env = fpl::Env::new();
    for (name, entry) in raw {
        env.insert(name, entry.outcome()?);
    }
    Ok(env)
}

#[derive(Debug, Deserialize)]
pub struct EnvEntry {
    pub kind: String,
    pub at: String,
}

impl EnvEntry {
    fn outcome(&self) -> Result<Outcome, Refusal> {
        let at = parse_instant("env.at", &self.at)?;
        match self.kind.as_str() {
            "Completed" => Ok(Outcome::Completed(at)),
            "Cancelled" => Ok(Outcome::Cancelled(at)),
            other => Err(format!(
                "env.kind must be \"Completed\" or \"Cancelled\", got {other:?}"
            )),
        }
    }
}

/// The environment as a FUNCTION OF TIME (§6.5), as `fold`'s `history` emits
/// it and `series_knots` reads it back: per todo, the chain's writes in causal
/// order, `null` where a `Reopened` cleared the binding.
///
/// It is a separate argument from `env` and not a convenience over it: a knot
/// at a past instant must be evaluated against the environment AS OF that
/// instant (`view.series_of` does exactly this), and a single `env` snapshot
/// would bind a dependency before it was completed.
#[derive(Debug, Deserialize)]
pub struct HistoryEntry {
    pub at: String,
    pub binding: Option<EnvEntry>,
}

pub struct History(BTreeMap<String, Vec<(Instant, Option<Outcome>)>>);

impl History {
    pub fn parse(json_text: &str) -> Result<History, Refusal> {
        let raw: BTreeMap<String, Vec<HistoryEntry>> =
            serde_json::from_str(json_text).map_err(|e| format!("history: {e}"))?;
        let mut out = BTreeMap::new();
        for (todo, entries) in raw {
            let mut timeline = Vec::with_capacity(entries.len());
            for entry in entries {
                let at = parse_instant("history.at", &entry.at)?;
                let binding = entry.binding.map(|b| b.outcome()).transpose()?;
                timeline.push((at, binding));
            }
            out.insert(todo, timeline);
        }
        Ok(History(out))
    }

    /// The environment at `t`: per todo, the last write dated at or before it.
    /// The timeline is in CHAIN order and is never sorted — §6's ordering rule
    /// holds on this side of the boundary too.
    pub fn at(&self, t: Instant) -> fpl::Env {
        let mut env = fpl::Env::new();
        for (todo, timeline) in &self.0 {
            let mut current = None;
            for (at, binding) in timeline {
                if *at <= t {
                    current = *binding;
                }
            }
            if let Some(outcome) = current {
                env.insert(todo.clone(), outcome);
            }
        }
        env
    }
}

// --- out ---------------------------------------------------------------------

/// One binding, in the shape §7's environment has: the two outcomes are an
/// ADT and never a boolean, because they mean OPPOSITE things downstream.
///
/// ONE ENV SHAPE, and this is it — `{kind, at}` with an ISO instant, which is
/// what `parse_env` reads, what `conformance/fpl.json` holds, and therefore
/// what a caller can hand straight back to [`crate::fulfillment`] without
/// rewriting anything. [`json_entry`]'s `state`/`at` is a RENDERING of this
/// (lowercased, `isoformat(" ")`), which `Entry::state`/`Entry::at` perform in
/// the core so that both bindings spell it one way.
pub fn json_binding(binding: Binding) -> Value {
    json!({ "kind": binding.kind(), "at": fpl::iso(binding.at()) })
}

pub fn json_env(env: &Env) -> Value {
    Value::Object(
        env.iter()
            .map(|(todo, binding)| (todo.as_str().to_owned(), json_binding(*binding)))
            .collect(),
    )
}

pub fn json_terms(terms: &BTreeMap<TodoId, Term>) -> Value {
    Value::Object(
        terms
            .iter()
            .map(|(todo, term)| (todo.as_str().to_owned(), fpl::to_json(term)))
            .collect(),
    )
}

/// A §2 literal as JSON, by the obvious mapping — the ONE reading of a
/// payload's fields this boundary has.
///
/// `None` is `null`, a bool is a bool, a string is a string; an integer is a
/// number where one fits and its print where it does not (§2's integers are
/// unbounded and JSON's are not); a float is a number; a datetime is
/// [`fpl::iso`], the one instant shape this whole file speaks; a timedelta is
/// its three components, which is what it IS; a tuple is an array; and a
/// constructor call is an object of its fields with its name under `"kind"`.
///
/// A nested call whose own field is named `kind` keeps the FIELD and loses the
/// tag, because a reader is after the value; the core kinds and the reference
/// payload have no such field, and a host that adds one should expect it.
///
/// A TERM ARRIVES AS ITS LITERAL SHAPE HERE, not as `fpl::to_json`'s — this
/// mapping is a payload's, and a payload's fields are opaque to this crate. A
/// caller that wants a term as `to_json` reads `fold`'s `specs`/`flatten`, or
/// hands the print to [`crate::term_json`].
pub fn json_literal(value: &literal::Value) -> Value {
    match value {
        literal::Value::None => Value::Null,
        literal::Value::Bool(flag) => Value::Bool(*flag),
        literal::Value::Int(int) => int
            .as_i64()
            .map_or_else(|| Value::String(literal::print_literal(value)), Value::from),
        literal::Value::Float(float) => Value::from(float.get()),
        literal::Value::Str(text) => Value::String(text.clone()),
        literal::Value::Datetime(at) => Value::String(fpl::iso(fpl::instant_of(*at))),
        literal::Value::Timedelta(delta) => json!({
            "days": delta.days(),
            "seconds": delta.seconds(),
            "microseconds": delta.microseconds(),
        }),
        literal::Value::Tuple(items) => Value::Array(items.iter().map(json_literal).collect()),
        literal::Value::Call(call) => {
            let mut out = Map::new();
            out.insert("kind".to_owned(), Value::String(call.name.clone()));
            for (name, field) in &call.fields {
                out.insert(name.clone(), json_literal(field));
            }
            Value::Object(out)
        }
    }
}

/// One content record: the three fields §4 gives every event, then the
/// PAYLOAD's own, keyed by the names it stores them under.
///
/// No rendering of a body, and there could not be one: markup is a compiler
/// this core does not carry (nor should — a browser that rendered its own
/// markup would be a second answer to "may this string become nodes"), and a
/// field's meaning is the host's anyway. A caller that renders adds its own
/// keys beside these.
pub fn json_record<P: Payload>(record: &Authored<P>) -> Value {
    let mut out = Map::new();
    out.insert(
        "todo".to_owned(),
        Value::String(record.todo.as_str().to_owned()),
    );
    out.insert(
        "at".to_owned(),
        Value::String(fpl::iso(fpl::instant_of(record.at))),
    );
    out.insert(
        "actor".to_owned(),
        Value::String(record.actor.as_str().to_owned()),
    );
    for (name, value) in record.payload.fields() {
        out.insert(name.to_owned(), json_literal(&value));
    }
    Value::Object(out)
}

pub fn json_content<P: Payload>(content: &BTreeMap<TodoId, Authored<P>>) -> Value {
    Value::Object(
        content
            .iter()
            .map(|(todo, record)| (todo.as_str().to_owned(), json_record(record)))
            .collect(),
    )
}

/// One §6.7 entry, in the shape the reference's row carried and the
/// wire still carries — one row, one reading, on every host.
///
/// `spec` is the todo's §6.4 function as its canonical PRINT and not as
/// `to_json`, which is the exception to this file's "terms travel as JSON"
/// rule and is deliberate: it is the value's IDENTITY (§3), it is what the
/// vector compares, and no page reads it. A caller that wants the JSON shape
/// has [`crate::term_json`], the §2 → §7 door.
pub fn json_entry(entry: &Entry) -> Value {
    json!({
        "todo": entry.todo.as_str(),
        "state": entry.state(),
        "at": entry.at(),
        "claimed": entry.claimed(),
        "value": entry.value(),
        "unconfirmed": entry.confidence.is_provisional(),
        "conflicts": Value::Object(
            entry
                .conflicts
                .iter()
                .map(|(kind, writes)| {
                    (
                        kind.as_str().to_owned(),
                        strings(writes.iter().map(|write| write.as_str().to_owned())),
                    )
                })
                .collect(),
        ),
        "content": entry.content.as_ref().map(Hash::as_str),
        "spec": entry.spec().map(fpl::print_term),
        "stream": strings(entry.stream.iter().map(|name| name.as_str().to_owned())),
    })
}

/// One event's identity on a todo's timeline — `json_series`'s marker,
/// and what a locally folded entry's `stream` is a list of.
pub fn json_marker<P: Payload>(name: &Hash, event: &TodoEvent<P>) -> Value {
    json!({
        "at": fpl::iso(fpl::instant_of(event.at())),
        "kind": event.kind_name(),
        "actor": event.actor().as_str(),
        "hash": name.as_str(),
    })
}

pub fn object(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect::<Map<String, Value>>(),
    )
}

pub fn strings(items: impl IntoIterator<Item = String>) -> Value {
    Value::Array(items.into_iter().map(Value::String).collect())
}
