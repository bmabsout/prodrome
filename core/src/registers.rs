//! §6.6 — registers over the DAG: the fold as a monoid action, conflicts as
//! values.
//!
//! See ../../SPEC.md; implemented against ../../conformance/dag.json and
//! ported from `suzatary/prodrome/registers.py`.
//!
//! Every fold in [`crate::fold`] is a last-writer-wins map keyed by todo, and
//! over a chain "last" is well defined. Over a DAG two writes to one register
//! can be CONCURRENT — neither descends from the other — and no clock and no
//! hash order should pick between them. So a register here holds its
//! [`Frontier`]: the writes that no later write has superseded, where "later"
//! means "descends from" and nothing else. One write in the frontier is a
//! value, two or more are a CONFLICT, and the next write that descends from
//! all of them settles it.
//!
//! THE FOLD IS A MONOID ACTION. [`extend`] applies nodes to a state,
//! `fold(nodes) == extend(EMPTY, nodes)`, and
//! `extend(extend(s, xs), ys) == extend(s, xs ++ ys)` — which is what lets a
//! consumer keep the state for a tip and apply only the objects since, instead
//! of refolding the store per request.
//!
//! Ancestry is a [`BitSet`] per object, one bit per topological position, so
//! "x descends from y" is one lookup. It costs O(n) bits per object and O(n²)
//! for the store — fine at thousands of objects, and the thing to replace
//! (interval labels, or a frontier-only index) if the DAG ever reaches tens of
//! thousands. That is a STATED limit, not a hidden one.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::event::{Authored, Envelope, Hash, TodoEvent, TodoId};
use crate::fold::{Binding, Env, Untrusted};
use crate::fpl::{self, Term};
use crate::literal::Datetime;

/// A set of small non-negative integers as a bitmap.
///
/// Hand-written over `Vec<u64>` rather than taken from a big-integer crate:
/// the operations an ancestry needs are exactly three — set a bit, test a bit,
/// union in a parent's set — and none of them wants arithmetic. A dependency
/// that also offered multiplication would be a larger surface for a smaller
/// job, and the growth policy (a word at a time, never shrinking) is the whole
/// implementation.
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

    /// The positions set, ascending — for a reader that wants to name them.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().enumerate().flat_map(|(index, word)| {
            (0..64).filter_map(move |bit| (word >> bit & 1 == 1).then_some(index * 64 + bit))
        })
    }
}

/// What the fold reads of a stored object: its name, its parents, and the
/// event it carries — `None` for a merge, which is STRUCTURE and not a write.
///
/// A struct rather than a trait: there is exactly one shape of stored object,
/// the store hands it over as a `(Hash, Envelope)` pair, and a trait here would
/// be a seam with one implementation on either side of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: Hash,
    pub parents: Vec<Hash>,
    pub event: Option<TodoEvent>,
}

impl Node {
    pub fn of(name: Hash, envelope: &Envelope) -> Node {
        Node {
            name,
            parents: crate::event::parents_of(envelope),
            event: envelope.event().cloned(),
        }
    }
}

/// The store's read, as nodes — `EventStore::read_dag_named`'s output in the
/// shape [`extend`] takes, in the linearisation's order (parents first).
pub fn nodes_of(objects: &[(Hash, Envelope)]) -> Vec<Node> {
    objects
        .iter()
        .map(|(name, envelope)| Node::of(name.clone(), envelope))
        .collect()
}

/// WHICH register a write is about. Three, because three folds read them:
/// lifecycle, price, content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    State,
    Spec,
    Content,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::State => "state",
            Kind::Spec => "spec",
            Kind::Content => "content",
        }
    }
}

/// WHICH register: a kind and a todo. Ordered, so a `Folded` is one value with
/// one comparison and the monoid law is an equality.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key(pub Kind, pub TodoId);

/// One write to one register: the object that made it and the event it
/// carried. The object's NAME is what makes a conflict reportable — a person
/// settling one has to be able to look the write up.
///
/// The event is SHARED, not owned. One `Authored` record is a whole todo — two
/// markup fields, a checklist, a spec TERM TREE — and a write is copied at
/// least twice on the way into a frontier (once for the write, once per
/// register kind it writes) and again on every `extend`, which clones the
/// state it extends. Deep-copying 250 of those per fold was measured at ~1 ms
/// of a 1.5 ms fold on the live chain (2026-09-07). An `Arc` makes the copies
/// a refcount bump and changes nothing observable: the event is immutable, and
/// `PartialEq` on an `Arc` is `PartialEq` on what it holds, so the monoid-action
/// law is still an equality of values.
#[derive(Debug, Clone, PartialEq)]
pub struct Write {
    pub at: Hash,
    pub event: Arc<TodoEvent>,
}

/// The writes to one register that no later write descends from.
///
/// NON-EMPTY by construction, and sorted by object name so that two states
/// that hold the same writes are the same value and a conflict's order carries
/// no meaning. Emptiness is unrepresentable rather than checked: a register
/// with no writes is an ABSENT KEY in [`Folded::frontiers`], so "no writes"
/// and "a frontier that happens to be empty" cannot both exist to be confused.
#[derive(Debug, Clone, PartialEq)]
pub struct Frontier(Vec<Write>);

impl Frontier {
    /// `kept` (the writes this one did not supersede) together with `write`.
    /// Takes the new write by value, which is what makes the result non-empty
    /// without a check.
    fn joining(kept: Vec<Write>, write: Write) -> Frontier {
        let mut writes = kept;
        writes.push(write);
        writes.sort_by(|a, b| a.at.as_str().cmp(b.at.as_str()));
        Frontier(writes)
    }

    pub fn writes(&self) -> &[Write] {
        &self.0
    }

    /// One write is a value; more is a conflict.
    pub fn is_conflict(&self) -> bool {
        self.0.len() > 1
    }
}

/// Which registers an event writes.
///
/// An `Authored` record writes its todo's content, and its spec when it
/// carries one; the lifecycle kinds write the state; `SpecRevised` writes the
/// spec. `Created` writes nothing a register holds — it is the todo's birth,
/// folded elsewhere.
pub fn writes_of(event: &TodoEvent) -> &'static [Kind] {
    match event {
        TodoEvent::Authored(authored) if authored.spec.is_some() => &[Kind::Content, Kind::Spec],
        TodoEvent::Authored(_) => &[Kind::Content],
        TodoEvent::Completed(_) | TodoEvent::Cancelled(_) | TodoEvent::Reopened(_) => {
            &[Kind::State]
        }
        TodoEvent::SpecRevised(_) => &[Kind::Spec],
        TodoEvent::Created(_) => &[],
    }
}

/// The state the fold carries. Compared by value, so the monoid-action law is
/// an equality. `position` and `ancestry` are the STRUCTURE — every object,
/// event or not — and `frontiers` are the registers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Folded {
    position: BTreeMap<Hash, usize>,
    ancestry: BTreeMap<Hash, BitSet>,
    frontiers: BTreeMap<Key, Frontier>,
}

impl Folded {
    /// The empty state — `fold`'s unit.
    pub fn empty() -> Folded {
        Folded::default()
    }

    /// Is `earlier` among `later`'s ancestors? False for a name this state has
    /// never folded, which is the honest answer: it knows of no such object.
    pub fn descends(&self, later: &Hash, earlier: &Hash) -> bool {
        match (self.position.get(earlier), self.ancestry.get(later)) {
            (Some(position), Some(ancestry)) => ancestry.contains(*position),
            _ => false,
        }
    }

    /// The frontier of one register, or `None` where nothing has written it.
    pub fn frontier(&self, kind: Kind, todo: &TodoId) -> Option<&Frontier> {
        self.frontiers.get(&Key(kind, todo.clone()))
    }

    pub fn frontiers(&self) -> &BTreeMap<Key, Frontier> {
        &self.frontiers
    }

    pub fn position(&self) -> &BTreeMap<Hash, usize> {
        &self.position
    }

    pub fn holds(&self, name: &Hash) -> bool {
        self.position.contains_key(name)
    }

    /// The write LATEST IN THE LINEARISATION — the same write the event folds
    /// pick, which is why the registers and the folds never disagree. It is
    /// deterministic and clock-free, and arbitrary between concurrent writes,
    /// which is why [`conflicts_of`] exists beside it: the choice is shown,
    /// never hidden.
    fn chosen<'a>(&self, frontier: &'a Frontier) -> &'a Write {
        frontier
            .0
            .iter()
            .max_by_key(|write| self.position[&write.at])
            .expect("a Frontier is non-empty by construction")
    }
}

/// Apply `nodes` (in topological order, parents first) to `state`.
///
/// Structure is ALWAYS folded — an object is positioned and its ancestry
/// recorded whether or not its event writes anything — so that a later
/// object's "descends" is answerable. An event writes its registers when it is
/// known by `t` (`None`: all of them) and binds under the trust rule;
/// otherwise it is skipped exactly as `chronological` and the folds skip it.
/// A write supersedes every frontier member it descends from and joins the
/// rest.
pub fn extend(
    state: &Folded,
    nodes: &[Node],
    t: Option<Datetime>,
    untrusted: &Untrusted,
) -> Folded {
    let mut position = state.position.clone();
    let mut ancestry = state.ancestry.clone();
    let mut frontiers = state.frontiers.clone();
    for node in nodes {
        if position.contains_key(&node.name) {
            // Already folded: extending with a prefix is a no-op.
            continue;
        }
        let mut bits = BitSet::default();
        for parent in &node.parents {
            if let Some(index) = position.get(parent) {
                bits.insert(*index);
                if let Some(inherited) = ancestry.get(parent) {
                    bits.union_with(inherited);
                }
            }
        }
        position.insert(node.name.clone(), position.len());
        ancestry.insert(node.name.clone(), bits.clone());

        let Some(event) = &node.event else { continue };
        if t.is_some_and(|moment| event.at() > moment) {
            continue;
        }
        if !untrusted.binds(event) {
            continue;
        }
        let write = Write {
            at: node.name.clone(),
            event: Arc::new(event.clone()),
        };
        for kind in writes_of(event) {
            let key = Key(*kind, event.todo().clone());
            let kept: Vec<Write> = frontiers.get(&key).map_or_else(Vec::new, |frontier| {
                frontier
                    .0
                    .iter()
                    .filter(|held| !bits.contains(position[&held.at]))
                    .cloned()
                    .collect()
            });
            frontiers.insert(key, Frontier::joining(kept, write.clone()));
        }
    }
    Folded {
        position,
        ancestry,
        frontiers,
    }
}

/// `extend` from the empty state — the monoid action at its unit.
pub fn fold(nodes: &[Node], t: Option<Datetime>, untrusted: &Untrusted) -> Folded {
    extend(&Folded::empty(), nodes, t, untrusted)
}

/// The nodes `state` has not folded yet, in their given order — what [`extend`]
/// will actually apply. The incremental step's input.
pub fn since<'a>(state: &Folded, nodes: &'a [Node]) -> Vec<&'a Node> {
    nodes
        .iter()
        .filter(|node| !state.holds(&node.name))
        .collect()
}

// --- Projections: what each consumer reads off the frontiers -----------------

/// The lifecycle bindings — [`crate::fold::env_at`]'s answer, from the state
/// registers. A chosen `Reopened` means the todo is open, which is an ABSENT
/// key and not a third binding.
pub fn env_of(state: &Folded) -> Env {
    let mut env = Env::new();
    for (Key(kind, todo), frontier) in &state.frontiers {
        if *kind != Kind::State {
            continue;
        }
        match state.chosen(frontier).event.as_ref() {
            TodoEvent::Completed(e) => {
                env.insert(todo.clone(), Binding::Completed(fpl::instant_of(e.at)));
            }
            TodoEvent::Cancelled(e) => {
                env.insert(todo.clone(), Binding::Cancelled(fpl::instant_of(e.at)));
            }
            _ => {}
        }
    }
    env
}

/// The prices — [`crate::fold::specs_at`]'s answer.
pub fn specs_of(state: &Folded) -> BTreeMap<TodoId, Term> {
    let mut out = BTreeMap::new();
    for (Key(kind, todo), frontier) in &state.frontiers {
        if *kind != Kind::Spec {
            continue;
        }
        let spec = match state.chosen(frontier).event.as_ref() {
            TodoEvent::SpecRevised(e) => Some(e.spec.clone()),
            TodoEvent::Authored(e) => e.spec.clone(),
            _ => None,
        };
        if let Some(spec) = spec {
            out.insert(todo.clone(), spec);
        }
    }
    out
}

/// The content — [`crate::fold::authored_at`]'s answer.
pub fn content_of(state: &Folded) -> BTreeMap<TodoId, Authored> {
    let mut out = BTreeMap::new();
    for (Key(kind, todo), frontier) in &state.frontiers {
        if *kind != Kind::Content {
            continue;
        }
        if let TodoEvent::Authored(authored) = state.chosen(frontier).event.as_ref() {
            out.insert(todo.clone(), (**authored).clone());
        }
    }
    out
}

/// The object whose write each register of `kind` currently shows — the
/// projection's CHOICE, by name. A reader holding the objects looks the event
/// up by this instead of taking a reprint of it across a boundary: a content
/// record is a whole todo body, and 184 of them printed per fold was 1.7 ms
/// of a 12 ms day-fold (2026-09-06).
pub fn chosen_of(state: &Folded, kind: Kind) -> BTreeMap<TodoId, Hash> {
    let mut out = BTreeMap::new();
    for (Key(held, todo), frontier) in &state.frontiers {
        if *held == kind {
            out.insert(todo.clone(), state.chosen(frontier).at.clone());
        }
    }
    out
}

/// Every register with more than one write in its frontier, by todo: the
/// writes a human has to settle, each named by the object that made it.
pub fn conflicts_of(state: &Folded) -> BTreeMap<TodoId, BTreeMap<Kind, Frontier>> {
    let mut out: BTreeMap<TodoId, BTreeMap<Kind, Frontier>> = BTreeMap::new();
    for (Key(kind, todo), frontier) in &state.frontiers {
        if frontier.is_conflict() {
            out.entry(todo.clone())
                .or_default()
                .insert(*kind, frontier.clone());
        }
    }
    out
}

/// The registers a state has any opinion about — what a reader enumerating
/// todos asks, instead of unioning three projections.
pub fn registers_of(state: &Folded) -> BTreeSet<&Key> {
    state.frontiers.keys().collect()
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
        // A union with a shorter set keeps what the longer one held.
        let mut shorter = BitSet::default();
        shorter.insert(1);
        other.union_with(&shorter);
        assert_eq!(other.iter().collect::<Vec<_>>(), vec![0, 1, 64, 130]);
    }

    #[test]
    fn a_created_event_writes_no_register() {
        let at = Datetime::new(2026, 9, 6, 0, 0, 0, 0).expect("a real instant");
        let created = crate::event::mk_created("alpha", at, "bassel", "", "").expect("valid");
        assert!(writes_of(&created).is_empty());
        let completed = crate::event::mk_completed("alpha", at, "bassel", "").expect("valid");
        assert_eq!(writes_of(&completed), &[Kind::State]);
    }
}
