//! SPEC §9.9 (§6.7) over `conformance/view/*.json`: every field of every
//! `view::entries` row, replayed under the two reference policies the vectors
//! were taken with — their `untrusted` arrays, read through `policy::Untrusted`.
//!
//! THE VECTORS ARE FROZEN. Nothing in this file regenerates them — a chain
//! this test cannot rebuild byte-for-byte is a chain it is supposed to fail
//! on, not paper over. Regeneration is `cargo run --example
//! generate_view_vectors -p prodrome-core`, run by hand; CI never runs
//! `cargo run --example` and this test never calls it either, which is what
//! makes `conformance/view/*.json` evidence and not a cache. See that
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
//! Each `Entry` is compared through `common::json_entry` — the same
//! rendering `prodrome-wasm`'s `wire::json_entry` ships — because that is
//! what the wire actually carries: `Confidence`'s three-way split collapses to
//! one `unconfirmed` bool on the wire, and a vector that compared the richer
//! Rust value would be pinning a distinction no consumer ever reads.

mod common;

use std::collections::BTreeMap;

use prodrome::event::parse_event;
use prodrome::literal::Datetime;
use prodrome::policy::Untrusted;
use prodrome::view;
use serde_json::Value;

type Event = common::Event;

struct Case {
    seed: u64,
    events: Vec<Event>,
    instants: Vec<(String, Value)>,
}

fn cases(name: &str) -> (Vec<Case>, Untrusted) {
    let data = common::vectors(&format!("view/{name}"));
    let untrusted = Untrusted::of(
        data["untrusted"]
            .as_array()
            .expect("untrusted is an array")
            .iter()
            .map(|actor| {
                prodrome::event::Actor::new(actor.as_str().expect("an actor name"))
                    .expect("a valid actor")
            }),
    );
    let cases = data["cases"]
        .as_array()
        .expect("cases is an array")
        .iter()
        .map(|case| {
            let seed = case["seed"].as_u64().expect("seed is a number");
            // Every event, parsed back through the same closed vocabulary the
            // store reads with, in the array's own append order.
            let events: Vec<Event> = case["events"]
                .as_array()
                .expect("events is an array")
                .iter()
                .enumerate()
                .map(|(i, text)| {
                    let text = text.as_str().expect("an event print is a string");
                    parse_event(text).unwrap_or_else(|e| panic!("seed {seed}: event {i}: {e}"))
                })
                .collect();
            let instants = case["instants"]
                .as_array()
                .expect("instants is an array")
                .iter()
                .map(|instant| {
                    (
                        instant["at"].as_str().expect("at is a string").to_owned(),
                        instant["entries"].clone(),
                    )
                })
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

fn parse_at(s: &str) -> Datetime {
    let iso = s.replacen(' ', "T", 1);
    prodrome::fpl::datetime_of(
        prodrome::fpl::parse_iso(&iso).unwrap_or_else(|e| panic!("bad instant {s:?}: {e}")),
    )
    .unwrap_or_else(|e| panic!("bad instant {s:?}: {e}"))
}

fn replay(name: &str) -> (usize, usize) {
    let (cases, untrusted) = cases(name);
    assert!(!cases.is_empty(), "{name}: the vector file lost its cases");
    let (mut logs, mut rows) = (0usize, 0usize);
    for case in &cases {
        let nodes = common::chain_of(&case.events);
        for (at, expected_entries) in &case.instants {
            let t = parse_at(at);
            let entries = view::entries(&nodes, t, &untrusted)
                .unwrap_or_else(|e| panic!("{name} seed {}: entries at {at}: {e}", case.seed));
            let mine = Value::Array(entries.iter().map(common::json_entry).collect());
            common::agrees("entries", &mine, expected_entries)
                .unwrap_or_else(|e| panic!("{name} seed {} at {at}: {e}", case.seed));
            rows += entries.len();
        }
        logs += 1;
    }
    (logs, rows)
}

#[test]
fn every_entry_agrees_under_the_triage_untrusted_policy() {
    let (logs, rows) = replay("triage-untrusted.json");
    assert_eq!(logs, 30, "view/triage-untrusted.json lost a case");
    assert!(rows > 0, "some case has at least one entry");
    println!("view/triage-untrusted.json: {logs} logs, {rows} entry-instants");
}

#[test]
fn every_entry_agrees_under_the_bassel_untrusted_policy() {
    let (logs, rows) = replay("bassel-untrusted.json");
    assert_eq!(logs, 30, "view/bassel-untrusted.json lost a case");
    assert!(rows > 0, "some case has at least one entry");
    println!("view/bassel-untrusted.json: {logs} logs, {rows} entry-instants");
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
    for name in ["triage-untrusted.json", "bassel-untrusted.json"] {
        let (cases, _) = cases(name);
        let mut any = false;
        for case in &cases {
            for (_, entries) in &case.instants {
                if entries
                    .as_array()
                    .expect("entries is an array")
                    .iter()
                    .any(|entry| entry["unconfirmed"].as_bool() == Some(true))
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
