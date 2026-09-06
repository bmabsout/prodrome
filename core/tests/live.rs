//! The whole chain, end to end, on the REAL store — SPEC §9 on `events/`.
//!
//! `conformance/live.json` is the reference's fold of this box's own history,
//! taken at one recorded instant (`scripts/conformance.py`'s `live_vectors`).
//! It is the case the generated vectors cannot be: 564 objects with a backfill
//! in them, completions dated a year before the records they resolve, 184
//! curves of which 116 are piecewise, and 248 content records whose bodies are
//! real typst source with real escaping in it.
//!
//! Skipped, not failed, where there is no chain: the crate is meant to be
//! usable outside this repository, and the live chain is evidence rather than
//! a dependency. Where the chain HAS moved since the file was written, the
//! `tip` check says so and names regeneration as the fix — a stale vector must
//! be loud, because a fold compared against the wrong history is worse than no
//! comparison at all.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use prodrome::event::{Actor, Envelope, Hash, TodoEvent};
use prodrome::fold::{authored_at, env_at, flatten, history, specs_at, Env, Untrusted};
use prodrome::fpl::{fulfillment, instant_of, iso, print_term};
use prodrome::literal::{parse_literal, print_literal, Datetime, Open, Value};
use prodrome::registers::{conflicts_of, content_of, env_of, fold, nodes_of, specs_of};
use prodrome::store::EventStore;
use serde::Deserialize;

#[derive(Deserialize)]
struct Live {
    at: String,
    tip: String,
    objects: usize,
    untrusted: Vec<String>,
    env: BTreeMap<String, Outcome>,
    specs: BTreeMap<String, String>,
    content: BTreeMap<String, String>,
    flatten: BTreeMap<String, String>,
    conflicts: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

#[derive(Deserialize, PartialEq, Eq, Debug)]
struct Outcome {
    kind: String,
    at: String,
}

fn conformance(name: &str) -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "..", "conformance", name]
        .iter()
        .collect()
}

fn repo_root() -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "..", ".."].iter().collect()
}

/// The generator's `datetime.isoformat()` as a `Datetime`, through the one
/// parser in the crate.
fn moment(text: &str) -> Datetime {
    let (date, time) = text.split_once('T').expect("an isoformat instant");
    let date: Vec<&str> = date.split('-').collect();
    let time: Vec<&str> = time.split(':').collect();
    let (second, micro) = match time[2].split_once('.') {
        Some((s, us)) => (s.to_owned(), format!("{us:0<6}")),
        None => (time[2].to_owned(), "0".to_owned()),
    };
    let call = format!(
        "datetime({}, {}, {}, {}, {}, {}, {})",
        date[0], date[1], date[2], time[0], time[1], second, micro
    );
    match parse_literal(&call, &Open).expect("a datetime literal") {
        Value::Datetime(at) => at,
        other => panic!("{other:?} is not a datetime"),
    }
}

fn outcomes(env: &Env) -> BTreeMap<String, Outcome> {
    env.iter()
        .map(|(todo, binding)| {
            (
                todo.as_str().to_owned(),
                Outcome {
                    kind: binding.kind().to_owned(),
                    at: iso(binding.at()),
                },
            )
        })
        .collect()
}

/// What a live test needs: the vector, the store it describes, and that
/// store's read. Named, because "the reference's answer beside the chain it
/// answered about" is one thing and not three.
struct Chain {
    vector: Live,
    store: EventStore,
    objects: Vec<(Hash, Envelope)>,
}

/// The vector and the store, or `None` when either is absent.
fn live() -> Option<Chain> {
    let events = repo_root().join("events");
    let path = conformance("live.json");
    if !events.is_dir() || !path.is_file() {
        return None;
    }
    let raw = fs::read_to_string(&path).expect("live.json");
    let vector: Live = serde_json::from_str(&raw).expect("live.json is the generator's shape");
    let untrusted = vector
        .untrusted
        .iter()
        .map(|name| Actor::new(name.as_str()).expect("an actor name"))
        .collect();
    let store = EventStore::new(&events, untrusted);
    // The chain grows every hour and the vector was taken at one tip, so read
    // the chain AS OF that tip: objects are never deleted, so the read is the
    // same one the generator made, for as long as the store holds them. A
    // missing tip means the store is not the one the vector describes.
    let tip = Hash::new(vector.tip.as_str()).expect("the vector's tip is a hash");
    let head = store.tip().expect("a live chain has a head");
    assert!(
        head == tip
            || store
                .ancestors(&head)
                .expect("the head's past reads")
                .contains(&tip),
        "live.json's tip is not in this chain's past — regenerate it with \
         `nix develop .#triage -c env PYTHONPATH=. python3 scripts/conformance.py`"
    );
    let objects = store
        .read_dag_at(&BTreeSet::from([tip]))
        .expect("the live chain reads at the vector's tip");
    assert_eq!(
        objects.len(),
        vector.objects,
        "the chain's object count at that tip"
    );
    Some(Chain {
        vector,
        store,
        objects,
    })
}

fn policy(vector: &Live) -> Untrusted {
    Untrusted::of(
        vector
            .untrusted
            .iter()
            .map(|name| Actor::new(name.as_str()).expect("an actor name")),
    )
}

fn events_of(objects: &[(Hash, Envelope)]) -> Vec<TodoEvent> {
    objects
        .iter()
        .filter_map(|(_, envelope)| envelope.event().cloned())
        .collect()
}

#[test]
fn the_live_chain_folds_exactly_as_the_reference_folds_it() {
    let Some(Chain {
        vector,
        store,
        objects,
    }) = live()
    else {
        return;
    };
    let events = events_of(&objects);
    let untrusted = policy(&vector);
    let t = moment(&vector.at);

    // A fold over a store that does not verify says nothing, so the read comes
    // first (§3): every object hashes to its name, every parent exists, no
    // cycle, no unreachable object, no untrusted event dated behind a parent.
    assert_eq!(store.verify(), Vec::<String>::new(), "the chain verifies");

    assert_eq!(
        outcomes(&env_at(&events, t, &untrusted)),
        vector.env,
        "env_at on the live chain"
    );

    let specs: BTreeMap<String, String> = specs_at(&events, t, &untrusted)
        .iter()
        .map(|(todo, spec)| (todo.as_str().to_owned(), print_term(spec)))
        .collect();
    assert_eq!(specs, vector.specs, "specs_at on the live chain");

    let content: BTreeMap<String, String> = authored_at(&events, t)
        .iter()
        .map(|(todo, record)| (todo.as_str().to_owned(), print_literal(&record.to_value())))
        .collect();
    assert_eq!(content, vector.content, "authored_at on the live chain");

    let functions = flatten(&events, t, &untrusted).expect("the live chain folds");
    let printed: BTreeMap<String, String> = functions
        .iter()
        .map(|(todo, term)| (todo.as_str().to_owned(), print_term(term)))
        .collect();
    assert_eq!(printed, vector.flatten, "flatten on the live chain");

    // §9.4 on real history, at the moment the vector pinned.
    assert_eq!(
        history(&events, &untrusted).at(t),
        env_at(&events, t, &untrusted)
    );

    // The corpus is the point: a real chain has backfilled resolutions and
    // revised specs, so most curves have transitions in them.
    let piecewise = printed
        .values()
        .filter(|text| text.starts_with("Piecewise("))
        .count();
    assert!(
        piecewise > 0 && !vector.env.is_empty() && vector.content.len() > vector.specs.len(),
        "the live chain holds resolutions, revisions and content without a price"
    );
}

/// The registers over the real DAG. It is a CHAIN — one head, every object
/// with one parent — so every write descends from the one before it and
/// nothing is in conflict; the register projections must still be the folds,
/// which is what makes "on a chain the registers ARE the folds" a measured
/// claim about this deployment rather than a generated one.
#[test]
fn the_live_chain_has_no_conflicts_and_its_registers_are_its_folds() {
    let Some(Chain {
        vector, objects, ..
    }) = live()
    else {
        return;
    };
    let events = events_of(&objects);
    let untrusted = policy(&vector);
    let t = moment(&vector.at);
    let state = fold(&nodes_of(&objects), None, &untrusted);

    let found: BTreeMap<String, BTreeMap<String, Vec<String>>> = conflicts_of(&state)
        .iter()
        .map(|(todo, by_kind)| {
            (
                todo.as_str().to_owned(),
                by_kind
                    .iter()
                    .map(|(kind, frontier)| {
                        (
                            kind.as_str().to_owned(),
                            frontier
                                .writes()
                                .iter()
                                .map(|write| write.at.as_str().to_owned())
                                .collect(),
                        )
                    })
                    .collect(),
            )
        })
        .collect();
    assert_eq!(found, vector.conflicts, "conflicts on the live chain");

    // The registers fold the WHOLE chain, so they are compared with the dated
    // folds at the same instant the vector pinned — every event on this chain
    // is dated before it.
    assert_eq!(env_of(&state), env_at(&events, t, &untrusted), "env");
    let printed = |specs: BTreeMap<prodrome::event::TodoId, prodrome::fpl::Term>| {
        specs
            .into_iter()
            .map(|(todo, spec)| (todo.into_string(), print_term(&spec)))
            .collect::<BTreeMap<String, String>>()
    };
    assert_eq!(
        printed(specs_of(&state)),
        printed(specs_at(&events, t, &untrusted)),
        "specs"
    );
    assert_eq!(content_of(&state), authored_at(&events, t), "content");
}

/// Every flattened curve on the real chain EVALUATES, at the moment it was
/// folded for, against the environment of that moment. The prints above prove
/// the shapes agree; this proves the shapes are functions — the whole reason
/// the seam was closed, one consumer further out.
#[test]
fn every_live_curve_prices_between_zero_and_one() {
    let Some(Chain {
        vector, objects, ..
    }) = live()
    else {
        return;
    };
    let events = events_of(&objects);
    let untrusted = policy(&vector);
    let t = moment(&vector.at);
    let now = instant_of(t);
    let env = prodrome::fold::evaluation_env(&env_at(&events, t, &untrusted));
    let functions = flatten(&events, t, &untrusted).expect("the live chain folds");
    assert!(functions.len() > 100, "a real corpus of curves");
    let mut resolved = 0;
    for (todo, term) in &functions {
        let value = fulfillment(term, now, &env);
        assert!(
            (0.0..=1.0).contains(&value),
            "{:?} prices at {value}",
            todo.as_str()
        );
        // A resolved todo reads exactly 1.0 from the moment it was resolved.
        if env.contains_key(todo.as_str()) {
            assert_eq!(value, 1.0, "{:?} is resolved", todo.as_str());
            resolved += 1;
        }
    }
    assert!(resolved > 0, "the live chain holds resolved todos");
}
