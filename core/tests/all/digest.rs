//! SPEC law 46: a digest's view is a decaying cut of a tree that only grows.
//!
//! Over generated logs (lines named by a small range of ids, so a line may
//! be given twice, of any cost) and generated summaries (each inner node
//! summarised or not, of any cost, by a draw from its name): a view tiles
//! the log; it fits its budget unless a run waits or none may close; its
//! parts' levels never rise toward the present; zooming every part down to
//! the lines reads the log back; the tree, `pending` and the view are
//! functions of the lines' names in order, whatever was given twice; an
//! appended line only coarsens the past and, with lines of one cost and
//! summaries of one no longer, changes at most `1 + ⌈line / summary⌉`
//! parts; a line arriving late renames a logarithmic number of nodes; and
//! the measure folded through `memo::fold`, and the view and `pending`
//! read through a cache keyed by `digest::read`, are the plain ones.

use std::collections::BTreeSet;

use prodrome::digest::{
    self, pending, tree, view, zoom, Leaf, Measure, Measured, Node, Tree, View,
};
use prodrome::event::Hash;
use prodrome::memo::{self, fold, Cache, Key, Lookup};
use proptest::prelude::*;

/// A measure whose join is not commutative: the first and last line's
/// names and how many lines, so a measure folded out of order is caught.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Ends(Option<(Hash, Hash)>, usize);

impl Measure for Ends {
    fn empty() -> Self {
        Ends(None, 0)
    }
    fn join(&self, other: &Self) -> Self {
        let ends = match (&self.0, &other.0) {
            (Some((first, _)), Some((_, last))) => Some((first.clone(), last.clone())),
            (ends, None) | (None, ends) => ends.clone(),
        };
        Ends(ends, self.1 + other.1)
    }
}

fn line_name(id: u16) -> Hash {
    memo::name(b"line", [&memo::name(&id.to_be_bytes(), [])])
}

fn leaf(id: u16, bytes: u32) -> Leaf<Ends> {
    let name = line_name(id);
    Leaf {
        measure: Ends(Some((name.clone(), name.clone())), 1),
        name,
        bytes,
    }
}

/// A log: lines by id and cost, ids drawn from a range small enough that
/// some come twice.
fn a_log(most: usize) -> impl Strategy<Value = Vec<(u16, u32)>> {
    prop::collection::vec((0..(most as u16 * 2), 1..=64u32), 1..most)
}

fn leaves(log: &[(u16, u32)]) -> Vec<Leaf<Ends>> {
    log.iter().map(|&(id, bytes)| leaf(id, bytes)).collect()
}

/// The log as its first copy of each line.
fn first_copies(log: &[(u16, u32)]) -> Vec<(u16, u32)> {
    let mut seen = BTreeSet::new();
    log.iter()
        .copied()
        .filter(|(id, _)| seen.insert(*id))
        .collect()
}

/// Which inner nodes have a summary, and how long: a draw from the name,
/// so a function of it, summarising `percent` of nodes.
#[derive(Debug, Clone, Copy)]
struct Summaries {
    seed: u64,
    percent: u64,
    longest: u64,
}

impl Summaries {
    fn of(&self, name: &Hash) -> Option<u32> {
        let draw = |salt: u64, modulo: u64| {
            let hash = memo::name(&(self.seed ^ salt).to_be_bytes(), [name]);
            u64::from_str_radix(&hash.as_str()[..12], 16).expect("hex") % modulo
        };
        (draw(0, 100) < self.percent)
            .then(|| u32::try_from(1 + draw(1, self.longest)).expect("small"))
    }
}

fn summaries() -> impl Strategy<Value = Summaries> {
    (
        any::<u64>(),
        prop_oneof![Just(100u64), 0..=100u64],
        1..=96u64,
    )
        .prop_map(|(seed, percent, longest)| Summaries {
            seed,
            percent,
            longest,
        })
}

fn a_budget() -> impl Strategy<Value = u64> {
    prop_oneof![0..200u64, 0..4000u64, Just(u64::MAX)]
}

/// Every node of the tree, in pre-order.
fn nodes<M>(tree: &Tree<M>) -> Vec<&Node<M>> {
    let mut all = Vec::new();
    let mut stack = vec![tree.root()];
    while let Some(node) = stack.pop() {
        all.push(node);
        stack.extend(node.children().iter().rev());
    }
    all
}

fn names<M>(tree: &Tree<M>) -> BTreeSet<Hash> {
    nodes(tree)
        .into_iter()
        .map(|node| node.name().clone())
        .collect()
}

/// A part's cost in the view.
fn cost<M>(tree: &Tree<M>, summaries: Summaries, name: &Hash) -> u64 {
    let node = tree.node(name).expect("a part is a node of the tree");
    if node.is_leaf() {
        node.bytes()
    } else {
        u64::from(summaries.of(name).expect("a closed part has a summary"))
    }
}

/// Whether some node all of whose children are parts may still close: it
/// is settled, it has no part above it, and it is not one of the parts.
fn a_settled_run<M>(tree: &Tree<M>, view: &View) -> bool {
    let parts: BTreeSet<&Hash> = view.parts.iter().map(|part| &part.node).collect();
    nodes(tree).into_iter().any(|node| {
        !node.is_leaf()
            && tree.settled(node)
            && !parts.contains(node.name())
            && node
                .children()
                .iter()
                .all(|child| parts.contains(child.name()))
    })
}

proptest! {
    #![proptest_config(crate::common::cases::cases(256))]

    /// Law 1, tiling, and the view's own bookkeeping: its parts partition
    /// the log in order, it costs what its parts cost, and each closed
    /// part has a summary.
    #[test]
    fn a_view_tiles_the_log(log in a_log(300), summaries in summaries(), budget in a_budget()) {
        let tree = tree(leaves(&log)).expect("a line");
        let view = view(&tree, |name| summaries.of(name), budget);
        let mut at = 0;
        for part in &view.parts {
            prop_assert_eq!(part.span.start, at);
            prop_assert!(part.span.end > at);
            prop_assert_eq!(tree.node(&part.node).map(Node::span), Some(part.span.clone()));
            at = part.span.end;
        }
        prop_assert_eq!(at, tree.len());
        let total: u64 = view.parts.iter().map(|part| cost(&tree, summaries, &part.node)).sum();
        prop_assert_eq!(view.bytes, total);
    }

    /// Law 2, budget: with nothing waiting, a view fits its budget or no
    /// run of it may close; whatever waits is a settled run with no
    /// summary.
    #[test]
    fn a_view_fits_its_budget_unless_a_run_waits(
        log in a_log(300),
        summaries in summaries(),
        budget in a_budget(),
    ) {
        let tree = tree(leaves(&log)).expect("a line");
        let view = view(&tree, |name| summaries.of(name), budget);
        if view.waiting.is_empty() {
            prop_assert!(view.bytes <= budget || !a_settled_run(&tree, &view));
        }
        let parts: BTreeSet<&Hash> = view.parts.iter().map(|part| &part.node).collect();
        for name in &view.waiting {
            let node = tree.node(name).expect("a waiting node is in the tree");
            prop_assert!(tree.settled(node) && summaries.of(name).is_none());
            prop_assert!(node.children().iter().all(|child| parts.contains(child.name())));
        }
        // With every node summarised, nothing waits, and an unbounded budget
        // shows every line.
        let all = Summaries { percent: 100, ..summaries };
        prop_assert!(view_of(&tree, all, budget).waiting.is_empty());
        let whole = view_of(&tree, summaries, u64::MAX);
        prop_assert_eq!(whole.parts.len(), tree.len());
    }

    /// Law 3, decay: toward the present, the parts' levels never rise.
    #[test]
    fn a_view_decays(log in a_log(400), summaries in summaries(), budget in a_budget()) {
        let tree = tree(leaves(&log)).expect("a line");
        let view = view(&tree, |name| summaries.of(name), budget);
        let levels: Vec<usize> = view
            .parts
            .iter()
            .map(|part| tree.node(&part.node).expect("in the tree").level())
            .collect();
        prop_assert!(levels.windows(2).all(|pair| pair[0] >= pair[1]), "levels {:?}", levels);
    }

    /// Law 4, refinement: zooming every part down to the lines reads the
    /// log back, line for line; a line does not open.
    #[test]
    fn zooming_every_part_reads_the_log(log in a_log(300), summaries in summaries(), budget in a_budget()) {
        let tree = tree(leaves(&log)).expect("a line");
        let view = view(&tree, |name| summaries.of(name), budget);
        let mut open = view.parts;
        let mut lines = Vec::new();
        while !open.is_empty() {
            let mut next = Vec::new();
            for part in open {
                match zoom(&tree, &part.node) {
                    Some(children) => {
                        prop_assert_eq!(children.first().map(|child| child.span.start), Some(part.span.start));
                        prop_assert_eq!(children.last().map(|child| child.span.end), Some(part.span.end));
                        next.extend(children);
                    }
                    None => {
                        prop_assert!(tree.node(&part.node).is_some_and(Node::is_leaf));
                        next.push(part);
                    }
                }
            }
            if next.iter().all(|part| part.span.len() == 1) {
                lines = next;
                break;
            }
            open = next;
        }
        let expected: Vec<Hash> = first_copies(&log).iter().map(|&(id, _)| line_name(id)).collect();
        let read: Vec<Hash> = lines.into_iter().map(|part| part.node).collect();
        prop_assert_eq!(read, expected);
        prop_assert_eq!(zoom(&tree, &line_name(u16::MAX)), None);
    }

    /// Law 5, free: the tree, `pending` and the view are the same for a log
    /// and for its first copies, whatever cost or measure a second copy
    /// carries; and two replicas that hold one log build one tree.
    #[test]
    fn a_digest_is_a_function_of_its_lines(
        log in a_log(300),
        again in prop::collection::vec((any::<prop::sample::Index>(), any::<prop::sample::Index>(), 1..=64u32), 0..40),
        summaries in summaries(),
        budget in a_budget(),
    ) {
        // Each copy of an earlier line, put anywhere after it.
        let mut doubled = log.clone();
        for (which, after, bytes) in again {
            let at = which.index(doubled.len());
            let (id, _) = doubled[at];
            let to = at + 1 + after.index(doubled.len() - at);
            doubled.insert(to, (id, bytes));
        }
        let (once, twice) = (tree(leaves(&first_copies(&log))).expect("a line"), tree(leaves(&doubled)).expect("a line"));
        prop_assert_eq!(&once, &twice);
        prop_assert_eq!(&once, &tree(leaves(&log)).expect("a line"));
        let of = |name: &Hash| summaries.of(name);
        prop_assert_eq!(pending(&once, of), pending(&twice, of));
        prop_assert_eq!(view(&once, of, budget), view(&twice, of, budget));
        prop_assert_eq!(digest::read(&once, of), digest::read(&twice, of));
    }

    /// `pending` is exactly the settled inner nodes with no summary whose
    /// children are lines or summarised, smallest first then leftmost; and
    /// recording every summary it names, round after round, summarises
    /// every settled node.
    #[test]
    fn pending_is_what_can_be_summarised(log in a_log(300), summaries in summaries()) {
        let tree = tree(leaves(&log)).expect("a line");
        let of = |name: &Hash| summaries.of(name);
        let ready = pending(&tree, of);
        let expected: BTreeSet<Hash> = nodes(&tree)
            .into_iter()
            .filter(|node| {
                !node.is_leaf()
                    && tree.settled(node)
                    && of(node.name()).is_none()
                    && node.children().iter().all(|child| child.is_leaf() || of(child.name()).is_some())
            })
            .map(|node| node.name().clone())
            .collect();
        prop_assert_eq!(ready.iter().cloned().collect::<BTreeSet<_>>(), expected);
        let order: Vec<(usize, usize)> = ready
            .iter()
            .map(|name| tree.node(name).map(|node| (node.span().len(), node.span().start)).expect("in the tree"))
            .collect();
        prop_assert!(order.windows(2).all(|pair| pair[0] < pair[1]));
        let mut written: BTreeSet<Hash> = BTreeSet::new();
        loop {
            let has = |name: &Hash| of(name).or_else(|| written.contains(name).then_some(1));
            let next = pending(&tree, has);
            if next.is_empty() {
                break;
            }
            written.extend(next);
        }
        let has = |name: &Hash| of(name).is_some() || written.contains(name);
        prop_assert!(nodes(&tree).into_iter().all(|node| node.is_leaf() || !tree.settled(node) || has(node.name())));
    }

    /// Law 6, stability: appending a line only coarsens the past, every
    /// part of the view before it lying within a part of the view after;
    /// and with lines of one cost and summaries of one no longer, from a
    /// view that fitted with nothing waiting, at most `1 + ⌈line /
    /// summary⌉` of the parts after are new.
    #[test]
    fn an_appended_line_only_coarsens_the_past(
        log in a_log(400),
        summaries in summaries(),
        budget in a_budget(),
        line in 1..=64u32,
        uniform in any::<bool>(),
    ) {
        let mut log = first_copies(&log);
        let summary = 1 + u32::try_from(summaries.seed % u64::from(line)).expect("under a line");
        let summaries = Summaries { longest: if uniform { 1 } else { summaries.longest }, ..summaries };
        let of = |name: &Hash| summaries.of(name).map(|bytes| if uniform { summary } else { bytes });
        if uniform {
            for (_, bytes) in &mut log {
                *bytes = line;
            }
        }
        let last = log.pop().expect("a line");
        prop_assume!(!log.is_empty());
        let (before, after) = (tree(leaves(&log)).expect("a line"), tree(leaves(&[log.clone(), vec![last]].concat())).expect("a line"));
        let (old, new) = (view(&before, of, budget), view(&after, of, budget));
        for part in &old.parts {
            prop_assert!(
                new.parts.iter().any(|wider| wider.span.start <= part.span.start && part.span.end <= wider.span.end),
                "{:?} opened", part.span
            );
            prop_assert!(after.node(&part.node).is_some(), "{:?} renamed", part.span);
        }
        if uniform && old.waiting.is_empty() && old.bytes <= budget {
            let c = 1 + line.div_ceil(summary) as usize;
            let changed = new.parts.iter().filter(|part| !old.parts.contains(part)).count();
            prop_assert!(changed <= c, "{} new parts, c = {}", changed, c);
        }
    }

    /// Law 6, a line arriving late: inserting a line anywhere renames at
    /// most a logarithmic number of the tree's nodes.
    #[test]
    fn a_late_line_renames_a_logarithmic_spine(
        log in prop::collection::btree_set(0..u16::MAX, 64..1200),
        at in any::<prop::sample::Index>(),
    ) {
        let log: Vec<(u16, u32)> = log.into_iter().map(|id| (id, 1)).collect();
        let late = log[at.index(log.len())];
        let without: Vec<(u16, u32)> = log.iter().copied().filter(|line| *line != late).collect();
        let (before, after) = (tree(leaves(&without)).expect("a line"), tree(leaves(&log)).expect("a line"));
        let renamed = names(&before).difference(&names(&after)).count();
        let bound = 4 * (usize::BITS - log.len().leading_zeros()) as usize;
        prop_assert!(renamed <= bound, "{} renamed of {}", renamed, log.len());
    }

    /// Law 7, memo: the measure folded through `memo::fold` is every node's
    /// measure, and the fold of the measures of the first copies; the view
    /// and `pending` read through a cache keyed by `digest::read` are the
    /// plain ones, and a summary recorded moves the key.
    #[test]
    fn a_digest_read_through_memo_is_the_plain_one(
        log in a_log(300),
        summaries in summaries(),
        budget in a_budget(),
    ) {
        let tree = tree(leaves(&log)).expect("a line");
        let mut cache = Cache::default();
        let (measure, _) = fold(&Measured::default(), tree.root(), &mut cache);
        prop_assert_eq!(&measure, tree.root().measure());
        let lines = leaves(&first_copies(&log));
        prop_assert_eq!(&measure, &lines.iter().fold(Ends::empty(), |measure, line| measure.join(&line.measure)));
        for node in nodes(&tree) {
            prop_assert_eq!(&fold(&Measured::default(), node, &mut cache).0, node.measure());
        }

        let of = |name: &Hash| summaries.of(name);
        let key = |function: &[u8], read: &Hash| Key {
            function: memo::name(function, []),
            argument: memo::name(&budget.to_be_bytes(), [read]),
        };
        let read = digest::read(&tree, of);
        let mut views: Cache<View> = Cache::default();
        let mut pendings: Cache<Vec<Hash>> = Cache::default();
        let view_key = key(b"digest view", &read);
        let pending_key = key(b"digest pending", &read);
        views.insert(view_key.clone(), view(&tree, of, budget)).expect("one value");
        pendings.insert(pending_key.clone(), pending(&tree, of)).expect("one value");
        prop_assert_eq!(views.get(&view_key), Lookup::Hit(&view(&tree, of, budget)));
        prop_assert_eq!(pendings.get(&pending_key), Lookup::Hit(&pending(&tree, of)));
        if let Some(next) = pending(&tree, of).first().cloned() {
            let recorded = |name: &Hash| if *name == next { Some(1) } else { of(name) };
            prop_assert_ne!(digest::read(&tree, recorded), read);
        }
    }
}

fn view_of<M>(tree: &Tree<M>, summaries: Summaries, budget: u64) -> View {
    view(tree, |name| summaries.of(name), budget)
}

/// A view of one line is that line, and a view with no budget and every
/// node summarised is as coarse as the log allows: the root's settled
/// children, closed, beside the path to the last line.
#[test]
fn a_view_with_no_budget_is_as_coarse_as_the_log_allows() {
    let one = tree(vec![leaf(0, 9)]).expect("a line");
    let view = view_of(
        &one,
        Summaries {
            seed: 0,
            percent: 100,
            longest: 4,
        },
        0,
    );
    assert_eq!(view.parts.len(), 1);
    assert_eq!((view.bytes, view.waiting.len()), (9, 0));

    let log: Vec<(u16, u32)> = (0..2000).map(|id| (id, 40)).collect();
    let tree = tree(leaves(&log)).expect("a line");
    let all = Summaries {
        seed: 1,
        percent: 100,
        longest: 8,
    };
    let view = view_of(&tree, all, 0);
    assert!(view.waiting.is_empty());
    assert!(!a_settled_run(&tree, &view));
    // At most one level of the path to the last line is open per level,
    // each with fewer than `WIDEST` settled children shown.
    let depth = tree.root().level();
    assert!(
        view.parts.len() <= depth * memo::balance::WIDEST,
        "{} parts",
        view.parts.len()
    );
}
