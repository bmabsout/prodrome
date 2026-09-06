//! The same 564 stored objects as `tests/literals.rs`, but through the CLOSED
//! vocabulary of §4 and as TYPED events: parsed into an `Envelope`, printed
//! back, and named by `seal_hash`.
//!
//! `tests/literals.rs` proves the grammar round-trips; this proves the KINDS
//! do — that every field the reference declared is read, validated by its
//! `mk_*`, and printed back in declared order. A shipped field this side
//! forgot would show up here as a print that is one field short of its name.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use prodrome::event::{
    binds, canonical_envelope, parents_of, parse_envelope, seal_hash, Envelope, TodoEvent,
    EVENT_SIGNATURES,
};
use prodrome::fpl::TERM_SIGNATURES;
use serde::Deserialize;

#[derive(Deserialize)]
struct Vectors {
    objects: Vec<Object>,
}

#[derive(Deserialize)]
struct Object {
    name: String,
    text: String,
}

fn objects() -> Vec<Object> {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "conformance",
        "literals.json",
    ]
    .iter()
    .collect();
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let vectors: Vectors =
        serde_json::from_str(&raw).expect("literals.json is the generator's shape");
    vectors.objects
}

#[test]
fn every_stored_object_is_a_typed_envelope_that_prints_back() {
    let objects = objects();
    assert!(objects.len() > 500, "the live chain, not a fragment");
    for object in &objects {
        let envelope =
            parse_envelope(&object.text).unwrap_or_else(|e| panic!("object {}: {e}", object.name));
        assert_eq!(
            canonical_envelope(&envelope),
            object.text,
            "object {}",
            object.name
        );
        assert_eq!(
            seal_hash(&envelope).as_str(),
            object.name,
            "seal_hash names the object"
        );
    }
}

/// The chain this box holds is the DAG in which every object has exactly one
/// parent — genesis alone has none.
#[test]
fn the_live_chain_is_a_chain() {
    let objects = objects();
    let mut genesis = 0;
    for object in &objects {
        let envelope = parse_envelope(&object.text).expect("parses");
        match parents_of(&envelope).len() {
            0 => genesis += 1,
            1 => {}
            more => panic!("object {} has {more} parents", object.name),
        }
        assert!(matches!(envelope, Envelope::Sealed { .. }));
    }
    assert_eq!(genesis, 1, "exactly one object rests on nothing");
}

/// Every kind the live chain uses, and the trust rule over it. The counts are
/// evidence about the corpus, not a target: what is pinned is that each kind
/// present parses as itself.
#[test]
fn the_live_chain_uses_the_closed_vocabulary_and_nothing_else() {
    let untrusted: BTreeSet<prodrome::event::Actor> = ["triage"]
        .into_iter()
        .map(|actor| prodrome::event::Actor::new(actor).expect("valid"))
        .collect();
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    let mut provisional = 0;
    let mut priced = 0;
    for object in objects() {
        let envelope = parse_envelope(&object.text).expect("parses");
        if let Some(event) = envelope.event() {
            *kinds.entry(event.kind_name()).or_default() += 1;
            if !binds(event, &untrusted) {
                provisional += 1;
            }
            // Since the seam closed, a stored spec IS an `fpl::Term`: it has
            // been through §7's smart constructors, so it can be evaluated.
            // The placeholder it replaced could only promise it printed back.
            if let TodoEvent::Authored(authored) = event {
                if let Some(spec) = &authored.spec {
                    let now = prodrome::fpl::instant_of(authored.at);
                    let value = prodrome::fpl::fulfillment(spec, now, &prodrome::fpl::Env::new());
                    assert!((0.0..=1.0).contains(&value), "todo {:?}", authored.todo);
                    priced += 1;
                }
            }
            // An `Authored` record binds whatever its actor is (§5).
            if matches!(event, TodoEvent::Authored(_)) {
                assert!(binds(event, &untrusted));
            }
        }
    }
    assert!(
        kinds.contains_key("Authored"),
        "the chain holds content since 0.10"
    );
    for kind in kinds.keys() {
        assert!(
            EVENT_SIGNATURES.iter().any(|(name, _)| name == kind),
            "{kind} is outside SPEC §4"
        );
    }
    // The corpus does hold provisional events; a count of zero would mean the
    // trust rule was never exercised by this test.
    assert!(
        provisional > 0,
        "the live chain holds triage-actor lifecycle events"
    );
    assert!(
        priced > 0,
        "the live chain holds specs, and every one of them evaluates"
    );
}

/// §2: "The names admitted are the closed vocabulary of §4 and §7 plus
/// `datetime`/`timedelta`." The reference pins the set in
/// `tests/test_contracts.py`; this is the same pin.
#[test]
fn the_vocabulary_is_exactly_the_spec_s() {
    let mut names: Vec<&str> = TERM_SIGNATURES
        .iter()
        .chain(EVENT_SIGNATURES.iter())
        .map(|(name, _)| *name)
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "After",
            "Authored",
            "Cancelled",
            "Completed",
            "Conj",
            "Created",
            "Curve",
            "CurvePoint",
            "Decay",
            "Flat",
            "Gate",
            "Importance",
            "Note",
            "Offset",
            "OffsetBy",
            "Piece",
            "Piecewise",
            "Reopened",
            "Sealed",
            "Shift",
            "Source",
            "SpecRevised",
            "SubTodo",
            "Within",
            "Woven",
        ]
    );
}
