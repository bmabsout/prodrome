//! SPEC §9 law 6 and §6.6, on `conformance/dag.py` and `conformance/folds.py`.
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

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::vectors::{each, field, integer, moment, strings, text, text_at, vectors};
use prodrome::event::{parse_envelope, Actor, Envelope, Hash, TodoEvent};
use prodrome::fold::{authored_at, env_at, specs_at, Env};
use prodrome::fpl::{iso, print_term};
use prodrome::literal::Value;
use prodrome::policy::Untrusted;
use prodrome::reference::Todo;
use prodrome::registers::{
    conflicts_of, content_of, env_of, extend, fold, nodes_of, since, specs_of, Folded, Node,
};
use prodrome::store::EventStore;

/// The vectors were taken with the reference payload, so that is the record
/// shape they are read back under.
type Event = TodoEvent<Todo>;
type Chain = Node<Todo>;
type Store = EventStore<Todo, Untrusted>;

/// An environment as the vectors hold it: `(todo, kind, instant)`.
type Outcomes = BTreeMap<String, (String, String)>;

fn dags() -> Vec<Value> {
    each(&vectors("dag.py"), "dags").to_vec()
}

fn logs() -> Vec<Value> {
    each(&vectors("folds.py"), "logs").to_vec()
}

fn roster() -> Untrusted {
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
        "prodrome-registers-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("objects")).expect("creates the store");
    let tips = strings(dag, "tips");
    for (name, text) in &objects_of(dag) {
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

fn frozen_outcomes(dag: &Value) -> Outcomes {
    each(dag, "env")
        .iter()
        .map(|bound| {
            (
                text_at(bound, "todo").to_owned(),
                (
                    text_at(bound, "kind").to_owned(),
                    iso(prodrome::fpl::instant_of(moment(field(bound, "at")))),
                ),
            )
        })
        .collect()
}

/// A DAG's conflicts as the vector holds them: by todo, then by register kind,
/// each the object names that wrote it.
fn frozen_conflicts(dag: &Value) -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
    let mut out: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for held in each(dag, "conflicts") {
        out.entry(text_at(held, "todo").to_owned())
            .or_default()
            .insert(text_at(held, "kind").to_owned(), strings(held, "writes"));
    }
    out
}

fn events_of(nodes: &[Chain]) -> Vec<Event> {
    nodes.iter().filter_map(|node| node.event.clone()).collect()
}

/// A log's events as a CHAIN of nodes, each sealed on the one before — which
/// is the DAG a single writer builds, and the shape the registers must agree
/// with the folds on.
fn chain_of(log: &Value) -> Vec<Chain> {
    let seed = integer(field(log, "seed"));
    let mut prev = String::new();
    let mut nodes = Vec::new();
    for item in each(log, "events") {
        let object = format!("Sealed(prev='{prev}', event={})", text(item));
        let envelope: Envelope<Todo> =
            parse_envelope(&object).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let name = prodrome::event::seal_hash(&envelope);
        prev = name.as_str().to_owned();
        nodes.push(Node::of(name, &envelope));
    }
    nodes
}

#[test]
fn every_dag_vector_has_the_references_conflicts_and_environment() {
    let dags = dags();
    assert!(!dags.is_empty());
    let mut conflicted = 0;
    let mut registers = 0;
    for dag in &dags {
        let seed = integer(field(dag, "seed"));
        let store = materialise(dag);
        let objects: Vec<(Hash, Envelope<Todo>)> = store
            .read_dag_named()
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let nodes = nodes_of(&objects);
        let state = fold(&nodes, None, &roster());

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
        assert_eq!(found, frozen_conflicts(dag), "seed {seed}: conflicts");
        conflicted += found.len();

        assert_eq!(
            outcomes(&env_of(&state)),
            frozen_outcomes(dag),
            "seed {seed}: env"
        );
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
    let dags = dags();
    let policy = roster();
    for dag in &dags {
        let seed = integer(field(dag, "seed"));
        let store = materialise(dag);
        let objects = store.read_dag_named().expect("the DAG reads");
        let nodes = nodes_of(&objects);
        let events = events_of(&nodes);
        let state = fold(&nodes, None, &policy);

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
    let logs = logs();
    let mut written = 0;
    for log in &logs {
        let seed = integer(field(log, "seed"));
        let policy = Untrusted::of(
            strings(log, "untrusted")
                .iter()
                .map(|name| Actor::new(name.as_str()).expect("an actor name")),
        );
        let nodes = chain_of(log);
        let events = events_of(&nodes);
        let state = fold(&nodes, None, &policy);
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
    let dags = dags();
    let policy = roster();
    for dag in &dags {
        let seed = integer(field(dag, "seed"));
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
            assert_eq!(whole, stepped, "seed {seed}: split at {split}");
            // And the prefix that is already folded is exactly what `since`
            // declines to hand back.
            let prefix = fold(&nodes[..split], None, &policy);
            let left: Vec<&Chain> = since(&prefix, &nodes);
            assert_eq!(
                left,
                nodes[split..].iter().collect::<Vec<_>>(),
                "seed {seed}: since at {split}"
            );
        }
        assert_eq!(
            extend(&whole, &nodes, None, &policy),
            whole,
            "seed {seed}: re-extending with a prefix"
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
    let dags = dags();
    let policy = roster();
    let mut pairs = 0;
    for dag in &dags {
        let seed = integer(field(dag, "seed"));
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
            assert_eq!(sorted, expected, "seed {seed}: frontier order");
            for a in &names {
                for b in &names {
                    if a != b {
                        assert!(
                            !state.descends(a, b),
                            "seed {seed}: {a:?} descends from {b:?} and is still in the frontier"
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
    let policy = roster();
    let empty = Folded::<Todo>::empty();
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
