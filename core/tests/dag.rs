//! SPEC §9 law 8 for §3, on `conformance/dag.py`: linearisations, tips,
//! parents and `verify` findings EXACTLY.
//!
//! Each vector is a store the reference built by appending, adopting another
//! replica's objects, and sometimes merging — so the DAGs have two heads,
//! merges, and triage-actor events dated behind what they rest on. Each is
//! rebuilt here by writing the object files and the heads, which is the honest
//! way to test a READER: nothing about the order or the findings may depend on
//! this side having been the writer.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::vectors::{each, field, integer, strings, text_at, vectors};
use prodrome::event::{parents_of, seal_hash, Actor, Hash};
use prodrome::literal::Value;
use prodrome::policy::Untrusted;
use prodrome::reference::Todo;
use prodrome::store::EventStore;

/// The vectors were taken with the reference payload, so the store these read
/// them into holds that record shape.
type Store = EventStore<Todo, Untrusted>;
fn roster() -> Untrusted {
    // The policy is the DEPLOYMENT's, never the engine's (§5) — it arrives as
    // a parameter here exactly as it does at every other call site. These
    // vectors were generated under the reference policy with `{"triage"}` on
    // its roster, so that is what their `verify` findings were taken under.
    Untrusted::of([Actor::new("triage").expect("valid")])
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// Write a vector's objects and heads out as a store on disk. `refs/` exists
/// only while there is more than one head, and HEAD names one of them — the
/// shape `EventStore::set_heads` would have produced.
/// One vector's objects, by name — the map the JSON keyed and the literal
/// holds as a tuple of `Object(name=, literal=)`.
fn objects_of(dag: &Value) -> BTreeMap<String, String> {
    each(dag, "objects")
        .iter()
        .map(|o| {
            (
                text_at(o, "name").to_owned(),
                text_at(o, "literal").to_owned(),
            )
        })
        .collect()
}

fn materialise(dag: &Value) -> Store {
    let root = std::env::temp_dir().join(format!(
        "prodrome-dag-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("objects")).expect("creates the store");
    let objects = objects_of(dag);
    let tips = strings(dag, "tips");
    for (name, text) in &objects {
        fs::write(root.join("objects").join(format!("{name}.py")), text).expect("writes an object");
    }
    if tips.len() > 1 {
        fs::create_dir_all(root.join("refs")).expect("creates refs/");
        for tip in &tips {
            fs::write(root.join("refs").join(tip), tip).expect("writes a ref");
        }
    }
    fs::write(root.join("HEAD"), &tips[0]).expect("writes HEAD");
    Store::new(root, roster())
}

#[test]
fn every_dag_vector_linearises_tips_parents_and_verifies_alike() {
    let data = vectors("dag.py");
    let dags = each(&data, "dags");
    assert!(!dags.is_empty());
    let mut merges = 0;
    let mut forked = 0;
    let mut findings = 0;
    for dag in dags {
        let seed = integer(field(dag, "seed"));
        let objects = objects_of(dag);
        let store = materialise(dag);
        let read: Vec<(Hash, prodrome::event::Envelope<Todo>)> = store
            .read_dag_named()
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));

        let order: Vec<&str> = read.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            order,
            strings(dag, "linearisation"),
            "seed {seed}: linearisation"
        );

        let tips: Vec<String> = store
            .tips()
            .iter()
            .map(|tip| tip.as_str().to_owned())
            .collect();
        assert_eq!(tips, strings(dag, "tips"), "seed {seed}: tips");
        if tips.len() > 1 {
            forked += 1;
        }

        let parents_of_name: BTreeMap<String, Vec<String>> = each(dag, "parents")
            .iter()
            .map(|p| (text_at(p, "name").to_owned(), strings(p, "parents")))
            .collect();
        for (name, object) in &read {
            let parents: Vec<String> = parents_of(object)
                .iter()
                .map(|parent| parent.as_str().to_owned())
                .collect();
            assert_eq!(
                &parents,
                &parents_of_name[name.as_str()],
                "seed {seed}: parents"
            );
            // Every object is still named by its own print.
            assert_eq!(seal_hash(object).as_str(), name.as_str());
            assert_eq!(
                &prodrome::event::canonical_envelope(object),
                &objects[name.as_str()]
            );
            if object.event().is_none() {
                merges += 1;
            }
        }

        let verify = strings(dag, "verify");
        assert_eq!(store.verify(), verify, "seed {seed}: verify");
        findings += verify.len();
        let _ = fs::remove_dir_all(store.root());
    }
    // The corpus exercises what it is for: forked stores, merge objects, and
    // real findings. A zero here would mean the vectors stopped covering a case
    // and this file went quietly green.
    assert!(forked > 0, "some vectors have two heads");
    assert!(merges > 0, "some vectors hold a merge envelope");
    assert!(findings > 0, "some vectors have verify findings");
}
