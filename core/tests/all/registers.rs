//! SPEC §6.6, on `conformance/dag.py` and `conformance/folds.py`:
//! each DAG's conflicts, named by the exact objects that wrote them, and its
//! environment at a far moment, a state conflict read as its candidates; and
//! the structure the registers stand on — the monoid action, frontiers,
//! agreeing twins, and deps that never leave a todo.
//!
//! The DAGs are rebuilt from their object files: nothing about a frontier may
//! depend on this side having been the writer.

use crate::common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::vectors::{each, field, integer, moment, strings, text, text_at, vectors};
use prodrome::change::mk_change;
use prodrome::dag::Dag;
use prodrome::event::{
    mk_cancelled, mk_completed, mk_created, mk_reopened, mk_sealed, parse_envelope, seal_hash,
    Actor, Envelope, Hash, TodoEvent,
};
use prodrome::fold::{Kind, Product};
use prodrome::fpl::{instant_of, iso, Env};
use prodrome::genesis::mk_genesis;
use prodrome::literal::{Datetime, Value};
use prodrome::policy::{Everything, Untrusted};
use prodrome::reference::Todo;
use prodrome::registers::{deps_for, extend, fold, since, Folded, Node};
use prodrome::store::EventStore;
use prodrome::view;

type Chain = Node<TodoEvent<Todo>>;
type Store = EventStore<TodoEvent<Todo>, Untrusted>;

/// An environment as the vectors hold it: `(todo, kind, instant)` per
/// candidate, `("open", "")` for an open one.
type Outcomes = BTreeSet<(String, String, String)>;

fn dags() -> Vec<Value> {
    each(&vectors("dag.py"), "dags").to_vec()
}

fn logs() -> Vec<Value> {
    each(&vectors("folds.py"), "logs").to_vec()
}

fn roster() -> Untrusted {
    Untrusted::of([Actor::new("triage").expect("valid")])
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn materialise(dag: &Value) -> Store {
    let root = std::env::temp_dir().join(format!(
        "prodrome-registers-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("objects")).expect("creates the store");
    let tips = strings(dag, "tips");
    for object in each(dag, "objects") {
        let path = root
            .join("objects")
            .join(format!("{}.py", text_at(object, "name")));
        fs::write(path, text_at(object, "literal")).expect("writes an object");
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

fn nodes(store: &Store) -> Vec<Chain> {
    store
        .dag()
        .and_then(|dag| dag.nodes())
        .expect("the DAG reads")
}

fn outcomes(env: &Env) -> Outcomes {
    env.outcomes
        .iter()
        .flat_map(|(todo, candidates)| {
            candidates.iter().map(move |binding| match binding {
                Some(b) => (todo.clone(), b.kind().to_owned(), iso(b.at())),
                None => (todo.clone(), "open".to_owned(), String::new()),
            })
        })
        .collect()
}

fn frozen_outcomes(dag: &Value) -> Outcomes {
    each(dag, "env")
        .iter()
        .map(|bound| {
            let todo = text_at(bound, "todo").to_owned();
            match bound.as_call().map(|call| call.name.as_str()) {
                Some("Open") => (todo, "open".to_owned(), String::new()),
                _ => (
                    todo,
                    text_at(bound, "kind").to_owned(),
                    iso(instant_of(moment(field(bound, "at")))),
                ),
            }
        })
        .collect()
}

fn frozen_conflicts(dag: &Value) -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
    let mut out: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for held in each(dag, "conflicts") {
        out.entry(text_at(held, "todo").to_owned())
            .or_default()
            .insert(text_at(held, "kind").to_owned(), strings(held, "writes"));
    }
    out
}

/// A log as the chain a single writer builds.
fn chain_of(log: &Value) -> Vec<Chain> {
    let seed = integer(field(log, "seed"));
    let mut prev = String::new();
    let mut objects = Vec::new();
    for item in each(log, "events") {
        let object = format!("Sealed(prev='{prev}', event={})", text(item));
        let envelope: Envelope<TodoEvent<Todo>> =
            parse_envelope(&object).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let name = seal_hash(&envelope);
        prev = name.as_str().to_owned();
        objects.push((name, envelope));
    }
    objects
        .into_iter()
        .collect::<Dag<TodoEvent<Todo>>>()
        .nodes()
        .unwrap_or_else(|e| panic!("seed {seed}: {e}"))
}

#[test]
fn every_dag_vector_has_the_references_conflicts_and_environment() {
    let mut conflicted = 0;
    for dag in &dags() {
        let seed = integer(field(dag, "seed"));
        let store = materialise(dag);
        let nodes = store
            .dag()
            .and_then(|dag| dag.nodes())
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let state = fold(&nodes);

        let mut found: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
        let mut env = Env::new();
        for (todo, stream) in state.entities() {
            let registers = prodrome::fold::read(stream, None, &roster());
            registers.bind(todo, &mut env);
            for (kind, names) in registers.conflicts() {
                found.entry(todo.as_str().to_owned()).or_default().insert(
                    kind.as_str().to_owned(),
                    names.iter().map(|n| n.as_str().to_owned()).collect(),
                );
            }
        }
        assert_eq!(found, frozen_conflicts(dag), "seed {seed}: conflicts");
        conflicted += found.len();
        assert_eq!(outcomes(&env), frozen_outcomes(dag), "seed {seed}: env");
        let _ = fs::remove_dir_all(store.root());
    }
    assert!(conflicted > 0, "some DAGs hold a conflicted todo");
}

#[test]
fn on_a_chain_nothing_conflicts() {
    let mut written = 0;
    for log in &logs() {
        let seed = integer(field(log, "seed"));
        let state = fold(&chain_of(log));
        for (_, stream) in state.entities() {
            let registers = prodrome::fold::read(stream, None, &Everything);
            assert!(registers.conflicts().is_empty(), "seed {seed}");
            written += registers.state.writes().len() + registers.spec.writes().len();
        }
    }
    assert!(written > 0, "the logs write registers");
}

/// §6.6's monoid action, on the real DAGs.
#[test]
fn the_fold_is_a_monoid_action_on_every_dag() {
    for dag in &dags() {
        let seed = integer(field(dag, "seed"));
        let store = materialise(dag);
        let nodes = nodes(&store);
        let whole = fold(&nodes);
        for split in 0..=nodes.len() {
            let prefix = fold(&nodes[..split]);
            assert_eq!(
                whole,
                extend(&prefix, &nodes[split..]),
                "seed {seed}: {split}"
            );
            assert_eq!(
                since(&prefix, &nodes),
                nodes[split..].iter().collect::<Vec<_>>(),
                "seed {seed}: since at {split}"
            );
        }
        assert_eq!(extend(&whole, &nodes), whole, "seed {seed}: re-extending");
        let _ = fs::remove_dir_all(store.root());
    }
}

/// Every write in a frontier is concurrent with every other, checked against
/// the ancestry the state computed.
#[test]
fn a_frontier_holds_exactly_the_writes_nothing_later_descends_from() {
    let mut pairs = 0;
    for dag in &dags() {
        let seed = integer(field(dag, "seed"));
        let store = materialise(dag);
        let state = fold(&nodes(&store));
        for (_, stream) in state.entities() {
            let registers = prodrome::fold::read(stream, None, &Everything);
            for kind in [Kind::State, Kind::Spec, Kind::Content] {
                let names = registers.frontier(kind).names();
                let mut sorted = names.clone();
                sorted.sort();
                assert_eq!(names, sorted, "seed {seed}: frontier order");
                for a in &names {
                    for b in names.iter().filter(|b| *b != a) {
                        assert!(!state.descends(a, b), "seed {seed}: {a:?} over {b:?}");
                        pairs += 1;
                    }
                }
            }
        }
        let _ = fs::remove_dir_all(store.root());
    }
    assert!(pairs > 0, "the corpus holds a frontier with two writes");
}

#[test]
fn the_empty_state_is_the_unit() {
    let empty = Folded::<TodoEvent<Todo>>::empty();
    assert_eq!(empty.entities().count(), 0);
    assert_eq!(extend(&empty, &[]), empty);
    assert_eq!(fold::<TodoEvent<Todo>>(&[]), empty);
    let unknown = Hash::new("a".repeat(64)).expect("hex");
    assert!(!empty.descends(&unknown, &unknown));
    assert!(!empty.holds(&unknown));
}

fn at(day: u32) -> Datetime {
    Datetime::new(2026, 9, day, 12, 0, 0, 0).expect("a real instant")
}

/// Changes of one genesis, each named by its own print, as a DAG.
struct Prodrome {
    genesis: Hash,
    dag: Dag<TodoEvent<Todo>>,
}

impl Prodrome {
    fn new() -> Prodrome {
        let genesis = Envelope::<TodoEvent<Todo>>::Genesis(
            mk_genesis("test", &"0".repeat(32)).expect("a genesis"),
        );
        let name = seal_hash(&genesis);
        Prodrome {
            dag: [(name.clone(), genesis)].into_iter().collect(),
            genesis: name,
        }
    }

    fn write(&mut self, deps: &[&Hash], event: TodoEvent<Todo>) -> Hash {
        let deps = deps.iter().map(|dep| (*dep).clone()).collect();
        let change = mk_change(self.genesis.clone(), deps, event).expect("a change");
        let envelope = Envelope::Change(change);
        let name = seal_hash(&envelope);
        let mut objects = self.dag.objects().clone();
        objects.insert(name.clone(), envelope);
        self.dag = objects.into_iter().collect();
        name
    }

    /// The nodes, as the DAG places them: the only way to have one.
    fn nodes(&self) -> Vec<Chain> {
        self.dag.nodes().expect("closed under deps")
    }
}

/// Law 24: the same event written over two different views is two objects and
/// ONE candidate. The frontier names both, and the reading is one.
#[test]
fn agreeing_twins_are_one_candidate() {
    let mut p = Prodrome::new();
    let done = p.write(
        &[],
        mk_completed("alpha", at(1), "bassel", "").expect("valid"),
    );
    let dropped = p.write(
        &[],
        mk_cancelled("alpha", at(2), "bassel", "").expect("valid"),
    );
    let reopened = mk_reopened("alpha", at(3), "bassel", "").expect("valid");
    let one = p.write(&[&done], reopened.clone());
    let two = p.write(&[&dropped], reopened.clone());
    assert_ne!(one, two, "twins are two objects");

    let state = fold(&p.nodes());
    let (_, stream) = state.entities().next().expect("alpha");
    let registers = prodrome::fold::read(stream, None, &roster());
    let mut twins = vec![one.clone(), two.clone()];
    twins.sort();
    assert_eq!(registers.state.names(), twins, "both are in the frontier");
    assert_eq!(registers.state.candidates().len(), 1, "one candidate");
    assert_eq!(registers.outcomes(), [None].into(), "and it reads open");

    let genesis = Some(p.genesis.clone());
    assert_eq!(
        deps_for(&state, &genesis, &reopened).expect("no register here is inflationary"),
        twins
    );
    let rows = view::entries(&p.nodes(), at(9), &roster()).expect("folds");
    let [row] = &rows[..] else { panic!("one todo") };
    assert_eq!(row.genesis, genesis);
    assert!(row.is_open());
    assert_eq!(row.conflicts[&Kind::State], twins);
}

/// Law 21's footing: a change's deps are its own todo's heads, whatever
/// else the prodrome holds.
#[test]
fn deps_name_one_todo() {
    let mut p = Prodrome::new();
    let alpha = p.write(
        &[],
        mk_completed("alpha", at(1), "bassel", "").expect("valid"),
    );
    let state = fold(&p.nodes());
    let genesis = Some(p.genesis.clone());
    let beta = mk_completed("beta", at(2), "bassel", "").expect("valid");
    assert!(deps_for(&state, &genesis, &beta)
        .expect("no register here is inflationary")
        .is_empty());
    let again = mk_reopened("alpha", at(2), "bassel", "").expect("valid");
    assert_eq!(
        deps_for(&state, &genesis, &again).expect("no register here is inflationary"),
        std::slice::from_ref(&alpha)
    );
    let tended = prodrome::event::mk_tended("alpha", at(2), "bassel", "").expect("valid");
    assert_eq!(
        deps_for(&state, &genesis, &tended).expect("no register here is inflationary"),
        [alpha],
        "a tending supersedes nothing, and rests on what its writer saw"
    );
    assert!(
        deps_for(&state, &None, &again)
            .expect("no register here is inflationary")
            .is_empty(),
        "another prodrome's frontier is not mine"
    );
}

/// A MIXED STORE READS ONE TODO AS ONE STREAM. A legacy chain, and a change
/// written into it after 0.9, naming the legacy root as its genesis: the
/// change is in the legacy prodrome, which nothing in its own bytes says, so
/// only the DAG can place it. Folded through the DAG, the todo is one
/// stream of three writes, the change descending from both legacy ones, and
/// there is no second prodrome to split it into.
#[test]
fn a_mixed_store_folds_a_todo_as_one_stream() {
    let root = mk_sealed(
        None,
        mk_created("alpha", at(1), "bassel", "", "").expect("valid"),
    );
    let root_name = seal_hash(&root);
    let done = mk_sealed(
        Some(root_name.clone()),
        mk_completed("alpha", at(2), "bassel", "").expect("valid"),
    );
    let done_name = seal_hash(&done);
    let reopened = Envelope::Change(
        mk_change(
            root_name.clone(),
            vec![done_name.clone()],
            mk_reopened("alpha", at(3), "bassel", "").expect("valid"),
        )
        .expect("a change"),
    );
    let reopened_name = seal_hash(&reopened);
    let dag: Dag<TodoEvent<Todo>> = [
        (root_name.clone(), root),
        (done_name.clone(), done),
        (reopened_name.clone(), reopened),
    ]
    .into_iter()
    .collect();
    let nodes = dag.nodes().expect("closed under parents");
    assert!(nodes.iter().all(|node| node.genesis().is_none()));
    let state = fold(&nodes);
    assert_eq!(state.prodromes().keys().collect::<Vec<_>>(), vec![&None]);
    let todo = prodrome::event::TodoId::new("alpha").expect("valid");
    let stream: Vec<&Hash> = state.prodromes()[&None][&todo]
        .iter()
        .map(|stamp| &stamp.name)
        .collect();
    assert_eq!(stream, vec![&root_name, &done_name, &reopened_name]);
    assert!(state.descends(&reopened_name, &root_name));
}
