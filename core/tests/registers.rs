//! SPEC §9 law 6 and §6.6, on `conformance/dag.json` and `conformance/folds.json`.
//!
//! Two claims, and they are different claims. The FIRST is the vectors: each
//! DAG's `conflicts` — every register both branches wrote, named by the exact
//! objects that wrote it — and its `env` at a far moment. The SECOND is the
//! law behind them: on ANY dag, and on any chain, the register projections are
//! the event folds, so a conflicted todo never shows one write's content
//! beside another write's price.
//!
//! The DAGs are rebuilt from their object files, exactly as `tests/dag.rs`
//! rebuilds them: nothing about a frontier may depend on this side having been
//! the writer.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use prodrome::event::{parse_envelope, Actor, Envelope, Hash, TodoEvent};
use prodrome::fold::{authored_at, env_at, specs_at, Env, Untrusted};
use prodrome::fpl::{iso, print_term};
use prodrome::registers::{
    conflicts_of, content_of, env_of, extend, fold, nodes_of, since, specs_of, Folded, Node,
};
use prodrome::store::EventStore;
use serde::Deserialize;

#[derive(Deserialize)]
struct Dags {
    dags: Vec<Dag>,
}

#[derive(Deserialize)]
struct Dag {
    seed: u32,
    objects: BTreeMap<String, String>,
    tips: Vec<String>,
    env: BTreeMap<String, Outcome>,
    conflicts: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

#[derive(Deserialize, PartialEq, Eq, Debug)]
struct Outcome {
    kind: String,
    at: String,
}

#[derive(Deserialize)]
struct Logs {
    logs: Vec<Log>,
}

#[derive(Deserialize)]
struct Log {
    seed: u32,
    untrusted: Vec<String>,
    events: Vec<String>,
}

fn conformance(name: &str) -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "..", "conformance", name]
        .iter()
        .collect()
}

fn untrusted() -> Untrusted {
    // The deployment's roster was `{"triage"}`, which is what
    // the generator folded these vectors with. The roster arrives as a
    // parameter here exactly as it does there.
    Untrusted::of([Actor::new("triage").expect("valid")])
}

/// `ORIGIN + 400 days` — the "far" moment the generator asked `env_at` at,
/// later than every event any vector holds, so the dated fold sees the whole
/// DAG and can be compared with the undated register fold.
fn far() -> prodrome::literal::Datetime {
    prodrome::literal::Datetime::new(2027, 10, 6, 0, 0, 0, 0).expect("a real instant")
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn materialise(dag: &Dag) -> EventStore {
    let root = std::env::temp_dir().join(format!(
        "prodrome-registers-{}-{}",
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
    EventStore::new(
        root,
        [Actor::new("triage").expect("valid")].into_iter().collect(),
    )
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

fn events_of(nodes: &[Node]) -> Vec<TodoEvent> {
    nodes.iter().filter_map(|node| node.event.clone()).collect()
}

/// A log's events as a CHAIN of nodes, each sealed on the one before — which
/// is the DAG a single writer builds, and the shape the registers must agree
/// with the folds on.
fn chain_of(log: &Log) -> Vec<Node> {
    let mut prev = String::new();
    let mut nodes = Vec::new();
    for text in &log.events {
        let object = format!("Sealed(prev='{prev}', event={text})");
        let envelope = parse_envelope(&object).unwrap_or_else(|e| panic!("seed {}: {e}", log.seed));
        let name = prodrome::event::seal_hash(&envelope);
        prev = name.as_str().to_owned();
        nodes.push(Node::of(name, &envelope));
    }
    nodes
}

#[test]
fn every_dag_vector_has_the_references_conflicts_and_environment() {
    let raw = fs::read_to_string(conformance("dag.json")).expect("dag.json");
    let vectors: Dags = serde_json::from_str(&raw).expect("dag.json is the generator's shape");
    assert!(!vectors.dags.is_empty());
    let mut conflicted = 0;
    let mut registers = 0;
    for dag in &vectors.dags {
        let store = materialise(dag);
        let objects: Vec<(Hash, Envelope)> = store
            .read_dag_named()
            .unwrap_or_else(|e| panic!("seed {}: {e}", dag.seed));
        let nodes = nodes_of(&objects);
        let state = fold(&nodes, None, &untrusted());

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
        assert_eq!(found, dag.conflicts, "seed {}: conflicts", dag.seed);
        conflicted += found.len();

        assert_eq!(outcomes(&env_of(&state)), dag.env, "seed {}: env", dag.seed);
        registers += state.frontiers().len();
        let _ = fs::remove_dir_all(store.root());
    }
    // A conflict is what these vectors are FOR: a zero would mean the corpus
    // stopped forking and this file went quietly green.
    assert!(conflicted > 0, "some DAGs hold a conflicted todo");
    assert!(registers > 0, "the DAGs write registers");
}

#[test]
fn on_every_dag_the_registers_are_the_folds() {
    let raw = fs::read_to_string(conformance("dag.json")).expect("dag.json");
    let vectors: Dags = serde_json::from_str(&raw).expect("dag.json is the generator's shape");
    let policy = untrusted();
    for dag in &vectors.dags {
        let store = materialise(dag);
        let objects = store.read_dag_named().expect("the DAG reads");
        let nodes = nodes_of(&objects);
        let events = events_of(&nodes);
        let state = fold(&nodes, None, &policy);
        let seed = dag.seed;

        assert_eq!(
            env_of(&state),
            env_at(&events, far(), &policy),
            "seed {seed}: env"
        );
        assert_eq!(
            printed(&specs_of(&state)),
            printed(&specs_at(&events, far(), &policy)),
            "seed {seed}: specs"
        );
        assert_eq!(
            content_of(&state),
            authored_at(&events, far()),
            "seed {seed}: content"
        );
        let _ = fs::remove_dir_all(store.root());
    }
}

fn printed(
    specs: &BTreeMap<prodrome::event::TodoId, prodrome::fpl::Term>,
) -> BTreeMap<String, String> {
    specs
        .iter()
        .map(|(todo, spec)| (todo.as_str().to_owned(), print_term(spec)))
        .collect()
}

/// On a CHAIN the registers are the folds too, and nothing is ever in
/// conflict: every write descends from the one before it.
#[test]
fn on_every_log_the_registers_are_the_folds_and_nothing_conflicts() {
    let raw = fs::read_to_string(conformance("folds.json")).expect("folds.json");
    let vectors: Logs = serde_json::from_str(&raw).expect("folds.json is the generator's shape");
    let mut written = 0;
    for log in &vectors.logs {
        let policy = Untrusted::of(
            log.untrusted
                .iter()
                .map(|name| Actor::new(name.as_str()).expect("an actor name")),
        );
        let nodes = chain_of(log);
        let events = events_of(&nodes);
        let state = fold(&nodes, None, &policy);
        let seed = log.seed;
        assert_eq!(
            conflicts_of(&state),
            BTreeMap::new(),
            "seed {seed}: a chain has no conflicts"
        );
        assert_eq!(
            env_of(&state),
            env_at(&events, far(), &policy),
            "seed {seed}: env"
        );
        assert_eq!(
            printed(&specs_of(&state)),
            printed(&specs_at(&events, far(), &policy)),
            "seed {seed}: specs"
        );
        assert_eq!(
            content_of(&state),
            authored_at(&events, far()),
            "seed {seed}: content"
        );
        written += state.frontiers().len();
    }
    assert!(written > 0, "the logs write registers");
}

/// §6.6's monoid action, on the real DAGs: splitting the read at any point and
/// applying the halves in turn is the same state as folding the whole, and
/// applying what is already folded changes nothing.
#[test]
fn the_fold_is_a_monoid_action_on_every_dag() {
    let raw = fs::read_to_string(conformance("dag.json")).expect("dag.json");
    let vectors: Dags = serde_json::from_str(&raw).expect("dag.json is the generator's shape");
    let policy = untrusted();
    for dag in &vectors.dags {
        let store = materialise(dag);
        let objects = store.read_dag_named().expect("the DAG reads");
        let nodes = nodes_of(&objects);
        let whole = fold(&nodes, None, &policy);
        for split in 0..=nodes.len() {
            let stepped = extend(
                &fold(&nodes[..split], None, &policy),
                &nodes[split..],
                None,
                &policy,
            );
            assert_eq!(whole, stepped, "seed {}: split at {split}", dag.seed);
            // And the prefix that is already folded is exactly what `since`
            // declines to hand back.
            let prefix = fold(&nodes[..split], None, &policy);
            let left: Vec<&Node> = since(&prefix, &nodes);
            assert_eq!(
                left,
                nodes[split..].iter().collect::<Vec<_>>(),
                "seed {}: since at {split}",
                dag.seed
            );
        }
        assert_eq!(
            extend(&whole, &nodes, None, &policy),
            whole,
            "seed {}: re-extending with a prefix",
            dag.seed
        );
        let _ = fs::remove_dir_all(store.root());
    }
}

/// The frontier is the DAG's, not the reader's: every write in a conflict is
/// concurrent with every other, and every write NOT in it is superseded by one
/// that is. That is the definition, checked against the ancestry the state
/// itself computed.
#[test]
fn a_frontier_holds_exactly_the_writes_nothing_later_descends_from() {
    let raw = fs::read_to_string(conformance("dag.json")).expect("dag.json");
    let vectors: Dags = serde_json::from_str(&raw).expect("dag.json is the generator's shape");
    let policy = untrusted();
    let mut pairs = 0;
    for dag in &vectors.dags {
        let store = materialise(dag);
        let objects = store.read_dag_named().expect("the DAG reads");
        let nodes = nodes_of(&objects);
        let state = fold(&nodes, None, &policy);
        for frontier in state.frontiers().values() {
            let names: Vec<&Hash> = frontier.writes().iter().map(|write| &write.at).collect();
            // Sorted by name, so two states holding the same writes are equal.
            let sorted: Vec<&str> = names.iter().map(|name| name.as_str()).collect();
            let mut expected = sorted.clone();
            expected.sort_unstable();
            assert_eq!(sorted, expected, "seed {}: frontier order", dag.seed);
            for a in &names {
                for b in &names {
                    if a != b {
                        assert!(
                            !state.descends(a, b),
                            "seed {}: {a:?} descends from {b:?} and is still in the frontier",
                            dag.seed
                        );
                        pairs += 1;
                    }
                }
            }
        }
        let _ = fs::remove_dir_all(store.root());
    }
    assert!(pairs > 0, "the corpus holds a frontier with two writes");
}

/// The empty state is the unit, and it holds nothing — the shape that makes
/// "no writes" an absent key rather than an empty frontier.
#[test]
fn the_empty_state_is_the_unit() {
    let policy = untrusted();
    let empty = Folded::empty();
    assert!(empty.frontiers().is_empty());
    assert_eq!(extend(&empty, &[], None, &policy), empty);
    assert_eq!(fold(&[], None, &policy), empty);
    assert_eq!(env_of(&empty), Env::new());
    assert_eq!(conflicts_of(&empty), BTreeMap::new());
    let unknown = Hash::new("a".repeat(64)).expect("hex");
    assert!(!empty.descends(&unknown, &unknown));
    assert!(!empty.holds(&unknown));
    let _: BTreeSet<_> = prodrome::registers::registers_of(&empty);
}
