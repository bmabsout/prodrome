//! SPEC §6.7 and §9.9 over `conformance/view.json`, and on the live chain.
//!
//! `view.json` is the LAST derivation in `prodrome/conformance/`: it was
//! generated from `suzatary/view.py::entries_of` while the composition was
//! still Python, before `prodrome::view` existed. So this file is a genuine
//! differential — a second implementation reproducing a first one's answers —
//! and not a re-statement, which is what the rest of the suite has been since
//! the Python evaluator was deleted.
//!
//! Two subjects, for the reason `live.rs` gives: forty generated DAGs are
//! where the FORKS are (concurrent writes, conflicts a chain never has, todos
//! whose only events are dated after the moment asked about), and the live
//! chain is where the SIZE and the history are (564 objects, a backfill,
//! revisions six months after the record they revise).
//!
//! WHAT IS COMPARED is the row `entry_json` writes, field for field: `state`,
//! `at` and `claimed` as the strings the wire carries, `value` to 1e-9,
//! `unconfirmed`, `conflicts`, the winning content object's NAME, the todo's
//! §6.4 function as its canonical PRINT, and the stream. A print rather than
//! `to_json` because a term's print is its identity (§3), the same choice
//! `folds.json` and `live.json` already make for `flatten`.
//!
//! THE LAW (§9.9) IS NOT HERE, deliberately: `fold_laws.rs` already holds the
//! generators and the two-replica `diverged` that §9.6 is stated over, and the
//! view law is that same statement one composition further out. A second copy
//! of those generators would be the duplication this stage exists to remove.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use prodrome::event::{Actor, Hash};
use prodrome::fold::Untrusted;
use prodrome::fpl::print_term;
use prodrome::literal::{parse_literal, Datetime, Open, Value as Literal};
use prodrome::registers::nodes_of;
use prodrome::store::EventStore;
use prodrome::view::{entries, Entry};
use serde_json::{json, Map, Value};

/// The generator's `datetime.isoformat()` as a `Datetime`, through the one
/// parser in the crate — `live.rs`'s helper, for the same input.
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
        Literal::Datetime(at) => at,
        other => panic!("{other:?} is not a datetime"),
    }
}

fn policy(names: &Value) -> Untrusted {
    Untrusted::of(
        names
            .as_array()
            .expect("untrusted is a list")
            .iter()
            .map(|name| Actor::new(name.as_str().expect("an actor name")).expect("a valid actor")),
    )
}

/// One entry as `scripts/conformance.py::entry_json` writes it.
fn row(entry: &Entry) -> Value {
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
                        Value::Array(writes.iter().map(|w| Value::String(w.as_str().to_owned())).collect()),
                    )
                })
                .collect::<Map<String, Value>>(),
        ),
        "content": entry.content.as_ref().map(Hash::as_str),
        "spec": entry.spec().map(print_term),
        "stream": Value::Array(
            entry.stream.iter().map(|name| Value::String(name.as_str().to_owned())).collect(),
        ),
    })
}

fn rows(entries: &[Entry]) -> Value {
    Value::Array(entries.iter().map(row).collect())
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// The vector's objects, back on disk under their own names — `registers.rs`'s
/// `materialise`, for the same reason: nothing about an entry may depend on
/// this side having been the writer.
fn materialise(view: &Value) -> EventStore {
    let root = std::env::temp_dir().join(format!(
        "prodrome-view-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("objects")).expect("creates the store");
    for (name, text) in view["objects"].as_object().expect("objects is a map") {
        fs::write(
            root.join("objects").join(format!("{name}.py")),
            text.as_str().expect("an object's print"),
        )
        .expect("writes an object");
    }
    let tips: Vec<&str> = view["tips"]
        .as_array()
        .expect("tips is a list")
        .iter()
        .map(|tip| tip.as_str().expect("a tip is a name"))
        .collect();
    if tips.len() > 1 {
        fs::create_dir_all(root.join("refs")).expect("creates refs/");
        for tip in &tips {
            fs::write(root.join("refs").join(tip), tip).expect("writes a ref");
        }
    }
    fs::write(root.join("HEAD"), tips[0]).expect("writes HEAD");
    EventStore::new(
        root,
        [Actor::new("triage").expect("valid")].into_iter().collect(),
    )
}

#[test]
fn every_view_vector_is_reproduced() {
    let data = common::vectors("view.json");
    let views = data["views"].as_array().expect("view.json has views");
    assert!(!views.is_empty(), "the vector file lost its DAGs");
    let (mut cases, mut counted, mut claimed, mut conflicted, mut priced) = (0, 0, 0, 0, 0);
    for view in views {
        let seed = &view["seed"];
        let store = materialise(view);
        let objects = store
            .read_dag_named()
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let nodes = nodes_of(&objects);
        for case in view["cases"].as_array().expect("cases is a list") {
            let at = moment(case["at"].as_str().expect("at is an instant"));
            let mine = entries(&nodes, at, &policy(&case["untrusted"]))
                .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            common::agrees("entries", &rows(&mine), &case["entries"]).unwrap_or_else(|e| {
                panic!("seed {seed} at {} ({}): {e}", case["at"], case["untrusted"])
            });
            cases += 1;
            counted += mine.len();
            claimed += mine.iter().filter(|e| e.claim.is_some()).count();
            conflicted += mine.iter().filter(|e| !e.conflicts.is_empty()).count();
            priced += mine.iter().filter(|e| e.priced.is_some()).count();
        }
        let _ = fs::remove_dir_all(store.root());
    }
    // The corpus is the point, so it is counted: a file that stopped holding
    // claims, conflicts or prices would go quietly green on the easy half.
    assert!(claimed > 0, "some entry carries a refused claim");
    assert!(conflicted > 0, "some entry carries a conflicted register");
    assert!(priced > 0, "some entry carries a price");
    println!(
        "view.json: {cases} cases over {} DAGs, {counted} entries \
         ({claimed} claimed, {conflicted} conflicted, {priced} priced); \
         largest float deviation {:e}",
        views.len(),
        common::worst_seen(),
    );
}

/// §6.7 on the real store, at the instant the vector was taken and AS OF the
/// tip it was taken over — `live.rs`'s discipline, because the chain grows and
/// a composition compared against the wrong history is worse than no
/// comparison. Skipped, not failed, where there is no chain: the crate is
/// meant to be usable outside this repository.
#[test]
fn the_live_chain_reads_as_the_reference_read_it() {
    let data = common::vectors("view.json");
    let Some(live) = data.get("live").filter(|value| !value.is_null()) else {
        return;
    };
    let events: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", "events"]
        .iter()
        .collect();
    if !events.is_dir() {
        return;
    }
    let untrusted = live["untrusted"]
        .as_array()
        .expect("untrusted is a list")
        .iter()
        .map(|name| Actor::new(name.as_str().expect("an actor name")).expect("valid"))
        .collect();
    let store = EventStore::new(&events, untrusted);
    let tip = Hash::new(live["tip"].as_str().expect("tip is a name")).expect("a hash");
    let head = store.tip().expect("a live chain has a head");
    assert!(
        head == tip
            || store
                .ancestors(&head)
                .expect("the head's past reads")
                .contains(&tip),
        "view.json's tip is not in this chain's past — regenerate it with \
         `nix develop .#triage -c env PYTHONPATH=. python3 scripts/conformance.py`"
    );
    let objects = store
        .read_dag_at(&BTreeSet::from([tip]))
        .expect("the live chain reads at the vector's tip");
    assert_eq!(
        objects.len(),
        live["objects"].as_u64().expect("an object count") as usize,
        "the chain's object count at that tip"
    );
    let at = moment(live["at"].as_str().expect("at is an instant"));
    let mine =
        entries(&nodes_of(&objects), at, &policy(&live["untrusted"])).expect("the chain folds");
    common::agrees("entries", &rows(&mine), &live["entries"]).expect("the live chain's entries");

    // A real corpus, asserted so the comparison cannot pass on an empty one.
    assert!(mine.len() > 100, "the live chain holds a corpus of todos");
    let resolved = mine.iter().filter(|e| e.outcome.is_some()).count();
    let unpriced = mine.iter().filter(|e| e.priced.is_none()).count();
    assert!(
        resolved > 0 && unpriced > 0,
        "the live chain holds resolved todos and todos it has no price for"
    );
    let by_id: BTreeMap<&str, &Entry> = mine.iter().map(|e| (e.todo.as_str(), e)).collect();
    assert_eq!(by_id.len(), mine.len(), "one row per todo");
    println!(
        "view.json live: {} entries, {resolved} resolved, {unpriced} unpriced",
        mine.len()
    );
}
