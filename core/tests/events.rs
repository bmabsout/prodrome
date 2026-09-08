//! The same stored objects as `tests/literals.rs`, but through the CLOSED
//! vocabulary of §4 and as TYPED events: parsed into an `Envelope`, printed
//! back, and named by `seal_hash`.
//!
//! `tests/literals.rs` proves the grammar round-trips; this proves the KINDS
//! do — that every field the spec declares is read, validated by its `mk_*`,
//! and printed back in declared order. A shipped field this side forgot would
//! show up here as a print that is one field short of its name.

mod common;

use std::collections::BTreeSet;

use prodrome::event::{
    binds, canonical_envelope, parents_of, parse_envelope, seal_hash, Actor, Envelope, TodoEvent,
    EVENT_SIGNATURES,
};
use prodrome::fpl::TERM_SIGNATURES;

fn untrusted() -> BTreeSet<Actor> {
    // The roster is the DEPLOYMENT's, never the engine's (§5): it arrives as a
    // parameter, and this is one.
    [Actor::new("triage").expect("valid")].into_iter().collect()
}

#[test]
fn every_stored_object_is_a_typed_envelope_that_prints_back() {
    let corpus = common::corpus();
    assert!(corpus.len() > 5, "a corpus, not a single object");
    for (name, envelope) in &corpus {
        let text = canonical_envelope(envelope);
        let parsed =
            parse_envelope(&text).unwrap_or_else(|e| panic!("object {}: {e}", name.as_str()));
        assert_eq!(&parsed, envelope, "object {}", name.as_str());
        assert_eq!(canonical_envelope(&parsed), text, "object {}", name.as_str());
        assert_eq!(
            seal_hash(&parsed).as_str(),
            name.as_str(),
            "seal_hash names the object"
        );
    }
}

/// §3's envelope shapes, over the corpus: exactly one object rests on nothing,
/// a `Sealed` has one parent, and a `Woven` has two or more and may carry no
/// event at all — a merge is structure.
#[test]
fn the_corpus_holds_every_envelope_shape() {
    let (mut genesis, mut sealed, mut merges) = (0, 0, 0);
    for (name, envelope) in common::corpus() {
        let parents = parents_of(&envelope).len();
        match &envelope {
            Envelope::Sealed { .. } => {
                assert!(parents <= 1, "object {} is a Sealed", name.as_str());
                if parents == 0 {
                    genesis += 1;
                } else {
                    sealed += 1;
                }
            }
            Envelope::Woven { .. } => {
                assert!(parents >= 2, "a Woven names at least two parents");
                if envelope.event().is_none() {
                    merges += 1;
                }
            }
        }
    }
    assert_eq!(genesis, 1, "exactly one object rests on nothing");
    assert!(sealed > 1 && merges > 0);
}

/// Every kind the corpus uses is a kind SPEC §4 declares, the trust rule (§5)
/// reads each one, and every spec a stored event carries EVALUATES — not just
/// prints back, which is what the closed seam between §4 and §7 buys.
#[test]
fn the_corpus_uses_the_closed_vocabulary_and_every_spec_it_carries_evaluates() {
    let untrusted = untrusted();
    let mut kinds: BTreeSet<&str> = BTreeSet::new();
    let (mut provisional, mut priced) = (0, 0);
    for (_, envelope) in common::corpus() {
        let Some(event) = envelope.event() else {
            continue;
        };
        kinds.insert(event.kind_name());
        if !binds(event, &untrusted) {
            provisional += 1;
        }
        if let TodoEvent::Authored(authored) = event {
            // An `Authored` record binds whatever its actor is (§5).
            assert!(binds(event, &untrusted));
            if let Some(spec) = &authored.spec {
                let now = prodrome::fpl::instant_of(authored.at);
                let value = prodrome::fpl::fulfillment(spec, now, &prodrome::fpl::Env::new());
                assert!((0.0..=1.0).contains(&value), "todo {:?}", authored.todo);
                priced += 1;
            }
        }
    }
    for kind in &kinds {
        assert!(
            EVENT_SIGNATURES.iter().any(|(name, _)| name == kind),
            "{kind} is outside SPEC §4"
        );
    }
    // Every kind §4 declares an EVENT for is exercised; the records
    // (`Source`, `Note`, `SubTodo`) and the envelopes ride inside them.
    for kind in [
        "Created",
        "Completed",
        "Cancelled",
        "Reopened",
        "SpecRevised",
        "Authored",
    ] {
        assert!(kinds.contains(kind), "the corpus is missing a {kind}");
    }
    assert!(provisional > 0, "the corpus exercises the trust rule");
    assert!(priced > 0, "the corpus holds specs, and every one evaluates");
}

/// §4 is a CLOSED vocabulary, so a name outside it is a refusal at the read
/// boundary — where `literal::Open` would have taken the same text.
#[test]
fn a_name_outside_spec_4_is_refused_at_the_envelope() {
    for text in [
        "Sealed(prev='', event=Invented(todo='t', at=datetime(2026, 1, 1, 0, 0, 0)))",
        "Created(todo='t', at=datetime(2026, 1, 1, 0, 0, 0), actor='bassel', text='x', note='')",
        "Sealed(prev='', event=Completed(todo='t', at=datetime(2026, 1, 1, 0, 0, 0)))",
        "Sealed(prev='not a hash', event=None)",
        "Woven(parents=('a',), event=None)",
        "None",
    ] {
        assert!(parse_envelope(text).is_err(), "must refuse {text:?}");
    }
}

/// §2: "The names admitted are the closed vocabulary of §4 and §7 plus
/// `datetime`/`timedelta`." This is the pin on that set.
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
