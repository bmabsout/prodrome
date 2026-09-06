//! SPEC §9 law 8 for §6.1–6.5, on `conformance/folds.json`: the five causal
//! folds, reproduced EXACTLY.
//!
//! Each vector is a random log the reference generated, a moment `t`, and what
//! each fold answered there. The comparison is byte-exact throughout — `env`
//! and `history_at` by kind and instant, `specs`, `content` and `flatten` by
//! their canonical prints — because a fold that agrees to nine decimals but
//! prints a different term is not the same fold. `flatten`'s print in
//! particular carries `mk_piecewise`'s normal form, so a splice or an adjacent
//! repeat this side missed shows up as a longer string and not as a number.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use prodrome::event::{canonical, parse_envelope, Actor, Envelope, TodoEvent};
use prodrome::fold::{authored_at, env_at, flatten, history, specs_at, Binding, Env, Untrusted};
use prodrome::fpl::{iso, print_term};
use prodrome::literal::{parse_literal, Datetime, Value};
use serde::Deserialize;

#[derive(Deserialize)]
struct Vectors {
    logs: Vec<Log>,
}

#[derive(Deserialize)]
struct Log {
    seed: u32,
    untrusted: Vec<String>,
    events: Vec<String>,
    at: String,
    env: BTreeMap<String, Outcome>,
    specs: BTreeMap<String, String>,
    content: BTreeMap<String, String>,
    flatten: BTreeMap<String, String>,
    history_at: BTreeMap<String, Outcome>,
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

/// The generator prints each event on its own; reading one back means wrapping
/// it in the envelope the loader knows, which is also the honest shape — an
/// event only ever reaches a fold out of a stored object.
fn events_of(log: &Log) -> Vec<TodoEvent> {
    log.events
        .iter()
        .map(|text| {
            let object = format!("Sealed(prev='', event={text})");
            match parse_envelope(&object).unwrap_or_else(|e| panic!("seed {}: {e}", log.seed)) {
                Envelope::Sealed { event, .. } => event,
                Envelope::Woven { .. } => unreachable!("the wrapper is a Sealed"),
            }
        })
        .collect()
}

fn moment(text: &str) -> Datetime {
    // The generator writes `datetime.isoformat()`; the grammar reads a
    // `datetime(...)` call, so the one parser in the crate does the work.
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
    match parse_literal(&call, &prodrome::literal::Open).expect("a datetime literal") {
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

fn policy(names: &[String]) -> Untrusted {
    Untrusted::of(
        names
            .iter()
            .map(|name| Actor::new(name.as_str()).expect("an actor name")),
    )
}

#[test]
fn every_log_folds_to_the_references_env_specs_content_flatten_and_history() {
    let raw = fs::read_to_string(conformance("folds.json")).expect("folds.json");
    let vectors: Vectors = serde_json::from_str(&raw).expect("folds.json is the generator's shape");
    assert!(!vectors.logs.is_empty());
    let mut flattened = 0;
    let mut piecewise = 0;
    let mut resolved = 0;
    for log in &vectors.logs {
        let events = events_of(log);
        let untrusted = policy(&log.untrusted);
        let t = moment(&log.at);
        let seed = log.seed;

        let env = env_at(&events, t, &untrusted);
        assert_eq!(outcomes(&env), log.env, "seed {seed}: env_at");
        resolved += env.len();

        let specs: BTreeMap<String, String> = specs_at(&events, t, &untrusted)
            .iter()
            .map(|(todo, spec)| (todo.as_str().to_owned(), print_term(spec)))
            .collect();
        assert_eq!(specs, log.specs, "seed {seed}: specs_at");

        let content: BTreeMap<String, String> = authored_at(&events, t)
            .iter()
            .map(|(todo, record)| {
                (
                    todo.as_str().to_owned(),
                    prodrome::literal::print_literal(&record.to_value()),
                )
            })
            .collect();
        assert_eq!(content, log.content, "seed {seed}: authored_at");

        let functions = flatten(&events, t, &untrusted).expect("the log folds");
        let printed: BTreeMap<String, String> = functions
            .iter()
            .map(|(todo, term)| (todo.as_str().to_owned(), print_term(term)))
            .collect();
        assert_eq!(printed, log.flatten, "seed {seed}: flatten");
        flattened += functions.len();
        piecewise += printed
            .values()
            .filter(|text| text.starts_with("Piecewise("))
            .count();

        // §9.4: the history is the environment as a function of time, and at
        // this moment it IS the environment.
        let past = history(&events, &untrusted);
        assert_eq!(
            outcomes(&past.at(t)),
            log.history_at,
            "seed {seed}: history"
        );
        assert_eq!(past.at(t), env, "seed {seed}: history.at == env_at");
    }
    // The corpus exercises what it is for. A zero here would mean the vectors
    // stopped covering a case and this file went quietly green.
    assert!(flattened > 0, "some logs flatten to a function");
    assert!(piecewise > 0, "some functions have transitions");
    assert!(resolved > 0, "some todos are resolved at the moment asked");
}

/// The one ordering (§6): every fold reads `chronological`, so an event dated
/// after the moment is invisible to all five — and the print of an event is
/// what the reference memoises for the same reason, so it is checked here too.
#[test]
fn a_later_event_is_invisible_and_every_event_prints_as_the_reference_wrote_it() {
    let raw = fs::read_to_string(conformance("folds.json")).expect("folds.json");
    let vectors: Vectors = serde_json::from_str(&raw).expect("folds.json is the generator's shape");
    let mut dropped = 0;
    for log in &vectors.logs {
        let events = events_of(log);
        for (event, text) in events.iter().zip(&log.events) {
            assert_eq!(&canonical(event), text, "seed {}: event print", log.seed);
        }
        let t = moment(&log.at);
        let untrusted = policy(&log.untrusted);
        let known: Vec<&TodoEvent> = prodrome::fold::chronological(&events, t).collect();
        assert!(known.len() <= events.len());
        dropped += events.len() - known.len();
        // Folding only what was known by `t` is the same as folding everything
        // and letting `chronological` do the dropping.
        let only_known: Vec<TodoEvent> = known.into_iter().cloned().collect();
        assert_eq!(
            env_at(&only_known, t, &untrusted),
            env_at(&events, t, &untrusted),
            "seed {}",
            log.seed
        );
    }
    assert!(
        dropped > 0,
        "the vectors hold events dated after their moment"
    );
}

/// §9.4 across the whole corpus, at instants the generator never asked about:
/// `history.at(t)` is `env_at(·, t)` for every `t`, not only for the one the
/// vector pinned.
#[test]
fn history_equals_env_at_at_every_instant_a_log_mentions() {
    let raw = fs::read_to_string(conformance("folds.json")).expect("folds.json");
    let vectors: Vectors = serde_json::from_str(&raw).expect("folds.json is the generator's shape");
    let mut asked = 0;
    for log in &vectors.logs {
        let events = events_of(log);
        let untrusted = policy(&log.untrusted);
        let past = history(&events, &untrusted);
        // Every instant the log names, plus the moment the vector pinned: the
        // transitions are where the two could differ, so they are what to ask.
        let instants: BTreeSet<Datetime> = events
            .iter()
            .map(TodoEvent::at)
            .chain(std::iter::once(moment(&log.at)))
            .collect();
        for t in instants {
            assert_eq!(
                past.at(t),
                env_at(&events, t, &untrusted),
                "seed {}: at {}",
                log.seed,
                iso(prodrome::fpl::instant_of(t))
            );
            asked += 1;
        }
    }
    assert!(asked > vectors.logs.len(), "more instants than logs");
}

/// The `Binding` ADT is not a boolean: a completion and a cancellation are
/// different facts, and the vectors hold both.
#[test]
fn the_corpus_holds_both_outcomes() {
    let raw = fs::read_to_string(conformance("folds.json")).expect("folds.json");
    let vectors: Vectors = serde_json::from_str(&raw).expect("folds.json is the generator's shape");
    let mut kinds: BTreeSet<&'static str> = BTreeSet::new();
    for log in &vectors.logs {
        let events = events_of(log);
        let env = env_at(&events, moment(&log.at), &policy(&log.untrusted));
        for binding in env.values() {
            kinds.insert(match binding {
                Binding::Completed(_) => "Completed",
                Binding::Cancelled(_) => "Cancelled",
            });
        }
    }
    assert_eq!(
        kinds.into_iter().collect::<Vec<_>>(),
        ["Cancelled", "Completed"]
    );
}
