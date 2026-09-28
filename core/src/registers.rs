//! §6.6 — the DAG as the registers read it: each object's place and ancestry,
//! and each todo's stream of writes.
//!
//! See ../../SPEC.md and Draft A. [`crate::fold`] folds a stream into its
//! registers; this module only says what descends from what.
//!
//! THE FOLD IS A MONOID ACTION. `fold(nodes) == extend(EMPTY, nodes)` and
//! `extend(extend(s, xs), ys) == extend(s, xs ++ ys)`, so a consumer keeps the
//! state for a tip and applies only the objects since.
//!
//! Ancestry is a [`BitSet`] over positions. A legacy object is placed among
//! every legacy object, O(n²) bits for the store; a change's deps never leave
//! its todo, so a change is placed within its `(genesis, todo)`, O(Σ kᵢ²).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::event::{Envelope, Hash, TodoEvent, TodoId};
use crate::fold::{Registers, Write};
use crate::payload::Payload;
use crate::policy::Everything;

/// The state the fold carries, compared by value.
#[derive(Debug, Clone, PartialEq)]
pub struct Folded<P> {
    index: BTreeMap<Hash, Place>,
    legacy: usize,
    attestations: BTreeSet<Hash>,
    prodromes: BTreeMap<Genesis, Prodrome<P>>,
}

/// Which prodrome: a genesis object's name, or `None` for the legacy one.
pub type Genesis = Option<Hash>;

/// One prodrome's todos, each its stream of writes in causal order.
pub type Prodrome<P> = BTreeMap<TodoId, Vec<Stamp<P>>>;

/// One object that carries an event, where its todo's registers can see it.
#[derive(Debug, Clone, PartialEq)]
pub struct Stamp<P> {
    pub name: Hash,
    pub event: Arc<TodoEvent<P>>,
    place: Place,
}

impl<P> Stamp<P> {
    /// Does this write descend from `earlier`, a write to the same todo?
    pub fn descends(&self, earlier: &Stamp<P>) -> bool {
        self.place.ancestry.contains(earlier.place.position)
    }
}

/// A position in an index, and the positions of every ancestor in it.
#[derive(Debug, Clone, PartialEq)]
struct Place {
    position: usize,
    ancestry: BitSet,
    scope: Option<(Hash, TodoId)>,
}

/// What the fold reads of a stored object. `genesis` is `None` for a legacy
/// object, and for a change in the legacy prodrome.
#[derive(Debug, Clone, PartialEq)]
pub struct Node<P> {
    pub name: Hash,
    pub parents: Vec<Hash>,
    pub event: Option<TodoEvent<P>>,
    pub genesis: Genesis,
}

impl<P: Payload> Node<P> {
    pub fn of(name: Hash, envelope: &Envelope<P>) -> Node<P> {
        let genesis = match envelope {
            Envelope::Sealed { .. } | Envelope::Woven { .. } => None,
            Envelope::Genesis(_) => Some(name.clone()),
            Envelope::Change(change) => Some(change.genesis.clone()),
            Envelope::Snapshot(snapshot) => Some(snapshot.genesis.clone()),
        };
        Node {
            parents: crate::event::parents_of(envelope),
            event: envelope.event().cloned(),
            name,
            genesis,
        }
    }
}

/// The store's read as nodes, in the linearisation's order. A change naming
/// a legacy root is in the legacy prodrome.
pub fn nodes_of<P: Payload>(objects: &[(Hash, Envelope<P>)]) -> Vec<Node<P>> {
    let roots: BTreeSet<&Hash> = objects
        .iter()
        .filter(|(_, envelope)| matches!(envelope, Envelope::Sealed { prev: None, .. }))
        .map(|(name, _)| name)
        .collect();
    objects
        .iter()
        .map(|(name, envelope)| {
            let mut node = Node::of(name.clone(), envelope);
            if node.genesis.as_ref().is_some_and(|g| roots.contains(g)) {
                node.genesis = None;
            }
            node
        })
        .collect()
}

impl<P: Payload> Folded<P> {
    pub fn empty() -> Folded<P> {
        Folded {
            index: BTreeMap::new(),
            legacy: 0,
            attestations: BTreeSet::new(),
            prodromes: BTreeMap::new(),
        }
    }

    pub fn holds(&self, name: &Hash) -> bool {
        self.index.contains_key(name) || self.attestations.contains(name)
    }

    /// Is `earlier` among `later`'s ancestors in one index? False for a name
    /// this state has never placed.
    pub fn descends(&self, later: &Hash, earlier: &Hash) -> bool {
        match (self.index.get(later), self.index.get(earlier)) {
            (Some(later), Some(earlier)) => {
                later.scope == earlier.scope && later.ancestry.contains(earlier.position)
            }
            _ => false,
        }
    }

    pub fn prodromes(&self) -> &BTreeMap<Genesis, Prodrome<P>> {
        &self.prodromes
    }

    /// Every prodrome's todos, for a reader keyed by todo id alone.
    pub fn todos(&self) -> impl Iterator<Item = (&TodoId, &[Stamp<P>])> {
        self.prodromes
            .values()
            .flatten()
            .map(|(todo, stream)| (todo, stream.as_slice()))
    }

    fn place(&mut self, node: &Node<P>) -> Option<Place> {
        let scope = match (&node.genesis, &node.event) {
            (None, _) => None,
            (Some(genesis), Some(event)) => Some((genesis.clone(), event.todo().clone())),
            (Some(_), None) => return None,
        };
        let mut ancestry = BitSet::default();
        for parent in node.parents.iter().filter_map(|p| self.index.get(p)) {
            if parent.scope == scope {
                ancestry.insert(parent.position);
                ancestry.union_with(&parent.ancestry);
            }
        }
        let position = match &scope {
            None => {
                self.legacy += 1;
                self.legacy - 1
            }
            Some((genesis, todo)) => self
                .prodromes
                .get(&Some(genesis.clone()))
                .and_then(|todos| todos.get(todo))
                .map_or(0, Vec::len),
        };
        Some(Place {
            position,
            ancestry,
            scope,
        })
    }
}

/// Apply `nodes` (parents first) to `state`. Structure is folded for every
/// object; standing and time are the readers' business.
pub fn extend<P: Payload>(state: &Folded<P>, nodes: &[Node<P>]) -> Folded<P> {
    let mut next = state.clone();
    for node in nodes {
        if next.holds(&node.name) {
            continue;
        }
        let Some(place) = next.place(node) else {
            next.attestations.insert(node.name.clone());
            continue;
        };
        next.index.insert(node.name.clone(), place.clone());
        if let Some(event) = &node.event {
            next.prodromes
                .entry(node.genesis.clone())
                .or_default()
                .entry(event.todo().clone())
                .or_default()
                .push(Stamp {
                    name: node.name.clone(),
                    event: Arc::new(event.clone()),
                    place,
                });
        }
    }
    next
}

pub fn fold<P: Payload>(nodes: &[Node<P>]) -> Folded<P> {
    extend(&Folded::empty(), nodes)
}

/// The nodes `state` has not folded yet, in their given order.
pub fn since<'a, P: Payload>(state: &Folded<P>, nodes: &'a [Node<P>]) -> Vec<&'a Node<P>> {
    nodes
        .iter()
        .filter(|node| !state.holds(&node.name))
        .collect()
}

/// A change's deps: the union of the frontiers of the registers `event`
/// writes, read structurally — everything binds, at no moment.
pub fn deps_for<P: Payload>(
    state: &Folded<P>,
    genesis: &Genesis,
    event: &TodoEvent<P>,
) -> Vec<Hash> {
    let Some(stream) = state
        .prodromes
        .get(genesis)
        .and_then(|todos| todos.get(event.todo()))
    else {
        return Vec::new();
    };
    let registers = Registers::read(stream, None, &Everything);
    let deps: BTreeSet<&Hash> = Write::of(event)
        .filter_map(|write| write.kind())
        .flat_map(|kind| registers.frontier(kind).writes())
        .map(|stamp| &stamp.name)
        .collect();
    deps.into_iter().cloned().collect()
}

/// A set of small non-negative integers as a bitmap: set, test, union.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BitSet(Vec<u64>);

impl BitSet {
    pub fn contains(&self, bit: usize) -> bool {
        self.0
            .get(bit / 64)
            .is_some_and(|word| word >> (bit % 64) & 1 == 1)
    }

    pub fn insert(&mut self, bit: usize) {
        let word = bit / 64;
        if self.0.len() <= word {
            self.0.resize(word + 1, 0);
        }
        self.0[word] |= 1 << (bit % 64);
    }

    pub fn union_with(&mut self, other: &BitSet) {
        if self.0.len() < other.0.len() {
            self.0.resize(other.0.len(), 0);
        }
        for (mine, theirs) in self.0.iter_mut().zip(&other.0) {
            *mine |= theirs;
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().enumerate().flat_map(|(index, word)| {
            (0..64).filter_map(move |bit| (word >> bit & 1 == 1).then_some(index * 64 + bit))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bitset_is_a_set_of_positions() {
        let mut bits = BitSet::default();
        assert!(!bits.contains(0));
        bits.insert(0);
        bits.insert(130);
        assert!(bits.contains(0) && bits.contains(130));
        assert!(!bits.contains(1) && !bits.contains(129) && !bits.contains(999));
        assert_eq!(bits.iter().collect::<Vec<_>>(), vec![0, 130]);

        let mut other = BitSet::default();
        other.insert(64);
        other.union_with(&bits);
        assert_eq!(other.iter().collect::<Vec<_>>(), vec![0, 64, 130]);
        let mut shorter = BitSet::default();
        shorter.insert(1);
        other.union_with(&shorter);
        assert_eq!(other.iter().collect::<Vec<_>>(), vec![0, 1, 64, 130]);
    }
}
