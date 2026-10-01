//! The register types' laws (`docs/design-register-types.md` §7): each
//! order is a partial order, over generated values; and a register reads
//! the maximal values of its frontier, over generated DAGs.
//!
//! Beside the todo vocabulary's orders, a small state machine that no
//! register of the vocabulary uses, so the laws see an order that is neither
//! discrete, total nor inclusion. Its values ride in the notes of state
//! writes, so they reach a frontier through the same fold as the todo's.
//!
//! Law 6, that the todo vocabulary reads byte for byte as before, is every
//! vector suite beside this one, unchanged.

use std::collections::BTreeSet;

use prodrome::event::{mk_completed, Hash, TodoEvent, TodoId};
use prodrome::fold::{maximal, Discrete, Frontier, Order, Registers, Total};
use prodrome::literal::Datetime;
use prodrome::policy::Everything;
use prodrome::reference::Todo;
use prodrome::registers::{fold, Folded, Node, Stamp};
use proptest::prelude::*;

/// `Draft < Review < Merged` and `Draft < Closed`: further along is greater,
/// and `Merged` and `Closed` are the one incomparable pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Draft,
    Review,
    Merged,
    Closed,
}

impl Order for Phase {
    fn le(&self, other: &Self) -> bool {
        use Phase::*;
        matches!(
            (self, other),
            (Draft, _) | (Review, Review | Merged) | (Merged, Merged) | (Closed, Closed)
        )
    }
}

const PHASES: [Phase; 4] = [Phase::Draft, Phase::Review, Phase::Merged, Phase::Closed];

fn a_phase() -> impl Strategy<Value = Phase> {
    prop::sample::select(PHASES.to_vec())
}

/// Reflexive, antisymmetric and transitive at `a`, `b`, `c`.
fn is_partial_order<V: Order + PartialEq + std::fmt::Debug>(a: &V, b: &V, c: &V) {
    assert!(a.le(a), "reflexive at {a:?}");
    if a.le(b) && b.le(a) {
        assert_eq!(a, b, "antisymmetric");
    }
    if a.le(b) && b.le(c) {
        assert!(a.le(c), "transitive at {a:?} ≤ {b:?} ≤ {c:?}");
    }
}

fn a_set() -> impl Strategy<Value = BTreeSet<u8>> {
    prop::collection::btree_set(0u8..4, 0..4)
}

proptest! {
    /// Law 2: every order is a partial order.
    #[test]
    fn every_order_is_a_partial_order(
        discrete in prop::array::uniform3(0u8..3),
        total in prop::array::uniform3(0u8..3),
        sets in prop::array::uniform3(a_set()),
        phases in prop::array::uniform3(a_phase()),
    ) {
        let [a, b, c] = discrete.map(Discrete);
        is_partial_order(&a, &b, &c);
        let [a, b, c] = total.map(Total);
        is_partial_order(&a, &b, &c);
        let [a, b, c] = sets;
        is_partial_order(&a, &b, &c);
        let [a, b, c] = phases;
        is_partial_order(&a, &b, &c);
    }
}

#[test]
fn the_orders_are_the_ones_named() {
    assert!(!Discrete(1).le(&Discrete(2)) && !Discrete(2).le(&Discrete(1)));
    assert!(Total(1).le(&Total(2)) && !Total(2).le(&Total(1)));
    let (one, two, both) = (
        BTreeSet::from([1]),
        BTreeSet::from([2]),
        BTreeSet::from([1, 2]),
    );
    assert!(Order::le(&one, &both) && !Order::le(&one, &two));
    assert!(Phase::Draft.le(&Phase::Merged) && Phase::Review.le(&Phase::Merged));
    assert!(!Phase::Merged.le(&Phase::Closed) && !Phase::Closed.le(&Phase::Merged));
}

/// One write of a generated DAG: the value it carries (an index into each
/// order's values), a day among two (so equal writes are twins), and the
/// earlier writes it rests on.
type Step = (u8, u32, u64);

fn a_dag() -> impl Strategy<Value = Vec<Step>> {
    prop::collection::vec((0u8..4, 0u32..2, any::<u64>()), 1..10)
}

fn name(i: usize) -> Hash {
    Hash::new(format!("{:064x}", i + 1)).expect("64 hex")
}

/// The steps as legacy objects of one todo, each a state write carrying its
/// value in its note.
fn nodes(dag: &[Step]) -> Vec<Node<Todo>> {
    dag.iter()
        .enumerate()
        .map(|(i, (value, day, parents))| {
            let at = Datetime::new(2026, 9, 1 + day, 12, 0, 0, 0).expect("a real instant");
            let event = mk_completed("alpha", at, "writer", &value.to_string()).expect("valid");
            Node {
                name: name(i),
                parents: (0..i.min(64))
                    .filter(|j| parents >> j & 1 == 1)
                    .map(name)
                    .collect(),
                event: Some(event),
                genesis: None,
            }
        })
        .collect()
}

fn value(stamp: &Stamp<Todo>) -> u8 {
    match &*stamp.event {
        TodoEvent::Completed(e) => e.note.parse().expect("a value"),
        _ => unreachable!("every write is a completion"),
    }
}

/// The todo's state frontier, read structurally.
fn frontier(folded: &Folded<Todo>) -> Frontier<'_, Todo> {
    let todo = TodoId::new("alpha").expect("valid");
    Registers::read(&folded.prodromes()[&None][&todo], None, &Everything).state
}

/// Law 3 at one frontier under `order`: the reading is exactly the maximal
/// values, each once, and one value exactly when one is greatest.
fn is_completion<V: Order + std::fmt::Debug>(frontier: &Frontier<Todo>, order: impl Fn(u8) -> V) {
    let of = |stamp: &Stamp<Todo>| order(value(stamp));
    let all: Vec<V> = frontier.writes().iter().map(|s| of(s)).collect();
    let read: Vec<V> = frontier.read(of).into_iter().map(of).collect();
    let maximal = |v: &V| !all.iter().any(|w| v.le(w) && !w.le(v));
    let equal = |v: &V, w: &V| v.le(w) && w.le(v);
    assert!(
        read.iter().all(maximal),
        "only maximal values: {read:?} of {all:?}"
    );
    for v in all.iter().filter(|v| maximal(v)) {
        assert_eq!(read.iter().filter(|w| equal(v, w)).count(), 1, "{v:?} once");
    }
    let greatest = all.iter().any(|g| all.iter().all(|v| v.le(g)));
    assert_eq!(read.len() == 1, greatest, "one value iff one is greatest");
}

proptest! {
    /// Law 3: a reading is the set of maximal values of its frontier's
    /// values, under every order.
    #[test]
    fn a_reading_is_the_maximal_values(dag in a_dag()) {
        let folded = fold(&nodes(&dag));
        let frontier = frontier(&folded);
        is_completion(&frontier, Discrete);
        is_completion(&frontier, Total);
        is_completion(&frontier, |i| PHASES[usize::from(i)]);
    }

    /// Under the discrete order on events the reading is the candidates as
    /// they always were: the distinct events, in name order, each named by
    /// the least write that carries it.
    #[test]
    fn the_discrete_reading_is_the_candidates(dag in a_dag()) {
        let folded = fold(&nodes(&dag));
        let frontier = frontier(&folded);
        let mut twins_once: Vec<&Stamp<Todo>> = Vec::new();
        for stamp in frontier.writes() {
            if !twins_once.iter().any(|held| held.event == stamp.event) {
                twins_once.push(stamp);
            }
        }
        prop_assert_eq!(frontier.candidates(), twins_once);
    }
}

#[test]
fn the_completion_keeps_the_first_of_equal_values_in_order() {
    use Phase::*;
    let phases = [Review, Closed, Draft, Merged, Closed];
    assert_eq!(maximal(&phases, |p| p), [Closed, Merged]);
    assert_eq!(maximal(&[3, 1, 3], Total), [3]);
    assert_eq!(maximal(&[3, 1, 3], Discrete), [3, 1]);
    assert!(maximal(&[] as &[u8], Discrete).is_empty());
}
