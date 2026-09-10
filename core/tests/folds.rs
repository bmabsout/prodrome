//! SPEC §9 law 8 for §6.1–6.5, on `conformance/folds.py`: the five causal
//! folds, reproduced EXACTLY.
//!
//! Each vector is a random log the reference generated, a moment `t`, and what
//! each fold answered there. The comparison is byte-exact throughout — `env`
//! and `history_at` by kind and instant, `specs`, `content` and `flatten` by
//! their canonical prints — because a fold that agrees to nine decimals but
//! prints a different term is not the same fold. `flatten`'s print in
//! particular carries `mk_piecewise`'s normal form, so a splice or an adjacent
//! repeat this side missed shows up as a longer string and not as a number.
//!
//! The vector file is itself a literal of §2's grammar (0.4), read by the same
//! parser the events inside it are read with — `tests/common/vectors.rs`.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::vectors::{each, field, integer, moment, strings, text, text_at, vectors};
use prodrome::event::{canonical, parse_envelope, Actor, Envelope, TodoEvent};
use prodrome::fold::{authored_at, env_at, flatten, history, specs_at, Binding, Env};
use prodrome::fpl::{instant_of, iso, print_term};
use prodrome::literal::{print_literal, Datetime, Value};
use prodrome::policy::Untrusted;
use prodrome::reference::Todo;

/// The vectors' `Authored(...)` prints are the reference payload's, so that is
/// what they are parsed back as — §9.1 under the generic path.
type Event = TodoEvent<Todo>;

/// The generator prints each event on its own; reading one back means wrapping
/// it in the envelope the loader knows, which is also the honest shape — an
/// event only ever reaches a fold out of a stored object.
fn events_of(log: &Value) -> Vec<Event> {
    let seed = integer(field(log, "seed"));
    each(log, "events")
        .iter()
        .map(|item| {
            let object = format!("Sealed(prev='', event={})", text(item));
            match parse_envelope::<Todo>(&object).unwrap_or_else(|e| panic!("seed {seed}: {e}")) {
                Envelope::Sealed { event, .. } => event,
                Envelope::Woven { .. } => unreachable!("the wrapper is a Sealed"),
            }
        })
        .collect()
}

/// An environment as the vectors hold it: `(todo, kind, instant)`, ordered by
/// todo, so a comparison is one equality and names what differs.
type Outcomes = BTreeMap<String, (String, String)>;

fn outcomes(env: &Env) -> Outcomes {
    env.iter()
        .map(|(todo, binding)| {
            (
                todo.as_str().to_owned(),
                (binding.kind().to_owned(), iso(binding.at())),
            )
        })
        .collect()
}

fn frozen_outcomes(log: &Value, key: &str) -> Outcomes {
    each(log, key)
        .iter()
        .map(|bound| {
            (
                text_at(bound, "todo").to_owned(),
                (
                    text_at(bound, "kind").to_owned(),
                    iso(instant_of(moment(field(bound, "at")))),
                ),
            )
        })
        .collect()
}

/// A `(todo, print)` fold — `specs`, `content`, `flatten` — as the vector holds
/// it, whichever field name that file gives the print.
fn frozen_prints(log: &Value, key: &str, field_name: &str) -> BTreeMap<String, String> {
    each(log, key)
        .iter()
        .map(|item| {
            (
                text_at(item, "todo").to_owned(),
                text_at(item, field_name).to_owned(),
            )
        })
        .collect()
}

fn policy(log: &Value) -> Untrusted {
    Untrusted::of(
        strings(log, "untrusted")
            .iter()
            .map(|name| Actor::new(name.as_str()).expect("an actor name")),
    )
}

fn logs() -> Vec<Value> {
    each(&vectors("folds.py"), "logs").to_vec()
}

#[test]
fn every_log_folds_to_the_references_env_specs_content_flatten_and_history() {
    let logs = logs();
    assert!(!logs.is_empty());
    let mut flattened = 0;
    let mut piecewise = 0;
    let mut resolved = 0;
    for log in &logs {
        let events = events_of(log);
        let untrusted = policy(log);
        let t = moment(field(log, "at"));
        let seed = integer(field(log, "seed"));

        let env = env_at(&events, t, &untrusted);
        assert_eq!(
            outcomes(&env),
            frozen_outcomes(log, "env"),
            "seed {seed}: env_at"
        );
        resolved += env.len();

        let specs: BTreeMap<String, String> = specs_at(&events, t, &untrusted)
            .iter()
            .map(|(todo, spec)| (todo.as_str().to_owned(), print_term(spec)))
            .collect();
        assert_eq!(
            specs,
            frozen_prints(log, "specs", "term"),
            "seed {seed}: specs_at"
        );

        let content: BTreeMap<String, String> = authored_at(&events, t)
            .iter()
            .map(|(todo, record)| (todo.as_str().to_owned(), print_literal(&record.to_value())))
            .collect();
        assert_eq!(
            content,
            frozen_prints(log, "content", "event"),
            "seed {seed}: authored_at"
        );

        let functions = flatten(&events, t, &untrusted).expect("the log folds");
        let printed: BTreeMap<String, String> = functions
            .iter()
            .map(|(todo, term)| (todo.as_str().to_owned(), print_term(term)))
            .collect();
        assert_eq!(
            printed,
            frozen_prints(log, "flatten", "term"),
            "seed {seed}: flatten"
        );
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
            frozen_outcomes(log, "history_at"),
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
    let mut dropped = 0;
    for log in &logs() {
        let seed = integer(field(log, "seed"));
        let events = events_of(log);
        for (event, item) in events.iter().zip(each(log, "events")) {
            assert_eq!(canonical(event), text(item), "seed {seed}: event print");
        }
        let t = moment(field(log, "at"));
        let untrusted = policy(log);
        let known: Vec<&Event> = prodrome::fold::chronological(&events, t).collect();
        assert!(known.len() <= events.len());
        dropped += events.len() - known.len();
        // Folding only what was known by `t` is the same as folding everything
        // and letting `chronological` do the dropping.
        let only_known: Vec<Event> = known.into_iter().cloned().collect();
        assert_eq!(
            env_at(&only_known, t, &untrusted),
            env_at(&events, t, &untrusted),
            "seed {seed}"
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
    let logs = logs();
    let mut asked = 0;
    for log in &logs {
        let seed = integer(field(log, "seed"));
        let events = events_of(log);
        let untrusted = policy(log);
        let past = history(&events, &untrusted);
        // Every instant the log names, plus the moment the vector pinned: the
        // transitions are where the two could differ, so they are what to ask.
        let instants: BTreeSet<Datetime> = events
            .iter()
            .map(TodoEvent::at)
            .chain(std::iter::once(moment(field(log, "at"))))
            .collect();
        for t in instants {
            assert_eq!(
                past.at(t),
                env_at(&events, t, &untrusted),
                "seed {seed}: at {}",
                iso(instant_of(t))
            );
            asked += 1;
        }
    }
    assert!(asked > logs.len(), "more instants than logs");
}

/// SPEC §9.1 ON THE GENERIC PATH. The vectors' record prints were taken with
/// the REFERENCE PAYLOAD, and this is the law that says so out loud: every
/// `Authored(...)` in `folds.py` parses under `reference::Todo` — through the
/// vocabulary that is now the core's names UNION the payload's — and prints
/// back BYTE FOR BYTE. Making the record kind a type parameter changed the
/// types and not one byte of the format, and a regression would show up here as
/// a print that differs from the text it came from.
#[test]
fn every_record_print_round_trips_byte_for_byte_under_the_reference_payload() {
    let mut records = 0;
    for log in &logs() {
        let seed = integer(field(log, "seed"));
        for (event, item) in events_of(log).iter().zip(each(log, "events")) {
            if !matches!(event, TodoEvent::Authored(_)) {
                continue;
            }
            assert_eq!(canonical(event), text(item), "seed {seed}: record print");
            records += 1;
        }
    }
    assert!(
        records > 100,
        "the vectors hold records to round-trip, not a handful"
    );
}

/// The `Binding` ADT is not a boolean: a completion and a cancellation are
/// different facts, and the vectors hold both.
#[test]
fn the_corpus_holds_both_outcomes() {
    let mut kinds: BTreeSet<&'static str> = BTreeSet::new();
    for log in &logs() {
        let events = events_of(log);
        let env = env_at(&events, moment(field(log, "at")), &policy(log));
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
