//! A store's memory (design §6.1, stage 7): what a store has read, held and
//! extended by the objects it has not, reads exactly as the same objects
//! read cold.
//!
//! The fold first, with no store in the way: [`Folded::insert`] in ANY order
//! that puts parents first is `fold` of the union, so the state a store holds
//! is a function of its objects and not of the order they reached it.

use crate::common;

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use prodrome::event::{Actor, Hash, TodoEvent};
use prodrome::policy::Untrusted;
use prodrome::reference::Todo;
use prodrome::registers::{fold, Folded, Node};
use prodrome::store::EventStore;
use proptest::prelude::*;

use common::{seal, two_writers};

type Event = TodoEvent<Todo>;
type Store = EventStore<Event, Untrusted>;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A directory of stores, removed when the case ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(what: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!(
            "prodrome-memory-{what}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        Scratch(root)
    }

    fn store(&self, name: &str) -> Store {
        Store::new(self.0.join(name), roster())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn roster() -> Untrusted {
    Untrusted::of([Actor::new("triage").expect("valid")])
}

/// Two writers' changes over one genesis: a shared log appended, then each
/// side's own on its own replica, then each side's tips adopted by the other.
fn changes(
    scratch: &Scratch,
    (shared, mine, theirs): &(Vec<Event>, Vec<Event>, Vec<Event>),
) -> Store {
    let here = scratch.store("here");
    here.init("memory").expect("begins");
    for event in shared {
        here.append(event.clone()).expect("appends");
    }
    let there = scratch.store("there");
    for tip in here.tips().expect("the tips derive") {
        there.adopt(&here, &tip).expect("adopts");
    }
    for event in mine {
        here.append(event.clone()).expect("appends");
    }
    for event in theirs {
        there.append(event.clone()).expect("appends");
    }
    for tip in there.tips().expect("the tips derive") {
        here.adopt(&there, &tip).expect("adopts");
    }
    here
}

/// The same logs as legacy objects: each sealed on every tip of its writer's
/// replica, so the second head is a `Woven` once both are adopted.
fn sealed(
    scratch: &Scratch,
    (shared, mine, theirs): &(Vec<Event>, Vec<Event>, Vec<Event>),
) -> Store {
    let here = scratch.store("here");
    for event in shared {
        seal(&here, event.clone());
    }
    let there = scratch.store("there");
    for tip in here.tips().expect("the tips derive") {
        there.adopt(&here, &tip).expect("adopts");
    }
    for event in mine {
        seal(&here, event.clone());
    }
    for event in theirs {
        seal(&there, event.clone());
    }
    for tip in there.tips().expect("the tips derive") {
        here.adopt(&there, &tip).expect("adopts");
    }
    here
}

/// `nodes` in the topological order `keys` draws: of the objects whose
/// parents are all placed, the one with the least key, then name.
fn shuffled(nodes: &[Node<Event>], keys: &[u32]) -> Vec<Node<Event>> {
    let names: BTreeSet<&Hash> = nodes.iter().map(|node| &node.name).collect();
    let mut placed: BTreeSet<&Hash> = BTreeSet::new();
    let mut left: Vec<(u32, &Node<Event>)> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (keys.get(i).copied().unwrap_or(0), node))
        .collect();
    let mut out = Vec::with_capacity(nodes.len());
    while !left.is_empty() {
        let ready = left
            .iter()
            .enumerate()
            .filter(|(_, (_, node))| {
                node.parents
                    .iter()
                    .all(|p| placed.contains(p) || !names.contains(p))
            })
            .min_by_key(|(_, (key, node))| (*key, &node.name))
            .map(|(i, _)| i)
            .expect("a DAG always has a ready node");
        let (_, node) = left.remove(ready);
        placed.insert(&node.name);
        out.push(node.clone());
    }
    out
}

/// The first `split` objects of `order` folded cold, in the linearisation's
/// order, and the rest inserted in `order`'s.
fn inserted(nodes: &[Node<Event>], order: &[Node<Event>], split: usize) -> Folded<Event> {
    let split = split.min(order.len());
    let first: BTreeSet<&Hash> = order[..split].iter().map(|node| &node.name).collect();
    let prefix: Vec<Node<Event>> = nodes
        .iter()
        .filter(|node| first.contains(&node.name))
        .cloned()
        .collect();
    let mut state = fold(&prefix);
    for node in &order[split..] {
        state
            .insert(node)
            .expect("every object rests within its scope");
    }
    state
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// THE FOLD IS AN ACTION OF THE SET. Over two writers' changes and over
    /// the same logs as legacy objects, a down-set folded cold and the rest
    /// inserted in any order that puts parents first is the fold of the
    /// whole; inserting what is held changes nothing.
    #[test]
    fn inserting_in_any_order_folds_as_the_union(
        logs in two_writers(),
        keys in prop::collection::vec(0u32..1000, 0..60),
        split in 0usize..60,
    ) {
        let scratch = Scratch::new("insert");
        for store in [changes(&scratch, &logs), sealed(&Scratch::new("insert"), &logs)] {
            let nodes = store.dag().and_then(|dag| dag.nodes()).expect("reads");
            let whole = fold(&nodes);
            prop_assert!(whole.local(), "an append's store rests within its scopes");
            let order = shuffled(&nodes, &keys);
            let mut state = inserted(&nodes, &order, split);
            prop_assert_eq!(&state, &whole);
            for node in &nodes {
                state.insert(node).expect("held");
            }
            prop_assert_eq!(&state, &whole);
        }
    }
}
