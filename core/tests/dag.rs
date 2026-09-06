//! SPEC §9 law 8 for §3, on `conformance/dag.json`: linearisations, tips,
//! parents and `verify` findings EXACTLY.
//!
//! Each vector is a store the reference built by appending, adopting another
//! replica's objects, and sometimes merging — so the DAGs have two heads,
//! merges, and triage-actor events dated behind what they rest on. Each is
//! rebuilt here by writing the object files and the heads, which is the honest
//! way to test a READER: nothing about the order or the findings may depend on
//! this side having been the writer.
//!
//! The live chain (`events/` at the repository root) is checked at the end of
//! this file — 564 objects, `verify` clean, and `read_dag`'s order equal to the
//! order `literals.json` was generated in.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use prodrome::event::{parents_of, seal_hash, Actor, Hash};
use prodrome::store::EventStore;
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

fn repo_root() -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "..", ".."].iter().collect()
}

fn untrusted() -> BTreeSet<Actor> {
    // `suzatary/instance.py`: UNTRUSTED = frozenset({"triage"}). The roster is
    // the deployment's, never the engine's — it arrives as a parameter here
    // exactly as it does there.
    [Actor::new("triage").expect("valid")].into_iter().collect()
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// Write a vector's objects and heads out as a store on disk. `refs/` exists
/// only while there is more than one head, and HEAD names one of them — the
/// shape `EventStore::set_heads` would have produced.
fn materialise(dag: &Dag) -> EventStore {
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
    EventStore::new(root, untrusted())
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
        let read: Vec<(Hash, prodrome::event::Envelope)> = store
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

/// The live chain, read as the deployment reads it.
#[test]
fn the_live_chain_reads_clean_and_in_the_generators_order() {
    let events = repo_root().join("events");
    if !events.is_dir() {
        // The crate is meant to be usable outside this repository; the live
        // chain is evidence, not a dependency.
        return;
    }
    let store = EventStore::new(&events, untrusted());
    let read = store.read_dag_named().expect("the live chain reads");
    assert_eq!(read.len(), 564, "the chain as of the vectors' generation");
    assert_eq!(store.verify(), Vec::<String>::new(), "verify is clean");

    #[derive(Deserialize)]
    struct Literals {
        objects: Vec<Object>,
    }
    #[derive(Deserialize)]
    struct Object {
        name: String,
    }
    let raw = fs::read_to_string(conformance("literals.json")).expect("literals.json");
    let literals: Literals =
        serde_json::from_str(&raw).expect("literals.json is the generator's shape");
    let generated: Vec<&str> = literals
        .objects
        .iter()
        .map(|object| object.name.as_str())
        .collect();
    let ours: Vec<&str> = read.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(ours, generated, "read_dag's order is the generator's");

    // One head, no refs/, and `read_chain` agrees with `read_dag` on it — the
    // chain is the DAG in which every object has one parent.
    assert_eq!(store.tips().len(), 1);
    let chain = store.read_chain().expect("the live store is a chain");
    assert_eq!(chain.len(), read.len());
    assert!(chain
        .iter()
        .zip(read.iter())
        .all(|(sealed, (_, object))| sealed == object));
}
