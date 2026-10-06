//! SPEC law 46: a digest's view is a decaying cut of a tree that only grows.
//!
//! Over generated logs (distinct lines of any cost, each resting on up to
//! two before it, given in any order and any number of times) and
//! generated summaries (each inner node
//! summarised or not, of any cost, by a draw from its name): a view tiles
//! the log; it never costs more than its budget or its lines, and fits
//! its budget unless no closing it may make lowers its cost; what waits is
//! pending; its
//! parts' levels never rise toward the present; zooming every part down to
//! the lines reads the log back in causal order; the tree, `pending` and
//! the view are functions of the set of lines; an
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

/// A line of a generated log: its id, its cost, and the ids it rests on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    id: u16,
    bytes: u32,
    parents: Vec<u16>,
}

fn leaf(line: &Line) -> Leaf<Ends> {
    let name = line_name(line.id);
    Leaf {
        measure: Ends(Some((name.clone(), name.clone())), 1),
        parents: line.parents.iter().map(|&id| line_name(id)).collect(),
        name,
        bytes: line.bytes,
    }
}

fn leaves(log: &[Line]) -> Vec<Leaf<Ends>> {
    log.iter().map(leaf).collect()
}

/// A log: distinct lines, each resting on up to two lines before it, so
/// some are concurrent and the order is the tree's to choose.
fn a_log(most: usize) -> impl Strategy<Value = Vec<Line>> {
    prop::collection::vec(
        (
            any::<u16>(),
            1..=64u32,
            prop::collection::vec(any::<prop::sample::Index>(), 0..3),
        ),
        1..most,
    )
    .prop_map(|drawn| {
        let mut seen = BTreeSet::new();
        let mut log: Vec<Line> = Vec::new();
        for (id, bytes, parents) in drawn {
            if !seen.insert(id) {
                continue;
            }
            let parents = if log.is_empty() {
                Vec::new()
            } else {
                parents
                    .iter()
                    .map(|at| log[at.index(log.len())].id)
                    .collect()
            };
            log.push(Line { id, bytes, parents });
        }
        log
    })
}

/// The log's names in causal order, ties by name, worked out here apart
/// from the crate's walk: the least line whose parents are all placed,
/// again and again.
fn causal_order(log: &[Line]) -> Vec<Hash> {
    let mut placed: BTreeSet<u16> = BTreeSet::new();
    let mut order = Vec::new();
    while placed.len() < log.len() {
        let next = log
            .iter()
            .filter(|line| !placed.contains(&line.id))
            .filter(|line| line.parents.iter().all(|parent| placed.contains(parent)))
            .min_by_key(|line| line_name(line.id))
            .expect("a log with no cycle has a line to place");
        placed.insert(next.id);
        order.push(line_name(next.id));
    }
    order
}

/// The tree over a generated log.
fn tree_of(log: &[Line]) -> Tree<Ends> {
    tree(leaves(log)).expect("no cycle").expect("a line")
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

/// The nodes the view may still close and lower its cost by, worked out
/// from the view alone: each above some parts and below none, settled,
/// after a part at least as high as itself (or first), and summarised in
/// less than the parts it spans cost.
fn closings_that_lower<M>(tree: &Tree<M>, summaries: Summaries, view: &View) -> Vec<Hash> {
    nodes(tree)
        .into_iter()
        .filter(|node| {
            let span = node.span();
            let inside: Vec<_> = view
                .parts
                .iter()
                .filter(|part| span.start <= part.span.start && part.span.end <= span.end)
                .collect();
            let above_parts =
                inside.len() > 1 || inside.first().is_some_and(|part| part.span != span);
            let below_none = !view
                .parts
                .iter()
                .any(|part| part.span.start <= span.start && span.end <= part.span.end);
            let decaying = view
                .parts
                .iter()
                .rev()
                .find(|part| part.span.end <= span.start)
                .is_none_or(|before| {
                    tree.node(&before.node).expect("in the tree").level() >= node.level()
                });
            let replaced: u64 = inside
                .iter()
                .map(|part| cost(tree, summaries, &part.node))
                .sum();
            above_parts
                && below_none
                && tree.settled(node)
                && decaying
                && summaries
                    .of(node.name())
                    .is_some_and(|size| u64::from(size) < replaced)
        })
        .map(|node| node.name().clone())
        .collect()
}

/// What the log's lines cost, every one shown.
fn lines_cost<M>(tree: &Tree<M>) -> u64 {
    tree.root().bytes()
}

proptest! {
    #![proptest_config(crate::common::cases::cases(256))]

    /// Law 1, tiling, and the view's own bookkeeping: its parts partition
    /// the log in order, it costs what its parts cost, and each closed
    /// part has a summary.
    #[test]
    fn a_view_tiles_the_log(log in a_log(300), summaries in summaries(), budget in a_budget()) {
        let tree = tree_of(&log);
        let view = view(&tree, |name| summaries.of(name), budget);
        let mut at = 0;
        for part in &view.parts {
            prop_assert_eq!(part.span.start, at);
            prop_assert!(part.span.end > at);
            prop_assert_eq!(tree.node(&part.node).map(Node::span), Some(part.span.clone()));
            at = part.span.end;
        }
        prop_assert_eq!(at, tree.lines());
        let total: u64 = view.parts.iter().map(|part| cost(&tree, summaries, &part.node)).sum();
        prop_assert_eq!(view.bytes, total);
    }

    /// Law 2, budget: a view never costs more than its budget or its
    /// lines, whichever is more; it fits its budget unless no closing it
    /// may make would lower its cost; and whatever waits is a settled run
    /// with no summary that `pending` offers.
    #[test]
    fn a_view_fits_its_budget_unless_no_closing_lowers_it(
        log in a_log(300),
        summaries in summaries(),
        budget in a_budget(),
    ) {
        let tree = tree_of(&log);
        let of = |name: &Hash| summaries.of(name);
        let view = view(&tree, of, budget);
        prop_assert!(view.bytes <= budget.max(lines_cost(&tree)));
        prop_assert!(view.bytes <= lines_cost(&tree));
        if view.bytes > budget {
            let lowering = closings_that_lower(&tree, summaries, &view);
            prop_assert!(lowering.is_empty(), "{} could close and lower the cost", lowering.len());
        }
        let parts: BTreeSet<&Hash> = view.parts.iter().map(|part| &part.node).collect();
        let ready: BTreeSet<Hash> = pending(&tree, of).into_iter().collect();
        for name in &view.waiting {
            let node = tree.node(name).expect("a waiting node is in the tree");
            prop_assert!(tree.settled(node) && of(name).is_none());
            prop_assert!(node.children().iter().all(|child| parts.contains(child.name())));
            prop_assert!(ready.contains(name), "waiting, and not pending");
        }
        // An unbounded budget shows every line.
        let whole = view_of(&tree, summaries, u64::MAX);
        prop_assert_eq!(whole.parts.len(), tree.lines());
    }

    /// Law 3, decay: toward the present, the parts' levels never rise.
    #[test]
    fn a_view_decays(log in a_log(400), summaries in summaries(), budget in a_budget()) {
        let tree = tree_of(&log);
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
        let tree = tree_of(&log);
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
        let read: Vec<Hash> = lines.into_iter().map(|part| part.node).collect();
        prop_assert_eq!(read, causal_order(&log));
        prop_assert_eq!(zoom(&tree, &line_name(u16::MAX)), None);
    }

    /// Law 5, free: the tree, `pending` and the view are functions of the
    /// set of lines: the same however the lines are ordered and however
    /// many times each is given, so two replicas that hold one log build
    /// one tree.
    #[test]
    fn a_digest_is_a_function_of_the_set_of_lines(
        (log, shuffled) in a_log(300).prop_flat_map(|log| (Just(log.clone()), Just(log).prop_shuffle())),
        again in prop::collection::vec(any::<prop::sample::Index>(), 0..40),
        summaries in summaries(),
        budget in a_budget(),
    ) {
        let mut given = shuffled;
        for which in again {
            let copy = given[which.index(given.len())].clone();
            given.push(copy);
        }
        let (once, twice) = (tree_of(&log), tree_of(&given));
        prop_assert_eq!(&once, &twice);
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
        let tree = tree_of(&log);
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
        let mut log = log;
        let summary = 1 + u32::try_from(summaries.seed % u64::from(line)).expect("under a line");
        let summaries = Summaries { longest: if uniform { 1 } else { summaries.longest }, ..summaries };
        let of = |name: &Hash| summaries.of(name).map(|bytes| if uniform { summary } else { bytes });
        if uniform {
            for each in &mut log {
                each.bytes = line;
            }
        }
        // The new line has seen every line, so it comes last.
        let ids: BTreeSet<u16> = log.iter().map(|each| each.id).collect();
        let id = (0..=u16::MAX).find(|id| !ids.contains(id)).expect("a free id");
        let last = Line { id, bytes: line, parents: ids.into_iter().collect() };
        let (before, after) = (tree_of(&log), tree_of(&[log.clone(), vec![last]].concat()));
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

    /// Law 6, a line arriving late: a line concurrent with the rest lands
    /// where its name puts it, and renames at most a logarithmic number of
    /// the tree's nodes.
    #[test]
    fn a_late_line_renames_a_logarithmic_spine(
        ids in prop::collection::btree_set(any::<u16>(), 64..1200),
        at in any::<prop::sample::Index>(),
    ) {
        let log: Vec<Line> = ids.into_iter().map(|id| Line { id, bytes: 1, parents: Vec::new() }).collect();
        let late = log[at.index(log.len())].id;
        let without: Vec<Line> = log.iter().filter(|line| line.id != late).cloned().collect();
        let (before, after) = (tree_of(&without), tree_of(&log));
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
        let tree = tree_of(&log);
        let mut cache = Cache::default();
        let (measure, _) = fold(&Measured::default(), tree.root(), &mut cache);
        prop_assert_eq!(&measure, tree.root().measure());
        let measures: Vec<Ends> = causal_order(&log).iter().map(|name| Ends(Some((name.clone(), name.clone())), 1)).collect();
        prop_assert_eq!(&measure, &measures.iter().fold(Ends::empty(), |measure, line| measure.join(line)));
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
    let one = tree_of(&[Line {
        id: 0,
        bytes: 9,
        parents: Vec::new(),
    }]);
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

    let log: Vec<Line> = (0..2000)
        .map(|id| Line {
            id,
            bytes: 40,
            parents: Vec::new(),
        })
        .collect();
    let tree = tree_of(&log);
    let all = Summaries {
        seed: 1,
        percent: 100,
        longest: 8,
    };
    let view = view_of(&tree, all, 0);
    assert!(view.waiting.is_empty());
    assert!(closings_that_lower(&tree, all, &view).is_empty());
    // At most one level of the path to the last line is open per level,
    // each with fewer than `WIDEST` settled children shown.
    let depth = tree.root().level();
    assert!(
        view.parts.len() <= depth * memo::balance::WIDEST,
        "{} parts",
        view.parts.len()
    );
}

/// A closing never raises the cost: 17 lines of 10 bytes under a budget of
/// 169, with summaries of 5 or 500 bytes, never cost more than the lines.
#[test]
fn a_summary_longer_than_its_lines_never_closes() {
    let log: Vec<Line> = (0..17)
        .map(|id| Line {
            id,
            bytes: 10,
            parents: id.checked_sub(1).into_iter().collect(),
        })
        .collect();
    let tree = tree_of(&log);
    for seed in 0..64 {
        let long_or_short = |name: &Hash| {
            let hash = memo::name(&u64::to_be_bytes(seed), [name]);
            Some(
                if hash
                    .as_str()
                    .ends_with(['0', '1', '2', '3', '4', '5', '6', '7'])
                {
                    5
                } else {
                    500
                },
            )
        };
        let view = view(&tree, long_or_short, 169);
        assert!(
            view.bytes <= 170,
            "{} bytes in {} parts",
            view.bytes,
            view.parts.len()
        );
    }
}

/// Lines that rest on one another in a cycle are refused, naming the least
/// line the cycle leaves unplaced; a parent the log does not hold is
/// passed over.
#[test]
fn a_cycle_is_refused() {
    let line = |id, parents: &[u16]| Line {
        id,
        bytes: 1,
        parents: parents.to_vec(),
    };
    let cycle = [line(0, &[]), line(1, &[2]), line(2, &[1])];
    let least = line_name(1).min(line_name(2));
    assert_eq!(tree(leaves(&cycle)), Err(digest::Cycle(least)));
    let dangling = tree_of(&[line(0, &[9]), line(1, &[0])]);
    assert_eq!(dangling.lines(), 2);
}
