//! Design §6.5: a digest, a summary tree over an append-only log of lines,
//! read through a view that fits a budget.
//!
//! A long log (a transcript, a journal) is too big to read whole. Its lines
//! are the leaves of a tree ([`tree`]) whose shape is [`memo::balance`] over
//! their names, so the shape is a function of the lines and not of when each
//! arrived: two replicas holding one log build one tree, and a line that
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
use std::collections::BTreeSet;
use std::ops::Range;

use crate::event::Hash;
use crate::memo::{self, Algebra};

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

/// A line of the log: its content name, its length in the view, and its
/// measure. Both are functions of the line, so of its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leaf<M> {
    /// The line's content name.
    pub name: Hash,
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

/// The tree over `leaves`, in the log's causal order as the caller gives
/// it; `None` for none. A line given twice is the line once, at its first
/// place, so the tree is a function of the lines' names in order.
#[must_use]
pub fn tree<M: Measure>(leaves: Vec<Leaf<M>>) -> Option<Tree<M>> {
    let mut seen = BTreeSet::new();
    let nodes = leaves
        .into_iter()
        .filter(|leaf| seen.insert(leaf.name.clone()))
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
    let root = memo::balance(nodes, |children: Vec<Node<M>>| Node {
        name: memo::name(b"digest", children.iter().map(Node::name)),
        span: children[0].span.start..children[children.len() - 1].span.end,
        level: children[0].level + 1,
        bytes: children.iter().map(Node::bytes).sum(),
        measure: joined(children.iter().map(Node::measure)),
        children,
    })?;
    Some(Tree { root })
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
/// its own, and while the parts cost more than the budget, a RUN of sibling
/// parts is replaced by their parent. A run is a node all of whose children
/// are parts. It may close only once a line has arrived after it, so it is
/// settled ([`Tree::settled`]) and no line to come renames a closed part;
/// and only where the view stays DECAYING, its parts' levels never rising
/// toward the present: at the start of the log, or after a part at least
/// as high as itself. Of those, the most DUE closes, its due being its age
/// over its size (the lines from its first to the present, over the lines
/// it spans), so an old run closes before a young one and a small one
/// before a large; of two as due, the older. A run with no summary cannot
/// close: it is `waiting`, and the next one closes instead. The oldest
/// settled run may always close, so the view stops over its budget only
/// with a run waiting or with no settled run left: the view is then as
/// coarse as the log allows. A closed part is never renamed and never opens
/// as lines arrive, so what a reader was shown of the past stays as it was
/// shown, or coarser.
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
    // Per node, how many of its children are parts. A node whose count
    // reaches its arity is a run, and stays one until it closes, since
    // only it may close its children.
    let mut parted = vec![0; arena.nodes.len()];
    let mut runs: BTreeSet<usize> = BTreeSet::new();
    let mut waiting: BTreeSet<usize> = BTreeSet::new();
    let mut parts: Vec<usize> = Vec::new();
    let mut bytes = 0u64;
    let mut parted_one = |id: usize, runs: &mut BTreeSet<usize>| {
        if let Some(parent) = arena.parent[id] {
            parted[parent] += 1;
            if parted[parent] == node(parent).children.len() {
                runs.insert(parent);
            }
        }
    };
    for (now, &leaf) in arena.leaves.iter().enumerate() {
        let now = now + 1;
        parts.push(leaf);
        bytes += node(leaf).bytes;
        parted_one(leaf, &mut runs);
        while bytes > budget {
            // Where each run's children begin among the parts.
            let first = |run: usize| {
                let start = node(run).span.start;
                parts.partition_point(|&part| node(part).span.start < start)
            };
            let decaying = |run: usize| match first(run) {
                0 => true,
                at => node(parts[at - 1]).level >= node(run).level,
            };
            let mut order: Vec<usize> = runs
                .iter()
                .copied()
                .filter(|&run| node(run).span.end < now && decaying(run))
                .collect();
            order.sort_by(|&a, &b| arena.due(b, now).cmp(&arena.due(a, now)).then(a.cmp(&b)));
            let Some((at, size)) = order
                .iter()
                .enumerate()
                .find_map(|(at, &run)| cost(run).map(|size| (at, size)))
            else {
                waiting.extend(order);
                break;
            };
            let run = order[at];
            waiting.extend(&order[..at]);
            runs.remove(&run);
            let from = first(run);
            let freed: u64 = parts
                .splice(from..from + node(run).children.len(), [run])
                .map(|part| cost(part).unwrap_or_default())
                .sum();
            bytes = bytes - freed + size;
            parted_one(run, &mut runs);
        }
    }
    View {
        parts: parts.iter().map(|&id| node(id).part()).collect(),
        bytes,
        waiting: waiting.iter().map(|&id| node(id).name.clone()).collect(),
    }
}

/// The tree's nodes numbered in pre-order, which is the log's order by
/// first line and, among nodes starting at one line, outermost first.
struct Arena<'t, M> {
    nodes: Vec<&'t Node<M>>,
    parent: Vec<Option<usize>>,
    leaves: Vec<usize>,
}

impl<'t, M> Arena<'t, M> {
    fn of(tree: &'t Tree<M>) -> Self {
        let mut arena = Arena {
            nodes: Vec::new(),
            parent: Vec::new(),
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
        arena
    }

    /// How due a run is at `now` lines: its age over its size.
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
