//! Law 1 of `docs/design-register-types.md` §7, FREE: a reading is a
//! function of the object set. Every permutation of a history that keeps
//! parents first, every duplication, and every partition into two replicas
//! folded apart and then merged reads the same, register by register.
//!
//! Stated once, generic over a schema, and run over the todo schema and the
//! review schema of `review.rs`: the law asks nothing of a schema, so neither
//! may break it.

use std::collections::{BTreeMap, BTreeSet};

use prodrome::change::mk_change;
use prodrome::dag::Dag;
use prodrome::event::{
    mk_cancelled, mk_completed, mk_reopened, mk_spec_revised, mk_tended, seal_hash, Envelope, Hash,
    TodoEvent,
};
use prodrome::fold::{read, Product};
use prodrome::fpl::mk_flat;
use prodrome::genesis::mk_genesis;
use prodrome::literal::Datetime;
use prodrome::policy::Everything;
use prodrome::reference::Todo;
use prodrome::registers::{extend, fold, Folded, Genesis, Node};
use prodrome::schema::Schema;
use proptest::prelude::*;

use crate::review::{self, Phase, Review, PHASES};

/// What one history is drawn as: each event with the earlier changes to its
/// entity it supersedes (a bit per earlier change), a rank per object for
/// the order it is folded in, the objects folded twice, and the objects one
/// replica starts from.
#[derive(Debug, Clone)]
struct Draw<E> {
    events: Vec<(E, u64)>,
    ranks: Vec<u32>,
    twice: Vec<usize>,
    replica: u64,
}

/// The objects: a genesis, and one change per event over the earlier
/// changes to its entity its bits name.
fn history<E: Schema>(events: &[(E, u64)]) -> Vec<(Hash, Envelope<E>)> {
    let genesis = Envelope::Genesis(mk_genesis("law 1", &"0".repeat(32)).expect("a genesis"));
    let root = seal_hash(&genesis);
    let mut objects = vec![(root.clone(), genesis)];
    let mut changes: Vec<(Hash, &E)> = Vec::new();
    for (event, bits) in events {
        let deps: BTreeSet<Hash> = changes
            .iter()
            .enumerate()
            .filter(|(i, (_, earlier))| bits >> (i % 64) & 1 == 1 && earlier.key() == event.key())
            .map(|(_, (name, _))| name.clone())
            .collect();
        let change = mk_change(root.clone(), deps.into_iter().collect(), event.clone())
            .expect("distinct deps");
        let object = Envelope::Change(change);
        let name = seal_hash(&object);
        changes.push((name.clone(), event));
        objects.push((name, object));
    }
    objects
}

/// Every register's frontier, by name, for every entity: the reading the
/// law is about, compared by object names because a fold's positions are
/// its own business.
type Reading<E> = BTreeMap<(Genesis, <E as Schema>::Key), Vec<Vec<Hash>>>;

fn reading<E: Schema>(state: &Folded<E>) -> Reading<E> {
    let mut out = BTreeMap::new();
    for (genesis, prodrome) in state.prodromes() {
        for (key, stream) in prodrome {
            let registers = read(stream, None, &Everything);
            let frontiers = E::REGISTERS
                .iter()
                .map(|register| registers.frontier(*register).names())
                .collect();
            out.insert((genesis.clone(), key.clone()), frontiers);
        }
    }
    out
}

/// `nodes` in the order their ranks give, parents first.
fn ordered<E: Schema>(nodes: &[Node<E>], ranks: &[u32]) -> Vec<Node<E>> {
    let mut placed: BTreeSet<&Hash> = BTreeSet::new();
    let mut left: Vec<(u32, &Node<E>)> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (ranks.get(i).copied().unwrap_or(0), node))
        .collect();
    let mut out = Vec::with_capacity(nodes.len());
    while !left.is_empty() {
        let ready = left
            .iter()
            .enumerate()
            .filter(|(_, (_, node))| {
                node.parents
                    .iter()
                    .all(|p| placed.contains(p) || !nodes.iter().any(|n| &n.name == p))
            })
            .min_by_key(|(_, (rank, node))| (*rank, &node.name))
            .map(|(i, _)| i)
            .expect("a DAG always has a ready node");
        let (_, node) = left.remove(ready);
        placed.insert(&node.name);
        out.push(node.clone());
    }
    out
}

/// Law 1 at one draw.
fn free<E: Schema>(draw: &Draw<E>) -> Result<(), TestCaseError> {
    let dag: Dag<E> = history(&draw.events).into_iter().collect();
    let nodes = dag.nodes().expect("a DAG");
    let expected = reading(&fold(&nodes));

    let permuted = ordered(&nodes, &draw.ranks);
    prop_assert_eq!(reading(&fold(&permuted)), expected.clone(), "permuted");

    let mut doubled = permuted.clone();
    for i in &draw.twice {
        let node = permuted[i % permuted.len()].clone();
        let after = doubled
            .iter()
            .position(|held| held.name == node.name)
            .expect("held");
        let at = (after + 1 + i % 3).min(doubled.len());
        doubled.insert(at, node);
    }
    prop_assert_eq!(reading(&fold(&doubled)), expected.clone(), "duplicated");

    let seeds = |mine: bool| {
        nodes
            .iter()
            .enumerate()
            .filter(move |(i, _)| (draw.replica >> (i % 64) & 1 == 1) == mine)
            .map(|(_, node)| node.name.clone())
    };
    let (here, there) = (dag.closure(seeds(true)), dag.closure(seeds(false)));
    let within = |held: &BTreeSet<Hash>| -> Vec<Node<E>> {
        permuted
            .iter()
            .filter(|node| held.contains(&node.name))
            .cloned()
            .collect()
    };
    let merged = extend(
        &fold(&within(&here)),
        &ordered(&within(&there), &draw.ranks),
    );
    prop_assert_eq!(reading(&merged), expected, "partitioned, then merged");
    Ok(())
}

fn at(seconds: u32) -> Datetime {
    Datetime::new(2026, 10, 1 + seconds / 86_400, 0, 0, 0, 0).expect("a real instant")
}

fn a_todo_event() -> impl Strategy<Value = TodoEvent<Todo>> {
    (0..2usize, 0..5u8, 0u32..5 * 86_400, 0..3u8).prop_map(|(todo, kind, seconds, note)| {
        let (todo, at, note) = (["alpha", "beta"][todo], at(seconds), format!("n{note}"));
        match kind {
            0 => mk_completed(todo, at, "ana", &note),
            1 => mk_cancelled(todo, at, "ana", &note),
            2 => mk_reopened(todo, at, "ana", &note),
            3 => mk_tended(todo, at, "ana", &note),
            _ => mk_spec_revised(todo, at, "ana", mk_flat(0.5).expect("valid"), &note),
        }
        .expect("valid")
    })
}

fn a_review_event() -> impl Strategy<Value = Review> {
    (
        0..2usize,
        prop::option::of(prop::sample::select(PHASES.to_vec())),
        0u32..5,
    )
        .prop_map(|(pr, phase, day): (usize, Option<Phase>, u32)| {
            let pr = ["pr-1", "pr-2"][pr];
            match phase {
                Some(phase) => review::moved(pr, review::day(1 + day), "ana", phase),
                None => review::opened(pr, review::day(1 + day), "ana", "a change"),
            }
        })
}

fn a_draw<E: Schema>(event: impl Strategy<Value = E>) -> impl Strategy<Value = Draw<E>> {
    (
        prop::collection::vec((event, any::<u64>()), 1..12),
        prop::collection::vec(any::<u32>(), 13),
        prop::collection::vec(0usize..13, 0..4),
        any::<u64>(),
    )
        .prop_map(|(events, ranks, twice, replica)| Draw {
            events,
            ranks,
            twice,
            replica,
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn a_todo_reading_is_a_function_of_the_object_set(draw in a_draw(a_todo_event())) {
        free(&draw)?;
    }

    #[test]
    fn a_review_reading_is_a_function_of_the_object_set(draw in a_draw(a_review_event())) {
        free(&draw)?;
    }
}
