//! Design §6.5: a digest, a summary tree over an append-only log of lines,
//! read through a view that fits a budget.
//!
//! A long log (a transcript, a journal) is too big to read whole. Its lines
//! are a set, each resting on the lines its writer had seen, and they are
//! the leaves of a tree ([`tree`]) whose shape is [`memo::balance`] over
//! their names in the crate's one causal order, so the shape is a function
//! of the set and not of when each line arrived: two replicas holding one
//! log build one tree, and a line that
//! arrives late renames one logarithmic spine. A node is named by
//! [`memo::name`] over its children and carries the [`Measure`] of the lines
//! it spans.
//!
//! An inner node may have a SUMMARY: a short text standing for its lines,
//! written elsewhere and recorded by the caller as an object of its own. A
//! summary is an observation, not a function of the lines, so this module
//! never makes or caches one; it reads only which nodes have one and how
//! long it is, `summarized(name)`, and says which could be written next
//! ([`pending`]).
//!
//! A [`View`] is a cut through the tree that tiles the whole log, recent
//! lines fine and old ones coarse, under a byte budget ([`view`]); [`zoom`]
//! opens a part of it into its children. The tree is lossless and only the
//! view is lossy. Everything here is a function of the leaves, the summaries'
//! sizes and the budget: no clock, no model, and nothing about what a line
//! says or who writes a summary.

#![warn(clippy::pedantic)]

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use crate::event::Hash;
use crate::memo::{self, Algebra};
use crate::topo;

/// What every node carries up the tree: a monoid, so a node's measure is
/// the fold of its children's (their count, their tokens, the instants they
/// span). `join` must be associative with `empty` its unit.
pub trait Measure: Clone + Eq {
    /// The measure of no lines.
    fn empty() -> Self;
    /// The measure of `self`'s lines followed by `other`'s.
    #[must_use]
    fn join(&self, other: &Self) -> Self;
}

/// The measure of lines given in order, the monoid's fold: the one fold
/// a chunk and [`Measured`] share.
fn joined<'m, M: Measure + 'm>(measures: impl IntoIterator<Item = &'m M>) -> M {
    measures
        .into_iter()
        .fold(M::empty(), |measure, next| measure.join(next))
}

/// No measure at all.
impl Measure for () {
    fn empty() {}
    fn join(&self, (): &()) {}
}

/// A line of the log: its content name, the lines it rests on, its length
/// in the view, and its measure.
///
/// The name is the line's content, so everything else here is a function
/// of it: two copies of a line are equal, and a tree, a [`read`] key and a
/// [`Measured`] fold, all keyed by names, assume as much.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leaf<M> {
    /// The line's content name.
    pub name: Hash,
    /// The lines it rests on: what its writer had seen. A line given none,
    /// or only lines the log does not hold, rests on nothing here.
    pub parents: Vec<Hash>,
    /// What the line costs in a view.
    pub bytes: u32,
    /// What the line carries up the tree.
    pub measure: M,
}

/// A node of the tree: a line, or a chunk of two to
/// [`memo::balance::WIDEST`] consecutive nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node<M> {
    name: Hash,
    span: Range<usize>,
    level: usize,
    bytes: u64,
    measure: M,
    children: Vec<Node<M>>,
}

impl<M> Node<M> {
    /// Its content name: a line's own, a chunk's [`memo::name`] over its
    /// children's.
    #[must_use]
    pub fn name(&self) -> &Hash {
        &self.name
    }

    /// The lines it spans, as positions in the log.
    #[must_use]
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }

    /// Its height: 0 for a line, one more than its children's for a chunk.
    /// Every line is at one depth, so every child of a node is one level
    /// below it.
    #[must_use]
    pub fn level(&self) -> usize {
        self.level
    }

    /// The bytes of the lines it spans: what it costs opened all the way.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// The fold of its lines' measures.
    #[must_use]
    pub fn measure(&self) -> &M {
        &self.measure
    }

    /// Its children, in the log's order; none for a line.
    #[must_use]
    pub fn children(&self) -> &[Node<M>] {
        &self.children
    }

    /// Whether it is a line.
    #[must_use]
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    fn part(&self) -> Part {
        Part {
            node: self.name.clone(),
            span: self.span(),
        }
    }

    fn find(&self, name: &Hash) -> Option<&Node<M>> {
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            if &node.name == name {
                return Some(node);
            }
            stack.extend(&node.children);
        }
        None
    }
}

impl<M> memo::Tree for Node<M> {
    fn name(&self) -> &Hash {
        &self.name
    }

    fn children(&self) -> impl Iterator<Item = &Self> {
        self.children.iter()
    }
}

/// The summary tree over a nonempty log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree<M> {
    root: Node<M>,
}

impl<M> Tree<M> {
    /// The root, spanning the whole log.
    #[must_use]
    pub fn root(&self) -> &Node<M> {
        &self.root
    }

    /// How many lines the log holds: one or more.
    #[must_use]
    pub fn lines(&self) -> usize {
        self.root.span.end
    }

    /// Whether `node` is SETTLED: it does not hold the last line, so no
    /// line appended renames it. A node that holds the last line is still
    /// growing, and its name, and any summary of it, is the next line's to
    /// replace.
    #[must_use]
    pub fn settled(&self, node: &Node<M>) -> bool {
        node.span.end < self.lines()
    }

    /// The node named `name`, if the tree has one.
    #[must_use]
    pub fn node(&self, name: &Hash) -> Option<&Node<M>> {
        self.root.find(name)
    }
}

/// A log whose lines rest on one another in a cycle, which no order of
/// them puts each after what it rests on: the least line it leaves
/// unplaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cycle(pub Hash);

/// The tree over a set of `leaves`, given in any order and any number of
/// times: the lines in the crate's one causal order ([`topo::linear`]),
/// each after the lines it rests on and lines no order relates by name, so
/// two replicas holding one set of lines build one tree. `None` for no
/// lines.
///
/// # Errors
///
/// [`Cycle`] when the lines rest on one another in a cycle.
pub fn tree<M: Measure>(leaves: Vec<Leaf<M>>) -> Result<Option<Tree<M>>, Cycle> {
    let mut lines: BTreeMap<Hash, Leaf<M>> = BTreeMap::new();
    for leaf in leaves {
        lines.entry(leaf.name.clone()).or_insert(leaf);
    }
    let rests_on: BTreeMap<&Hash, Vec<&Hash>> = lines
        .iter()
        .map(|(name, leaf)| (name, leaf.parents.iter().collect()))
        .collect();
    let order: Vec<Hash> = topo::linear(&rests_on)
        .map_err(|stuck| Cycle(stuck.clone()))?
        .into_iter()
        .cloned()
        .collect();
    let nodes = order
        .into_iter()
        .filter_map(|name| lines.remove(&name))
        .enumerate()
        .map(|(at, leaf)| Node {
            name: leaf.name,
            span: at..at + 1,
            level: 0,
            bytes: u64::from(leaf.bytes),
            measure: leaf.measure,
            children: Vec::new(),
        })
        .collect();
    Ok(memo::balance(nodes, |children: Vec<Node<M>>| Node {
        name: memo::name(b"digest", children.iter().map(Node::name)),
        span: children[0].span.start..children[children.len() - 1].span.end,
        level: children[0].level + 1,
        bytes: children.iter().map(Node::bytes).sum(),
        measure: joined(children.iter().map(Node::measure)),
        children,
    })
    .map(|root| Tree { root }))
}

/// The measure as an algebra, for [`memo::fold`]: a line is its own, a chunk
/// the join of its children's.
#[derive(Debug, Clone)]
pub struct Measured(Hash);

impl Default for Measured {
    fn default() -> Self {
        Measured(memo::name(b"digest measure", []))
    }
}

impl<M: Measure> Algebra<Node<M>> for Measured {
    type Out = M;
    fn name(&self) -> &Hash {
        &self.0
    }
    fn apply(&self, node: &Node<M>, children: Vec<M>) -> M {
        if node.is_leaf() {
            node.measure.clone()
        } else {
            joined(&children)
        }
    }
}

/// The name of the tree as read under its summaries: its root's name and,
/// for every inner node in order, whether it has a summary and how long.
/// [`view`] and [`pending`] are functions of it (and of the budget), so a
/// cache keyed by it is never stale, and a summary recorded moves the key.
#[must_use]
pub fn read<M>(tree: &Tree<M>, summarized: impl Fn(&Hash) -> Option<u32>) -> Hash {
    let mut own = Vec::new();
    let mut stack = vec![&tree.root];
    while let Some(node) = stack.pop() {
        if !node.is_leaf() {
            match summarized(&node.name) {
                Some(bytes) => {
                    own.push(1);
                    own.extend(bytes.to_be_bytes());
                }
                None => own.push(0),
            }
        }
        stack.extend(node.children.iter().rev());
    }
    memo::name(&own, [&tree.root.name])
}

/// A part of a view: a node, standing for the lines it spans.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Part {
    /// The node's content name, its identity.
    pub node: Hash,
    /// The lines it spans.
    pub span: Range<usize>,
}

impl Part {
    /// A handle to show a reader: its first line and how many it spans,
    /// `first+lines`. A position, so it moves when a line arrives before it;
    /// the node's name is what stays.
    #[must_use]
    pub fn handle(&self) -> String {
        format!("{}+{}", self.span.start, self.span.len())
    }
}

/// A cut through the tree under a budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// The parts, in the log's order, tiling it.
    pub parts: Vec<Part>,
    /// What the parts cost: each line's bytes and each summary's.
    pub bytes: u64,
    /// The nodes without a summary the view would have closed to fit its
    /// budget, in the log's order: what to summarise next for this view.
    pub waiting: Vec<Hash>,
}

/// The nodes whose summary can be written now, smallest first and then
/// leftmost: each is settled and has none, and each of its children is a
/// line or has one. A function of which nodes have a summary and nothing
/// else, so summaries of different nodes may be written in any order, or
/// at once.
#[must_use]
pub fn pending<M>(tree: &Tree<M>, summarized: impl Fn(&Hash) -> Option<u32>) -> Vec<Hash> {
    let mut ready = Vec::new();
    let mut stack = vec![&tree.root];
    while let Some(node) = stack.pop() {
        if node.is_leaf() {
            continue;
        }
        if tree.settled(node)
            && summarized(&node.name).is_none()
            && node
                .children
                .iter()
                .all(|child| child.is_leaf() || summarized(&child.name).is_some())
        {
            ready.push(node);
        }
        stack.extend(&node.children);
    }
    ready.sort_by_key(|node| (node.span.len(), node.span.start));
    ready.into_iter().map(|node| node.name.clone()).collect()
}

/// The children of `name` as parts; `None` when the tree has no inner node
/// of that name (a line does not open).
#[must_use]
pub fn zoom<M>(tree: &Tree<M>, name: &Hash) -> Option<Vec<Part>> {
    let node = tree.node(name).filter(|node| !node.is_leaf())?;
    Some(node.children.iter().map(Node::part).collect())
}

/// The view of the log under `budget` bytes.
///
/// It is built as the log was: line by line, each appended as a part of
/// its own, and while the parts cost more than the budget, a node above
/// some parts CLOSES: the parts it spans are replaced by the node, shown as
/// its summary. A node may close only
///
/// - when its summary costs less than the parts it replaces, so a closing
///   always lowers the view's cost and the view never costs more than its
///   lines (a node may close over a child whose own summary would not
///   lower it);
/// - once a line has arrived after it, so it is settled ([`Tree::settled`])
///   and no line to come renames a closed part;
/// - where the view stays DECAYING, its parts' levels never rising toward
///   the present: at the start of the log, or after a part at least as high
///   as itself.
///
/// Of those, the most DUE closes, its due being its age over its size (the
/// lines from its first to the present, over the lines it spans), so an old
/// node closes before a young one and a small one before a large; of two as
/// due, the older. A RUN, a node all of whose children are parts, with no
/// summary is `waiting` when it is more due than the node that closes, or
/// when none can: it is what to summarise next. The view stops over its
/// budget only when no node may close and lower its cost. A closed part is
/// never renamed and never opens as lines arrive, so what a reader was
/// shown of the past stays as it was shown, or coarser.
///
/// A part's cost is its summary's bytes, or a line's own.
#[must_use]
pub fn view<M>(tree: &Tree<M>, summarized: impl Fn(&Hash) -> Option<u32>, budget: u64) -> View {
    let arena = Arena::of(tree);
    let node = |id: usize| arena.nodes[id];
    let cost = |id: usize| -> Option<u64> {
        if node(id).is_leaf() {
            Some(node(id).bytes)
        } else {
            summarized(&node(id).name).map(u64::from)
        }
    };
    // Per node: how many of its children are parts, and what the parts it
    // spans cost. OPEN is every node above a part, none of whose ancestors
    // is one: the nodes that may close.
    let mut parted = vec![0; arena.nodes.len()];
    let mut covered = vec![0u64; arena.nodes.len()];
    let mut open: BTreeSet<usize> = BTreeSet::new();
    let mut waiting: BTreeSet<usize> = BTreeSet::new();
    let mut parts: Vec<usize> = Vec::new();
    let mut bytes = 0u64;
    for (now, &leaf) in arena.leaves.iter().enumerate() {
        let now = now + 1;
        let size = node(leaf).bytes;
        parts.push(leaf);
        bytes += size;
        if let Some(parent) = arena.parent[leaf] {
            parted[parent] += 1;
        }
        for above in arena.ancestors(leaf) {
            covered[above] += size;
            open.insert(above);
        }
        while bytes > budget {
            let first = |id: usize| {
                let start = node(id).span.start;
                parts.partition_point(|&part| node(part).span.start < start)
            };
            let decaying = |id: usize| match first(id) {
                0 => true,
                at => node(parts[at - 1]).level >= node(id).level,
            };
            let mut order: Vec<usize> = open
                .iter()
                .copied()
                .filter(|&id| node(id).span.end < now && decaying(id))
                .collect();
            order.sort_by(|&a, &b| arena.due(b, now).cmp(&arena.due(a, now)).then(a.cmp(&b)));
            let run = |id: usize| parted[id] == node(id).children.len();
            let lowers = |id: usize| cost(id).filter(|&size| size < covered[id]);
            let chosen = order
                .iter()
                .enumerate()
                .find_map(|(at, &id)| lowers(id).map(|size| (at, size)));
            waiting.extend(
                order[..chosen.map_or(order.len(), |(at, _)| at)]
                    .iter()
                    .filter(|&&id| run(id) && cost(id).is_none()),
            );
            let Some((at, size)) = chosen else { break };
            let closing = order[at];
            let span = node(closing).span.clone();
            let from = first(closing);
            let to = from + parts[from..].partition_point(|&part| node(part).span.start < span.end);
            let freed: u64 = parts
                .splice(from..to, [closing])
                .map(|part| cost(part).unwrap_or_default())
                .sum();
            bytes = bytes - freed + size;
            for above in arena.ancestors(closing) {
                covered[above] = covered[above] - freed + size;
            }
            let inside: Vec<usize> = open.range(closing..arena.after[closing]).copied().collect();
            for id in inside {
                open.remove(&id);
            }
            if let Some(parent) = arena.parent[closing] {
                parted[parent] += 1;
            }
        }
    }
    View {
        parts: parts.iter().map(|&id| node(id).part()).collect(),
        bytes,
        // A run closed over since it waited no longer waits.
        waiting: waiting
            .iter()
            .filter(|id| open.contains(id))
            .map(|&id| node(id).name.clone())
            .collect(),
    }
}

/// The tree's nodes numbered in pre-order, which is the log's order by
/// first line and, among nodes starting at one line, outermost first; so a
/// node's descendants are the ids after it, up to `after`.
struct Arena<'t, M> {
    nodes: Vec<&'t Node<M>>,
    parent: Vec<Option<usize>>,
    after: Vec<usize>,
    leaves: Vec<usize>,
}

impl<'t, M> Arena<'t, M> {
    fn of(tree: &'t Tree<M>) -> Self {
        let mut arena = Arena {
            nodes: Vec::new(),
            parent: Vec::new(),
            after: Vec::new(),
            leaves: Vec::new(),
        };
        let mut stack = vec![(&tree.root, None)];
        while let Some((node, parent)) = stack.pop() {
            let id = arena.nodes.len();
            arena.nodes.push(node);
            arena.parent.push(parent);
            if node.is_leaf() {
                arena.leaves.push(id);
            }
            stack.extend(node.children.iter().rev().map(|child| (child, Some(id))));
        }
        // A node's subtree is itself and its children's, which come after it.
        let mut size = vec![1; arena.nodes.len()];
        for id in (1..arena.nodes.len()).rev() {
            if let Some(parent) = arena.parent[id] {
                size[parent] += size[id];
            }
        }
        arena.after = size
            .iter()
            .enumerate()
            .map(|(id, size)| id + size)
            .collect();
        arena
    }

    /// The node's ancestors, its parent first.
    fn ancestors(&self, id: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(self.parent[id], |&id| self.parent[id])
    }

    /// How due a node is at `now` lines: its age over its size.
    fn due(&self, id: usize, now: usize) -> Due {
        let span = &self.nodes[id].span;
        Due {
            age: now - span.start,
            size: span.len(),
        }
    }
}

/// A ratio, compared exactly.
#[derive(Clone, Copy)]
struct Due {
    age: usize,
    size: usize,
}

impl Ord for Due {
    fn cmp(&self, other: &Self) -> Ordering {
        let wide = |n: usize| n as u128;
        (wide(self.age) * wide(other.size)).cmp(&(wide(other.age) * wide(self.size)))
    }
}

impl PartialEq for Due {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Due {}

impl PartialOrd for Due {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
