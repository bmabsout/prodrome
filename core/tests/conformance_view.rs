//! SPEC §9.9 (§6.7) over `conformance/view/*.py`: every field of every
//! `view::entries` row, replayed under the two reference policies the vectors
//! were taken with — their `untrusted` arrays, read through `policy::Untrusted`.
//!
//! THE VECTORS ARE FROZEN. Nothing in this file regenerates them — a chain
//! this test cannot rebuild byte-for-byte is a chain it is supposed to fail
//! on, not paper over. Regeneration is `cargo run --example
//! generate_view_vectors -p prodrome-core`, run by hand; CI never runs
//! `cargo run --example` and this test never calls it either, which is what
//! makes `conformance/view/*.py` evidence and not a cache. See that
//! example's module doc for the seed and the generator (`tests/common/mod.rs`'s
//! `a_log`, the same one `tests/fold_laws.rs`'s properties draw from — one
//! generator, not a second).
//!
//! Each case is a log, frozen as its events' canonical PRINTS in append
//! order — never as a map keyed by name, because a JSON object carries no
//! order of its own and this chain's names are hashes, which sort by
//! nothing a reader would call "order". This file parses each print back
//! with `parse_event` and reseals the array with `common::chain_of`, the
//! same function the generator used, so a name here is trusted no further
//! than the bytes that produced it and the two sides' hashes agree without
//! either one freezing them — exactly as `tests/dag.rs` insists a store's
//! reader rebuild its DAG from object files rather than from a writer's
//! in-memory graph.
//!
//! Each `Entry` is compared through `common::entry_value` — the ten fields
//! `prodrome-wasm`'s `wire::json_entry` puts on the wire, in the same forms —
//! because that is what a consumer actually reads: `Confidence`'s three-way
//! split collapses to one `unconfirmed` bool there, and a vector that compared
//! the richer Rust value would be pinning a distinction nobody reads.

mod common;

use std::collections::BTreeMap;

use common::vectors::{boolean, each, field, integer, moment, strings, text, vectors};
use prodrome::event::parse_event;
use prodrome::literal::Datetime;
use prodrome::literal::Value;
use prodrome::policy::Untrusted;
use prodrome::view;

type Event = common::Event;

struct Case {
    seed: i64,
    events: Vec<Event>,
    instants: Vec<(Datetime, Vec<Value>)>,
}

fn cases(name: &str) -> (Vec<Case>, Untrusted) {
    let data = vectors(&format!("view/{name}"));
    let untrusted = Untrusted::of(
        strings(&data, "untrusted")
            .iter()
            .map(|actor| prodrome::event::Actor::new(actor.as_str()).expect("a valid actor")),
    );
    let cases = each(&data, "cases")
        .iter()
        .map(|case| {
            let seed = integer(field(case, "seed"));
            // Every event, parsed back through the same closed vocabulary the
            // store reads with, in the tuple's own append order.
            let events: Vec<Event> = each(case, "events")
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    parse_event(text(item))
                        .unwrap_or_else(|e| panic!("seed {seed}: event {i}: {e}"))
                })
                .collect();
            let instants = each(case, "instants")
                .iter()
                .map(|asked| (moment(field(asked, "at")), each(asked, "entries").to_vec()))
                .collect();
            Case {
                seed,
                events,
                instants,
            }
        })
        .collect();
    (cases, untrusted)
}

fn replay(name: &str) -> (usize, usize) {
    let (cases, untrusted) = cases(name);
    assert!(!cases.is_empty(), "{name}: the vector file lost its cases");
    let (mut logs, mut rows) = (0usize, 0usize);
    for case in &cases {
        let nodes = common::chain_of(&case.events);
        for (t, expected) in &case.instants {
            let at = t.isoformat();
            let entries = view::entries(&nodes, *t, &untrusted)
                .unwrap_or_else(|e| panic!("{name} seed {}: entries at {at}: {e}", case.seed));
            let mine = Value::Tuple(entries.iter().map(common::entry_value).collect());
            common::agrees("entries", &mine, &Value::Tuple(expected.clone()))
                .unwrap_or_else(|e| panic!("{name} seed {} at {at}: {e}", case.seed));
            rows += entries.len();
        }
        logs += 1;
    }
    (logs, rows)
}

#[test]
fn every_entry_agrees_under_the_triage_untrusted_policy() {
    let (logs, rows) = replay("triage-untrusted.py");
    assert_eq!(logs, 30, "view/triage-untrusted.py lost a case");
    assert!(rows > 0, "some case has at least one entry");
    println!("view/triage-untrusted.py: {logs} logs, {rows} entry-instants");
}

#[test]
fn every_entry_agrees_under_the_bassel_untrusted_policy() {
    let (logs, rows) = replay("bassel-untrusted.py");
    assert_eq!(logs, 30, "view/bassel-untrusted.py lost a case");
    assert!(rows > 0, "some case has at least one entry");
    println!("view/bassel-untrusted.py: {logs} logs, {rows} entry-instants");
}

/// §9.9 names TWO asymmetries — a claim, and a content record the policy does
/// not confirm — and a suite that never saw either would not be testing what
/// §6.7 is for. Both vector files together must show every `confidence` this
/// crate can print: the roster flips which writes are provisional, so the two
/// policies see the two directions of the same 2:1 split
/// (`tests/common/mod.rs`'s `ACTORS`).
#[test]
fn the_vectors_show_a_claim_and_an_unconfirmed_record_between_them() {
    let mut unconfirmed = BTreeMap::new();
    for name in ["triage-untrusted.py", "bassel-untrusted.py"] {
        let (cases, _) = cases(name);
        let mut any = false;
        for case in &cases {
            for (_, entries) in &case.instants {
                if entries
                    .iter()
                    .any(|entry| boolean(field(entry, "unconfirmed")))
                {
                    any = true;
                }
            }
        }
        unconfirmed.insert(name, any);
    }
    assert_eq!(
        unconfirmed.values().copied().collect::<Vec<_>>(),
        vec![true, true],
        "both policies should provoke at least one unconfirmed entry: {unconfirmed:?}"
    );
}
