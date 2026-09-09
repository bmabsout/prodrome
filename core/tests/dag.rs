//! SPEC §9 law 8 for §3, on `conformance/dag.json`: linearisations, tips,
//! parents and `verify` findings EXACTLY.
//!
//! Each vector is a store the reference built by appending, adopting another
//! replica's objects, and sometimes merging — so the DAGs have two heads,
//! merges, and triage-actor events dated behind what they rest on. Each is
//! rebuilt here by writing the object files and the heads, which is the honest
//! way to test a READER: nothing about the order or the findings may depend on
//! this side having been the writer.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use prodrome::event::{parents_of, seal_hash, Actor, Hash};
use prodrome::reference::Todo;
use prodrome::store::EventStore;

/// The vectors were taken with the reference payload, so the store these read
/// them into holds that record shape.
type Store = EventStore<Todo>;
use serde::Deserialize;

#[derive(Deserialize)]
struct Vectors {
    dags: Vec<Dag>,
}

#[derive(Deserialize)]
struct Dag {
    seed: u32,
    objects: BTreeMap<String, String>,
    tips: Vec<String>,
    linearisation: Vec<String>,
    parents: BTreeMap<String, Vec<String>>,
    verify: Vec<String>,
}

fn conformance(name: &str) -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "..", "conformance", name]
        .iter()
        .collect()
}

fn untrusted() -> BTreeSet<Actor> {
    // The roster is the DEPLOYMENT's, never the engine's (§5) — it arrives as
    // a parameter here exactly as it does at every other call site. These
    // vectors were generated under `{"triage"}`, so that is the policy their
    // `verify` findings were taken under.
    [Actor::new("triage").expect("valid")].into_iter().collect()
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// Write a vector's objects and heads out as a store on disk. `refs/` exists
/// only while there is more than one head, and HEAD names one of them — the
/// shape `EventStore::set_heads` would have produced.
fn materialise(dag: &Dag) -> Store {
    let root = std::env::temp_dir().join(format!(
        "prodrome-dag-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("objects")).expect("creates the store");
    for (name, text) in &dag.objects {
        fs::write(root.join("objects").join(format!("{name}.py")), text).expect("writes an object");
    }
    if dag.tips.len() > 1 {
        fs::create_dir_all(root.join("refs")).expect("creates refs/");
        for tip in &dag.tips {
            fs::write(root.join("refs").join(tip), tip).expect("writes a ref");
        }
    }
    fs::write(root.join("HEAD"), &dag.tips[0]).expect("writes HEAD");
    Store::new(root, untrusted())
}

#[test]
fn every_dag_vector_linearises_tips_parents_and_verifies_alike() {
    let raw = fs::read_to_string(conformance("dag.json")).expect("dag.json");
    let vectors: Vectors = serde_json::from_str(&raw).expect("dag.json is the generator's shape");
    assert!(!vectors.dags.is_empty());
    let mut merges = 0;
    let mut forked = 0;
    let mut findings = 0;
    for dag in &vectors.dags {
        let store = materialise(dag);
        let read: Vec<(Hash, prodrome::event::Envelope<Todo>)> = store
            .read_dag_named()
            .unwrap_or_else(|e| panic!("seed {}: {e}", dag.seed));

        let order: Vec<&str> = read.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(order, dag.linearisation, "seed {}: linearisation", dag.seed);

        let tips: Vec<String> = store
            .tips()
            .iter()
            .map(|tip| tip.as_str().to_owned())
            .collect();
        assert_eq!(tips, dag.tips, "seed {}: tips", dag.seed);
        if tips.len() > 1 {
            forked += 1;
        }

        for (name, object) in &read {
            let parents: Vec<String> = parents_of(object)
                .iter()
                .map(|parent| parent.as_str().to_owned())
                .collect();
            assert_eq!(
                &parents,
                &dag.parents[name.as_str()],
                "seed {}: parents",
                dag.seed
            );
            // Every object is still named by its own print.
            assert_eq!(seal_hash(object).as_str(), name.as_str());
            assert_eq!(
                &prodrome::event::canonical_envelope(object),
                &dag.objects[name.as_str()]
            );
            if object.event().is_none() {
                merges += 1;
            }
        }

        assert_eq!(store.verify(), dag.verify, "seed {}: verify", dag.seed);
        findings += dag.verify.len();
        let _ = fs::remove_dir_all(store.root());
    }
    // The corpus exercises what it is for: forked stores, merge objects, and
    // real findings. A zero here would mean the vectors stopped covering a case
    // and this file went quietly green.
    assert!(forked > 0, "some vectors have two heads");
    assert!(merges > 0, "some vectors hold a merge envelope");
    assert!(findings > 0, "some vectors have verify findings");
}
