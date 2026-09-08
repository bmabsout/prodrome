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
//! Two places print an instant differently, and both are RENDERINGS.
//! `json_authored`'s `created` and `source.date` are DATES, a genuine
//! narrowing of the value, and so are done here. `json_entry`'s `at` is
//! `isoformat(" ")` and used to be the page's to spell; since §6.7 it is
//! `Entry::at`, in the core — because the same string now has to come out of
//! the PyO3 binding too, and one spelling in two crates is one spelling too
//! many.

use std::collections::BTreeMap;

use prodrome::event::{Authored, Hash, Source, TodoEvent, TodoId};
use prodrome::fold::{Binding, Env, Untrusted};
use prodrome::fpl::{self, Instant, Outcome, Term};
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

/// The deployment's trust policy (§5), as the browser was told it. It arrives
/// from `/api/chain` rather than being assumed here: WHICH actors are
/// provisional is instance knowledge, and a core that guessed it would fold a
/// different chain than the server did while claiming to fold the same one.
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

fn json_source(source: &Source) -> Value {
    json!({
        "sender": source.sender.as_str(),
        "subject": source.subject.as_str(),
        "date": fpl::instant_of(source.date).format("%Y-%m-%d").to_string(),
        "hash": source.hash.as_str(),
        "thread_id": source.thread_id.as_str(),
        "message_id": source.message_id.as_str(),
    })
}

/// The reference's `json_authored` MINUS `rich`.
///
/// `rich` is typst's HTML, and typst is a compiler this core does not carry
/// (nor should: `markup.suspicious` and the trust gate around it are the
/// server's, and a browser that rendered its own markup would be a second
/// answer to "may this string become nodes"). The caller adds
/// `rich: {body: null, detail: null, rationale: null, waiting_on: null}`,
/// which is the shape `json_rich` already emits for a record nobody may
/// render, and which the frontend already answers by showing the source.
pub fn json_authored(record: &Authored) -> Value {
    json!({
        "kind": record.kind.as_str(),
        "created": fpl::instant_of(record.created).format("%Y-%m-%d").to_string(),
        "body": record.body.as_str(),
        "detail": record.detail.as_str(),
        "category": record.category.as_ref().map_or("", |c| c.as_str()),
        "waiting_on": record.waiting_on.as_str(),
        "rationale": Value::Array(record.rationale.iter().map(|r| Value::String(r.clone())).collect()),
        "spec": record.spec.as_ref().map_or(Value::Null, fpl::to_json),
        "actor": record.actor.as_str(),
        "at": fpl::iso(fpl::instant_of(record.at)),
        "source": record.source.as_ref().map_or(Value::Null, json_source),
        "subtodos": Value::Array(
            record
                .subtodos
                .iter()
                .map(|s| json!({"body": s.body, "done": s.done}))
                .collect(),
        ),
    })
}

pub fn json_content(content: &BTreeMap<TodoId, Authored>) -> Value {
    Value::Object(
        content
            .iter()
            .map(|(todo, record)| (todo.as_str().to_owned(), json_authored(record)))
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
        "unconfirmed": entry.standing.is_provisional(),
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
pub fn json_marker(name: &Hash, event: &TodoEvent) -> Value {
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
