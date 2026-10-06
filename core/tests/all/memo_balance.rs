//! Design §6.2: a long sequence is a balanced, measured tree, so a change's
//! spine through it is logarithmic.
//!
//! A sequence built with `memo::balance` folds to the sequence itself, its
//! chunks' measures index it, every chunk holds two to `WIDEST` children
//! (the last of a level, one or more) and every element is at one depth, its tree is a function of its
//! elements alone, and after an element is replaced, inserted or removed, a fold through
//! the cache of the old tree computes a number of nodes logarithmic in the
//! sequence's length, where the same sequence as a list computes up to all
//! of them.

use prodrome::event::Hash;
use prodrome::memo::balance::WIDEST;
use prodrome::memo::{self, balance, fold, Algebra, Cache, Outcome, Tree};
use proptest::prelude::*;

/// A sequence's node: an element, or a chunk holding its children's count.
#[derive(Debug, Clone)]
enum Seq {
    Item {
        name: Hash,
        value: u32,
    },
    Chunk {
        name: Hash,
        len: usize,
        kids: Vec<Seq>,
    },
}

impl Seq {
    fn item(value: u32) -> Seq {
        Seq::Item {
            name: memo::name(&value.to_be_bytes(), []),
            value,
        }
    }

    /// The host's constructor for a run: named over its children, measured
    /// by their count.
    fn chunk(kids: Vec<Seq>) -> Seq {
        Seq::Chunk {
            name: memo::name(b"chunk", kids.iter().map(Tree::name)),
            len: kids.iter().map(Seq::len).sum(),
            kids,
        }
    }

    fn len(&self) -> usize {
        match self {
            Seq::Item { .. } => 1,
            Seq::Chunk { len, .. } => *len,
        }
    }

    /// The `index`th element, found by the measures: one chunk per level.
    fn get(&self, mut index: usize) -> Option<u32> {
        match self {
            Seq::Item { value, .. } => (index == 0).then_some(*value),
            Seq::Chunk { kids, .. } => {
                for kid in kids {
                    if index < kid.len() {
                        return kid.get(index);
                    }
                    index -= kid.len();
                }
                None
            }
        }
    }

    /// Whether every chunk below holds two to [`WIDEST`] children, but the
    /// last at each depth, which may hold one.
    fn fans_out(&self) -> bool {
        let mut level = vec![self];
        while !level.is_empty() {
            let mut below = Vec::new();
            for (at, node) in level.iter().enumerate() {
                if let Seq::Chunk { kids, .. } = node {
                    let least = if at + 1 == level.len() { 1 } else { 2 };
                    if !(least..=WIDEST).contains(&kids.len()) {
                        return false;
                    }
                    below.extend(kids);
                }
            }
            level = below;
        }
        true
    }

    /// The depth of its shallowest element: its [`Seq::depth`] when every
    /// element is at one depth.
    fn shallowest(&self) -> usize {
        match self {
            Seq::Item { .. } => 0,
            Seq::Chunk { kids, .. } => 1 + kids.iter().map(Seq::shallowest).min().unwrap_or(0),
        }
    }

    /// Every chunk's name in the tree.
    fn chunks(&self, into: &mut std::collections::BTreeSet<Hash>) {
        if let Seq::Chunk { name, kids, .. } = self {
            into.insert(name.clone());
            for kid in kids {
                kid.chunks(into);
            }
        }
    }

    /// The chunks that hold the last element, the root's first.
    fn spine(&self) -> Vec<Hash> {
        let mut spine = Vec::new();
        let mut node = self;
        while let Seq::Chunk { name, kids, .. } = node {
            spine.push(name.clone());
            node = kids.last().expect("a chunk holds two or more");
        }
        spine
    }

    fn depth(&self) -> usize {
        match self {
            Seq::Item { .. } => 0,
            Seq::Chunk { kids, .. } => 1 + kids.iter().map(Seq::depth).max().unwrap_or(0),
        }
    }
}

impl Tree for Seq {
    fn name(&self) -> &Hash {
        match self {
            Seq::Item { name, .. } | Seq::Chunk { name, .. } => name,
        }
    }

    fn children(&self) -> impl Iterator<Item = &Seq> {
        match self {
            Seq::Item { .. } => [].iter(),
            Seq::Chunk { kids, .. } => kids.iter(),
        }
    }
}

/// The sequence back: a chunk is the concatenation of its children.
struct Elements(Hash);

impl Algebra<Seq> for Elements {
    type Out = Vec<u32>;
    fn name(&self) -> &Hash {
        &self.0
    }
    fn apply(&self, node: &Seq, children: Vec<Vec<u32>>) -> Vec<u32> {
        match node {
            Seq::Item { value, .. } => vec![*value],
            Seq::Chunk { .. } => children.concat(),
        }
    }
}

fn elements() -> Elements {
    Elements(memo::name(b"elements", []))
}

fn balanced(values: &[u32]) -> Option<Seq> {
    balance(values.iter().copied().map(Seq::item).collect(), Seq::chunk)
}

/// The same values as a cons list, each node holding the rest.
fn list(values: &[u32]) -> Option<Seq> {
    values.iter().rev().fold(None, |rest, value| {
        let item = Seq::item(*value);
        Some(match rest {
            None => item,
            Some(rest) => Seq::chunk(vec![item, rest]),
        })
    })
}

/// An edit at a position, drawn as any number and [`resolve`]d against a
/// sequence's length.
#[derive(Debug, Clone)]
enum Edit {
    Replace(usize, u32),
    Insert(usize, u32),
    Remove(usize),
}

fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        (any::<usize>(), any::<u32>()).prop_map(|(at, v)| Edit::Replace(at, v)),
        (any::<usize>(), any::<u32>()).prop_map(|(at, v)| Edit::Insert(at, v)),
        any::<usize>().prop_map(Edit::Remove),
    ]
}

/// The edit at a position within a nonempty sequence of `len`.
fn resolve(edit: &Edit, len: usize) -> Edit {
    match *edit {
        Edit::Replace(at, value) => Edit::Replace(at % len, value),
        Edit::Insert(at, value) => Edit::Insert(at % (len + 1), value),
        Edit::Remove(at) => Edit::Remove(at % len),
    }
}

/// `values` with a resolved edit applied.
fn edited(values: &[u32], edit: &Edit) -> Vec<u32> {
    let mut values = values.to_vec();
    match *edit {
        Edit::Replace(at, value) => values[at] = value,
        Edit::Insert(at, value) => values.insert(at, value),
        Edit::Remove(at) => {
            values.remove(at);
        }
    }
    values
}

/// How many nodes a fold of `after` computes through the cache of `before`.
fn recomputed(before: &Seq, after: &Seq) -> usize {
    let mut cache = Cache::default();
    let _ = fold(&elements(), before, &mut cache);
    let (value, report) = fold(&elements(), after, &mut cache);
    let mut expected = Vec::new();
    for index in 0..after.len() {
        expected.extend(after.get(index));
    }
    assert_eq!(value, expected, "the memoised fold reads the sequence");
    report.names(Outcome::Computed).count()
}

proptest! {
    #![proptest_config(crate::common::cases::cases(64))]

    /// The tree holds the sequence, in order, and its measures index it.
    #[test]
    fn a_balanced_sequence_is_the_sequence(values in prop::collection::vec(0..64u32, 0..600)) {
        let Some(tree) = balanced(&values) else {
            prop_assert!(values.is_empty());
            return Ok(());
        };
        prop_assert_eq!(fold(&elements(), &tree, &mut Cache::default()).0, values.clone());
        prop_assert_eq!(tree.len(), values.len());
        prop_assert!(tree.fans_out(), "a chunk of one, or of more than {}", WIDEST);
        prop_assert_eq!(tree.shallowest(), tree.depth(), "every element at one depth");
        for (index, value) in values.iter().enumerate() {
            prop_assert_eq!(tree.get(index), Some(*value));
        }
    }

    /// The tree is a function of the elements: an edit and its undoing
    /// build the tree the sequence had, name for name.
    #[test]
    fn the_shape_is_the_elements(values in prop::collection::vec(any::<u32>(), 1..400), edit in edit()) {
        let edit = resolve(&edit, values.len());
        let changed = edited(&values, &edit);
        let undo = match edit {
            Edit::Replace(at, _) => Edit::Replace(at, values[at]),
            Edit::Insert(at, _) => Edit::Remove(at),
            Edit::Remove(at) => Edit::Insert(at, values[at]),
        };
        let undone = edited(&changed, &undo);
        prop_assert_eq!(&undone, &values);
        let (before, after) = (balanced(&values).expect("nonempty"), balanced(&undone).expect("nonempty"));
        prop_assert_eq!(before.name(), after.name());
    }

    /// An element appended renames only chunks that held the last element
    /// and names only chunks that hold it: every other chunk is the one it
    /// was, so a tree that only grows keeps every chunk off its last path.
    #[test]
    fn an_append_renames_only_the_spine(
        values in prop::collection::btree_set(any::<u32>(), 1..600),
        last in any::<u32>(),
    ) {
        prop_assume!(!values.contains(&last));
        let values: Vec<u32> = values.into_iter().collect();
        let mut longer = values.clone();
        longer.push(last);
        let (before, after) = (balanced(&values).expect("nonempty"), balanced(&longer).expect("nonempty"));
        let (mut old, mut new) = (std::collections::BTreeSet::new(), std::collections::BTreeSet::new());
        before.chunks(&mut old);
        after.chunks(&mut new);
        let renamed: std::collections::BTreeSet<Hash> = old.difference(&new).cloned().collect();
        prop_assert!(renamed.is_subset(&before.spine().into_iter().collect()));
        let named: std::collections::BTreeSet<Hash> = new.difference(&old).cloned().collect();
        prop_assert!(named.is_subset(&after.spine().into_iter().collect()));
    }

    /// After one edit, a fold of the balanced sequence computes a number of
    /// nodes logarithmic in its length; the list computes up to all of them.
    #[test]
    fn an_edit_recomputes_a_logarithmic_spine(
        values in prop::collection::vec(any::<u32>(), 256..2048),
        edit in edit(),
    ) {
        let edit = resolve(&edit, values.len());
        let after = edited(&values, &edit);
        let before = balanced(&values).expect("nonempty");
        let tree = balanced(&after).expect("nonempty");
        let log = usize::BITS - values.len().leading_zeros();
        let bound = 4 * log as usize;
        prop_assert!(tree.depth() <= bound, "depth {} over {}", tree.depth(), bound);
        let computed = recomputed(&before, &tree);
        prop_assert!(computed <= bound, "{} computed of {} elements", computed, values.len());
        if let Edit::Replace(at, _) = edit {
            // A list's spine to the `at`th element is the `at` nodes before
            // it and its own: the element and the node holding it.
            let (before, after) = (list(&values).expect("nonempty"), list(&after).expect("nonempty"));
            if before.name() != after.name() {
                let computed = recomputed(&before, &after);
                prop_assert!((at + 1..=at + 2).contains(&computed), "{} computed at {}", computed, at);
            }
        }
    }
}

#[test]
fn equal_elements_still_balance() {
    let values = vec![7; 5000];
    let tree = balanced(&values).expect("nonempty");
    assert!(tree.depth() <= 4, "depth {}", tree.depth());
    let mut longer = values.clone();
    longer.push(7);
    assert!(recomputed(&tree, &balanced(&longer).expect("nonempty")) <= 8);
}

/// Equal elements whose name ends a chunk close a run at every second
/// node, so they balance as a binary tree, every chunk but a level's last
/// holding two.
#[test]
fn equal_boundaries_balance_in_pairs() {
    // `memo::name` of 1 ends a chunk; of 7, above, does not.
    let values = vec![1; 5000];
    let tree = balanced(&values).expect("nonempty");
    assert!(tree.fans_out());
    assert!(tree.depth() <= 13, "depth {}", tree.depth());
    let mut longer = values.clone();
    longer.push(1);
    assert!(recomputed(&tree, &balanced(&longer).expect("nonempty")) <= 26);
}
