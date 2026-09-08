//! Shared plumbing for the conformance suites (SPEC §9): loading a vector
//! file, comparing two JSON trees the way the spec compares them — shape and
//! strings exactly, floats to 1e-9 — and the synthetic corpus of stored
//! objects the grammar and the event suites both read.
//!
//! Included by several test binaries, each of which uses a part of it; the
//! part one binary does not call is not dead code, it is another's.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

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
    mk_authored, mk_cancelled, mk_completed, mk_created, mk_note, mk_reopened, mk_sealed,
    mk_source, mk_spec_revised, mk_subtodo, mk_woven, seal_hash, Envelope, Hash, TodoEvent,
};
use prodrome::fpl::{self, mk_conj, mk_decay, mk_flat, mk_piecewise, mk_within, Term};
use prodrome::literal::Datetime;

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
pub fn events() -> Vec<TodoEvent> {
    let ok = |result: Result<TodoEvent, prodrome::literal::ProdromeError>| {
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
pub fn corpus() -> Vec<(Hash, Envelope)> {
    let mut events = events();
    let last = events.pop().expect("the corpus is not empty");
    let fork = events.pop().expect("the corpus is not empty");
    let mut objects: Vec<(Hash, Envelope)> = Vec::new();
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
