//! Law 10 (design §6.3, §7), the monad: histories keyed by path, over
//! generated nests of two and three levels whose keys share prefixes.
//! Unit and associativity, bind as `join ∘ fmap`, reading at a path
//! commuting with join, and the restricted join reading as the parent plus
//! exactly what was chosen.

use std::collections::BTreeMap;

use prodrome::nest::{History, Path, Segment};
use proptest::prelude::*;

/// Few segments, so that generated keys share prefixes.
fn a_segment() -> impl Strategy<Value = Segment> {
    prop::sample::select(vec!["a", "b", "c"])
        .prop_map(|text| Segment::new(text).expect("a segment"))
}

fn a_path() -> impl Strategy<Value = Path> {
    prop::collection::vec(a_segment(), 0..3).prop_map(Path::of)
}

fn a_history<T: Ord + std::fmt::Debug>(
    values: impl Strategy<Value = T>,
) -> impl Strategy<Value = History<T>> {
    prop::collection::vec((a_path(), values), 0..5)
        .prop_map(|entries| entries.into_iter().collect())
}

fn a_nest() -> impl Strategy<Value = History<History<u8>>> {
    a_history(a_history(any::<u8>()))
}

fn a_nest_of_three() -> impl Strategy<Value = History<History<History<u8>>>> {
    a_history(a_history(a_history(any::<u8>())))
}

/// A function into histories, tabulated: a value's sub-history.
fn a_kleisli() -> impl Strategy<Value = Vec<History<u8>>> {
    prop::collection::vec(a_history(any::<u8>()), 1..4)
}

fn apply(table: &[History<u8>], value: u8) -> History<u8> {
    table[usize::from(value) % table.len()].clone()
}

/// Paths of exactly two segments: none begins another.
fn a_prefix_free_nest() -> impl Strategy<Value = Vec<(Path, History<u8>)>> {
    prop::collection::vec(
        (
            (a_segment(), a_segment()).prop_map(|(a, b)| Path::of([a, b])),
            a_history(any::<u8>()),
        ),
        0..5,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// `join ∘ unit = id` and `join ∘ fmap unit = id`: a history held at
    /// the root, or each of its values as a history of one, flattens to
    /// itself.
    #[test]
    fn unit_is_a_unit_of_join(history in a_history(any::<u8>())) {
        prop_assert_eq!(History::unit(history.clone()).join(), history.clone());
        prop_assert_eq!(history.clone().fmap(History::unit).join(), history);
    }

    /// `join ∘ join = join ∘ fmap join`: three levels flatten to one
    /// history in either order, as `a/(b/c)` and `(a/b)/c` are one path.
    #[test]
    fn join_is_associative(nest in a_nest_of_three()) {
        prop_assert_eq!(nest.clone().join().join(), nest.fmap(History::join).join());
    }

    /// Bind is `join ∘ fmap`, and its laws are the monad's: a unit bound is
    /// its function's value, a history bound to unit is itself, and binding
    /// twice is binding once to the composite.
    #[test]
    fn bind_is_the_monads(
        history in a_history(any::<u8>()),
        value in any::<u8>(),
        f in a_kleisli(),
        g in a_kleisli(),
    ) {
        prop_assert_eq!(History::unit(value).bind(|x| apply(&f, x)), apply(&f, value));
        prop_assert_eq!(history.clone().bind(History::unit), history.clone());
        prop_assert_eq!(
            history.clone().bind(|x| apply(&f, x)).bind(|y| apply(&g, y)),
            history.clone().bind(|x| apply(&f, x).bind(|y| apply(&g, y)))
        );
        prop_assert_eq!(
            history.clone().bind(|x| apply(&f, x)),
            history.fmap(|x| apply(&f, x)).join()
        );
    }

    /// Join renames no value and only prefixes keys: each value of the
    /// flattened history is a value of a history held, at its outer key then
    /// its inner one, and every such pair is in it.
    #[test]
    fn join_only_prefixes_keys(nest in a_nest()) {
        let mut expected = Vec::new();
        for (outer, inner) in nest.iter() {
            for (key, value) in inner.iter() {
                expected.push((outer.then(key), *value));
            }
        }
        prop_assert_eq!(nest.join(), expected.into_iter().collect::<History<u8>>());
    }

    /// READING COMMUTES WITH JOIN. Where no key of the nest begins another,
    /// reading the joined history at a key is reading the union of the
    /// histories held there; and reading at a path then at another is
    /// reading at the two joined.
    #[test]
    fn reading_commutes_with_join(entries in a_prefix_free_nest(), p in a_path(), q in a_path()) {
        let nest: History<History<u8>> = entries.iter().cloned().collect();
        let mut held: BTreeMap<Path, History<u8>> = BTreeMap::new();
        for (path, history) in entries {
            let union = held.remove(&path).unwrap_or_default().union(history);
            held.insert(path, union);
        }
        let joined = nest.join();
        for (path, history) in &held {
            prop_assert_eq!(&joined.at(path), history);
        }
        prop_assert_eq!(joined.at(&p).at(&q), joined.at(&p.then(&q)));
        prop_assert_eq!(joined.at(&Path::root()), joined);
    }

    /// THE RESTRICTED JOIN reads as the parent plus exactly the chosen
    /// values, each under the path the inner history was held at; at the
    /// root they read as the parent's own; and with everything chosen it is
    /// the join of the two, a flush.
    #[test]
    fn a_restricted_join_reads_as_the_parent_plus_the_chosen(
        parent in a_history(any::<u8>()),
        inner in a_history(any::<u8>()),
        at in a_path(),
        keep in any::<u8>(),
    ) {
        let chosen = |_: &Path, value: &u8| value % 3 != keep % 3;
        let accepted = parent.clone().accept(&at, inner.clone(), chosen);
        let mut expected = parent.clone();
        for (key, value) in inner.iter().filter(|(key, value)| chosen(key, value)) {
            expected = expected.union(History::from_iter([(at.then(key), *value)]));
        }
        prop_assert_eq!(&accepted, &expected);
        prop_assert_eq!(
            parent.clone().accept(&Path::root(), inner.clone(), chosen),
            parent.clone().union(inner.clone().only(chosen))
        );
        prop_assert_eq!(
            parent.clone().accept(&at, inner.clone(), |_, _| true),
            History::from_iter([(Path::root(), parent), (at, inner)]).join()
        );
    }
}

#[test]
fn a_path_prints_as_its_segments() {
    let path = Path::of(["todo-1", "draft"].map(|text| Segment::new(text).expect("a segment")));
    assert_eq!(path.to_string(), "todo-1/draft");
    assert_eq!(Path::root().to_string(), "");
    assert!(Segment::new("").is_err());
    assert!(Segment::new("a/b").is_err());
}
