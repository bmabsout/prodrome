//! Shared plumbing for the conformance suites (SPEC §9): loading a vector
//! file, comparing two JSON trees the way the spec compares them — shape and
//! strings exactly, floats to 1e-9 — the synthetic corpus of stored objects
//! the grammar and the event suites both read, and the RANDOM LOG GENERATOR
//! (`a_log` and what it is built from) that `tests/fold_laws.rs` draws its
//! properties from and `examples/generate_view_vectors.rs` draws
//! `conformance/view/*.json` from — one generator, read by a property and
//! frozen by a vector file, never two.
//!
//! Included by several test binaries, each of which uses a part of it; the
//! part one binary does not call is not dead code, it is another's.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{Duration, NaiveDate};
use proptest::prelude::*;
use serde_json::{json, Value};

/// The largest float deviation any comparison has seen, so a suite can report
/// its margin rather than only its verdict.
pub static WORST: AtomicU64 = AtomicU64::new(0);

pub fn tolerance() -> f64 {
    1e-9
}

pub fn worst_seen() -> f64 {
    f64::from_bits(WORST.load(Ordering::Relaxed))
}

fn record(delta: f64) {
    let mut current = WORST.load(Ordering::Relaxed);
    while delta > f64::from_bits(current) {
        match WORST.compare_exchange_weak(
            current,
            delta.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

pub fn vectors(name: &str) -> Value {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "conformance", name]
        .iter()
        .collect();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()))
}

/// `mine` against `theirs`, in the spec's terms. Returns the first
/// disagreement as a path plus a message.
pub fn agrees(path: &str, mine: &Value, theirs: &Value) -> Result<(), String> {
    match (mine, theirs) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (
                a.as_f64().unwrap_or(f64::NAN),
                b.as_f64().unwrap_or(f64::NAN),
            );
            let delta = (a - b).abs();
            record(delta);
            if delta <= tolerance() * a.abs().max(b.abs()).max(1.0) {
                Ok(())
            } else {
                Err(format!("{path}: {a} != {b} (off by {delta:e})"))
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Err(format!("{path}: {} items, expected {}", a.len(), b.len()));
            }
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                agrees(&format!("{path}[{i}]"), x, y)?;
            }
            Ok(())
        }
        (Value::Object(a), Value::Object(b)) => {
            let mut mine_keys: Vec<&String> = a.keys().collect();
            let mut theirs_keys: Vec<&String> = b.keys().collect();
            mine_keys.sort();
            theirs_keys.sort();
            if mine_keys != theirs_keys {
                return Err(format!("{path}: keys {mine_keys:?} != {theirs_keys:?}"));
            }
            for (k, x) in a {
                agrees(&format!("{path}.{k}"), x, &b[k])?;
            }
            Ok(())
        }
        (x, y) if x == y => Ok(()),
        (x, y) => Err(format!("{path}: {x} != {y}")),
    }
}

// --- the synthetic corpus ----------------------------------------------------
//
// The suites that used to read a vector file of stored objects read this
// instead: a chain of stored objects built here, through the crate's own
// smart constructors, covering every kind of §4 and every production of §2
// that a stored object can carry. It is authored rather than generated
// because the point is COVERAGE OF THE GRAMMAR — the quoting rules, the
// microsecond argument, the empty tuple, an absent optional field — and a
// random draw covers what it happens to draw.
//
// `tests/literals.rs` reads these prints through the OPEN vocabulary (§2 and
// nothing else); `tests/events.rs` reads the same prints through the CLOSED
// one as typed envelopes (§4). One corpus, two readings, exactly as the two
// files always stood to each other.

use prodrome::event::{
    mk_cancelled, mk_completed, mk_created, mk_reopened, mk_sealed, mk_spec_revised, mk_woven,
    seal_hash, Envelope, Hash, TodoEvent,
};
use prodrome::fpl::{self, mk_conj, mk_decay, mk_flat, mk_piecewise, mk_within, Instant, Term};
use prodrome::literal::Datetime;
pub use prodrome::reference::{mk_authored, mk_note, mk_source, mk_subtodo, Todo};

/// The corpus is written against the REFERENCE PAYLOAD (`prodrome::reference`)
/// — the record shape `conformance/*.json` was taken with, and therefore the
/// one every vector in this repository round-trips under.
pub type Event = TodoEvent<Todo>;
pub type Object = Envelope<Todo>;

/// A moment in the corpus's window, with microseconds where §2's seventh
/// argument has to be exercised.
pub fn at(day: u32, hour: u32, micro: u32) -> Datetime {
    Datetime::new(2026, 9, day, hour, 30, 15, micro).expect("a real instant")
}

/// A spec with the shapes that make the printer work: a schedule to splice, a
/// conjunction to weigh, a window to sample, floats that print in exponent
/// form and floats that do not.
pub fn a_spec() -> Term {
    let deadline = fpl::instant_of(at(20, 9, 0));
    let decay = mk_decay(
        0.55,
        0.05,
        deadline,
        fpl::delta_from_hours(72.0),
        Some(fpl::instant_of(at(10, 9, 0))),
    )
    .expect("the reference's decay defaults");
    let conj = mk_conj(
        vec![decay, mk_flat(0.25).expect("a fulfillment")],
        fpl::PRIORITY_POWER,
    )
    .expect("a conjunction");
    let within = mk_within(fpl::delta_from_hours(48.0), 2.0, conj).expect("a window");
    mk_piecewise(
        mk_flat(1e-05).expect("a fulfillment"),
        vec![(fpl::instant_of(at(12, 0, 0)), within)],
    )
    .expect("a schedule")
}

/// One event of every kind in §4, with the awkward values inside them: a
/// string that carries both quote characters, a newline and a tab, printable
/// non-ASCII and an unprintable code point, an empty optional field, an empty
/// tuple, and a record whose fields are all filled.
pub fn events() -> Vec<Event> {
    let ok = |result: Result<Event, prodrome::literal::ProdromeError>| {
        result.expect("the corpus only builds events the constructors admit")
    };
    vec![
        ok(mk_created(
            "todo-1",
            at(1, 9, 0),
            "bassel",
            "write the README for a stranger",
            "",
        )),
        ok(mk_created(
            "todo-2",
            at(1, 10, 500_000),
            "triage",
            "it's \"quoted\", \ttabbed,\nnewlined, é ü ✓ 日本 🙂 and \u{200b}zero-width",
            "a note with a \\ backslash in it",
        )),
        ok(mk_spec_revised(
            "todo-1",
            at(2, 9, 0),
            "bassel",
            a_spec(),
            "",
        )),
        ok(mk_authored(
            "todo-1",
            at(3, 9, 250),
            "bassel",
            "todo",
            at(1, 9, 0),
            "= A heading\n\nBody with \\@escapes and #strong[markup].",
            Some(a_spec()),
            vec![
                "because it is the contract".to_owned(),
                "'quoted'".to_owned(),
            ],
            "writing",
            "review from Bassel",
            "The detail, on its own line.",
            Some(mk_source(
                "someone@example.com",
                "Re: the README",
                at(1, 8, 0),
                "0".repeat(64).as_str(),
                "thread-7",
                "<message@example.com>",
            )),
            vec![
                mk_subtodo("draft it", true).expect("a subtodo"),
                mk_subtodo("read it back", false).expect("a subtodo"),
            ],
            vec![mk_note("block", vec!["a note line".to_owned()]).expect("a note")],
            "",
        )),
        // The same kind with every optional field absent: no spec, no source,
        // no category, empty tuples.
        ok(mk_authored(
            "todo-2",
            at(3, 10, 0),
            "triage",
            "mail",
            at(1, 10, 0),
            "A record with nothing optional in it.",
            None,
            vec![],
            "",
            "",
            "",
            None,
            vec![],
            vec![],
            "",
        )),
        ok(mk_completed("todo-1", at(4, 9, 0), "bassel", "done")),
        ok(mk_reopened("todo-1", at(5, 9, 0), "bassel", "")),
        ok(mk_cancelled("todo-2", at(6, 9, 999_999), "triage", "")),
    ]
}

/// The events above sealed into a store's shape: a chain, then a FORK — two
/// objects on one parent — joined by a `Woven` that carries no event, so the
/// corpus holds every envelope kind §3 admits, genesis included.
pub fn corpus() -> Vec<(Hash, Object)> {
    let mut events = events();
    let last = events.pop().expect("the corpus is not empty");
    let fork = events.pop().expect("the corpus is not empty");
    let mut objects: Vec<(Hash, Object)> = Vec::new();
    let mut prev: Option<Hash> = None;
    for event in events {
        let envelope = mk_sealed(prev.clone(), event);
        prev = Some(seal_hash(&envelope));
        objects.push((seal_hash(&envelope), envelope));
    }
    let tip = prev.expect("the chain has a tip");
    let left = mk_sealed(Some(tip.clone()), fork);
    let right = mk_sealed(Some(tip), last);
    let (left_name, right_name) = (seal_hash(&left), seal_hash(&right));
    let merge = mk_woven(vec![left_name.clone(), right_name.clone()], None).expect("a merge");
    objects.push((left_name, left));
    objects.push((right_name, right));
    objects.push((seal_hash(&merge), merge));
    objects
}

// --- the random log generator -------------------------------------------
//
// Moved out of `tests/fold_laws.rs` (2026, view vectors) so that the SAME
// generator draws both a property's cases (`fold_laws.rs`'s `proptest!`
// block, unmoved) and `conformance/view/*.json`'s frozen ones
// (`examples/generate_view_vectors.rs`, over a fixed seed). A vector file
// generated by a different draw than the properties run against would be
// a second, undocumented generator; this is why there is exactly one.

/// The reference generator's three todos.
pub const TODOS: [&str; 3] = ["alpha", "beta", "gamma"];
/// The reference generator's actors, in its proportions: two writes trusted
/// for every one that is not.
pub const ACTORS: [&str; 3] = ["bassel", "bassel", "triage"];
/// The generator's window: sixty days from the origin.
pub const WINDOW: i64 = 60 * 86_400;

pub fn origin() -> Instant {
    NaiveDate::from_ymd_opt(2026, 9, 1)
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .expect("a real date")
}

pub fn moment(seconds: i64) -> Datetime {
    fpl::datetime_of(origin() + Duration::seconds(seconds)).expect("inside the grammar's years")
}

/// Later than every event any generator here produces, so a dated fold sees
/// the whole log and can be compared with the undated register fold.
pub fn far() -> Datetime {
    moment(WINDOW * 20)
}

pub fn ok<T>(result: Result<T, prodrome::literal::ProdromeError>) -> T {
    result.expect("the generator only builds events the constructors admit")
}

/// An event with its instant left OPEN. The generators produce these, and a
/// caller realises them at whatever instant it is about: the prefix law
/// needs an event dated after the log, the shift law needs the same log
/// dated differently, and the view vectors need each log dated once and then
/// queried at several instants — none of which is expressible if the stamp
/// is baked in.
#[derive(Debug, Clone)]
pub struct Draft {
    pub todo: &'static str,
    pub actor: &'static str,
    pub roll: u8,
    pub spec: Option<Term>,
    pub items: usize,
    pub text: u32,
}

impl Draft {
    pub fn at(&self, at: Datetime) -> Event {
        self.at_with_note(at, "")
    }

    pub fn at_with_note(&self, at: Datetime, note: &str) -> Event {
        match self.roll {
            0 => ok(mk_created(
                self.todo,
                at,
                self.actor,
                &format!("t{}", self.text),
                note,
            )),
            1 | 2 => {
                let subtodos = (0..self.items)
                    .map(|i| ok(mk_subtodo(&format!("item {i}"), i % 3 == 0)))
                    .collect();
                ok(mk_authored(
                    self.todo,
                    at,
                    self.actor,
                    "todo",
                    at,
                    &format!("body {} \\@x", self.text),
                    self.spec.clone(),
                    vec![],
                    "",
                    "",
                    "",
                    None,
                    subtodos,
                    vec![],
                    note,
                ))
            }
            3 => ok(mk_spec_revised(
                self.todo,
                at,
                self.actor,
                self.spec
                    .clone()
                    .unwrap_or_else(|| fpl::mk_flat(0.5).expect("0.5 is a fulfillment")),
                note,
            )),
            4 => ok(mk_completed(self.todo, at, self.actor, note)),
            5 => ok(mk_cancelled(self.todo, at, self.actor, note)),
            _ => ok(mk_reopened(self.todo, at, self.actor, note)),
        }
    }
}

/// A spec the way the reference's generator draws one, shallow: the shapes
/// that make `flatten`'s normal form do work — a schedule to splice, a
/// conjunction to weigh, a window to sample — without the depth `fpl`'s own
/// laws already cover.
///
/// Named `a_random_spec` and not `a_spec`: this file already has [`a_spec`],
/// the ONE fixed spec the synthetic corpus (`corpus`, above) is authored
/// with, and the two are different things — a strategy that draws many
/// shapes against a value that is one, chosen for what it exercises.
pub fn a_random_spec() -> impl Strategy<Value = Term> {
    let leaf = prop_oneof![
        (0.02f64..0.98).prop_map(|v| fpl::mk_flat(v).expect("in [0, 1]")),
        (0.3f64..0.95, 0.0f64..0.2, 0i64..WINDOW, 6i64..400).prop_map(|(start, end, at, lead)| {
            fpl::mk_decay(
                start,
                end,
                origin() + Duration::seconds(at),
                Duration::hours(lead),
                None,
            )
            .expect("a well-formed decay")
        }),
    ];
    leaf.prop_recursive(2, 8, 2, |inner| {
        prop_oneof![
            (
                prop::collection::vec(inner.clone(), 1..3),
                prop::sample::select(vec![-4.0, -1.0, 0.0])
            )
                .prop_map(|(terms, p)| fpl::mk_conj(terms, p).expect("p is in range")),
            (
                inner.clone(),
                prop::collection::btree_set(0i64..WINDOW, 1..3),
                prop::collection::vec(inner, 2)
            )
                .prop_map(|(head, ats, terms)| fpl::mk_piecewise(
                    head,
                    ats.iter()
                        .zip(terms)
                        .map(|(at, term)| (origin() + Duration::seconds(*at), term))
                        .collect()
                )
                .expect("the instants are a sorted set"))
        ]
    })
}

pub fn a_draft() -> impl Strategy<Value = Draft> {
    (
        prop::sample::select(TODOS.to_vec()),
        prop::sample::select(ACTORS.to_vec()),
        0u8..7,
        // The generator carries a spec on most `Authored` records and not all.
        prop::option::weighted(0.85, a_random_spec()),
        prop::sample::select(vec![0usize, 0, 2, 3]),
        0u32..999,
    )
        .prop_map(|(todo, actor, roll, spec, items, text)| Draft {
            todo,
            actor,
            roll,
            spec,
            items,
            text,
        })
}

/// Drafts with their instants, ready to realise. Kept apart from the events so
/// a caller can re-date the same log.
pub fn a_schedule(size: std::ops::Range<usize>) -> impl Strategy<Value = Vec<(Draft, i64)>> {
    prop::collection::vec((a_draft(), 0i64..WINDOW), size).prop_map(|mut drafts| {
        // `a_log` sorts by instant, and Rust's sort is stable, so equal stamps
        // keep the order they were drawn in — the reference's `sorted` too.
        drafts.sort_by_key(|(_, at)| *at);
        drafts
    })
}

pub fn realise(schedule: &[(Draft, i64)], shift: i64) -> Vec<Event> {
    schedule
        .iter()
        .map(|(draft, at)| draft.at(moment(at + shift)))
        .collect()
}

pub fn a_log() -> impl Strategy<Value = Vec<Event>> {
    a_schedule(0..25).prop_map(|schedule| realise(&schedule, 0))
}

/// A log as the chain a single writer builds: each object sealed on the one
/// before, so the node's parents are real and its name is its own hash.
pub fn chain_of(log: &[Event]) -> Vec<prodrome::registers::Node<Todo>> {
    let mut prev: Option<Hash> = None;
    let mut nodes = Vec::with_capacity(log.len());
    for event in log {
        let envelope = mk_sealed(prev.clone(), event.clone());
        let name = seal_hash(&envelope);
        prev = Some(name.clone());
        nodes.push(prodrome::registers::Node::of(name, &envelope));
    }
    nodes
}

/// One §6.7 entry as `prodrome-wasm`'s `wire::json_entry` renders it — copied
/// rather than depended on, because `core` does not depend on `wasm` (the
/// dependency runs the other way) and a dev-dependency cycle across the
/// workspace is not worth it for one rendering function used by one
/// conformance suite. Keep this byte-for-byte with `wasm/src/wire.rs`'s
/// `json_entry`: this is the WIRE, and `conformance/view/*.json` is frozen
/// against it, not against a reading of the `Entry` struct's own fields.
pub fn json_entry(entry: &prodrome::view::Entry) -> Value {
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
                        Value::Array(
                            writes
                                .iter()
                                .map(|write| Value::String(write.as_str().to_owned()))
                                .collect(),
                        ),
                    )
                })
                .collect(),
        ),
        "content": entry.content.as_ref().map(Hash::as_str),
        "spec": entry.spec().map(fpl::print_term),
        "stream": Value::Array(
            entry
                .stream
                .iter()
                .map(|name| Value::String(name.as_str().to_owned()))
                .collect(),
        ),
    })
}
