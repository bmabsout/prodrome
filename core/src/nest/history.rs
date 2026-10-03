//! The nesting monad (design §6.3): a history is a finite set of values,
//! each keyed by a path.

use std::collections::BTreeSet;

use super::path::Path;

/// A HISTORY KEYED BY PATH: a finite set of `(path, value)` pairs.
///
/// THE MONAD IS BUILT FROM TWO FREE STRUCTURES. Values form the finite
/// powerset, whose join is union (design §1); keys are paths, the free
/// monoid ([`Path`]). A history is the powerset of the monoid's writer,
/// `P(Path × T)`: Haskell's `WriterT Path Set`, were `Set` a `Monad`:
///
/// - [`History::unit`] is a value at the root, `{(ε, v)}`;
/// - [`History::join`] flattens a history of histories, putting each inner
///   value at its outer key then its inner one, `{(p·q, v)}`;
/// - [`History::bind`] is `join ∘ fmap f`: every value given its own
///   sub-history, flattened under its key.
///
/// Unit and associativity are those two structures' laws: `ε` is the unit
/// of `·` and `·` is associative, and a union of unions is one union
/// (`core/tests/all/nest.rs`). A value is never changed by flattening; only
/// its key gains a prefix. Which is why a content-addressed object, whose
/// name is its bytes, is never renamed by one and none of its deps is
/// rewritten.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct History<T>(BTreeSet<(Path, T)>);

impl<T: Ord> Default for History<T> {
    fn default() -> Self {
        History(BTreeSet::new())
    }
}

impl<T: Ord> FromIterator<(Path, T)> for History<T> {
    fn from_iter<I: IntoIterator<Item = (Path, T)>>(entries: I) -> Self {
        History(entries.into_iter().collect())
    }
}

impl<T: Ord> IntoIterator for History<T> {
    type Item = (Path, T);
    type IntoIter = std::collections::btree_set::IntoIter<(Path, T)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<T: Ord> History<T> {
    /// `{(ε, value)}`: an event is the history of one object.
    pub fn unit(value: T) -> History<T> {
        History([(Path::root(), value)].into())
    }

    /// Each value mapped by `f`, at its key.
    pub fn fmap<U: Ord>(self, mut f: impl FnMut(T) -> U) -> History<U> {
        self.0
            .into_iter()
            .map(|(path, value)| (path, f(value)))
            .collect()
    }

    /// `join ∘ fmap f`: each value given the sub-history `f` makes of it,
    /// flattened under that value's key. Local state per entity: each
    /// todo's draft, each conversation's lines, as one history.
    pub fn bind<U: Ord>(self, f: impl FnMut(T) -> History<U>) -> History<U> {
        self.fmap(f).join()
    }

    /// The union, the semilattice's join (design §1).
    #[must_use]
    pub fn union(mut self, other: History<T>) -> History<T> {
        self.0.extend(other.0);
        self
    }

    /// The values `chosen` keeps, each at its key.
    #[must_use]
    pub fn only(self, mut chosen: impl FnMut(&Path, &T) -> bool) -> History<T> {
        History(
            self.0
                .into_iter()
                .filter(|(path, value)| chosen(path, value))
                .collect(),
        )
    }

    /// THE RESTRICTED JOIN: this history, and the values `chosen` keeps of
    /// `inner` held at `at`, flattened. It reads as this history plus exactly
    /// the chosen values, each under `at`; at the root, `at` is no prefix and
    /// they read as this history's own. An agent's batch accepted object by
    /// object, an overlay's flush (everything chosen) and an admitted
    /// plugin's store are this shape, and [`crate::store::accept`] is it for
    /// replicas.
    #[must_use]
    pub fn accept(
        self,
        at: &Path,
        inner: History<T>,
        chosen: impl FnMut(&Path, &T) -> bool,
    ) -> History<T> {
        History::from_iter([(Path::root(), self), (at.clone(), inner.only(chosen))]).join()
    }

    pub fn iter(&self) -> impl Iterator<Item = &(Path, T)> {
        self.0.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The values, whatever their keys.
    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.0.iter().map(|(_, value)| value)
    }
}

impl<T: Ord + Clone> History<T> {
    /// READING AT A PATH: the values whose keys begin with `path`, each at
    /// what follows it. Reading commutes with join: where a nest holds its
    /// histories at keys none of which begins another, reading the joined
    /// history at one of them is reading the history held there.
    #[must_use]
    pub fn at(&self, path: &Path) -> History<T> {
        self.0
            .iter()
            .filter_map(|(key, value)| Some((key.strip(path)?, value.clone())))
            .collect()
    }
}

impl<T: Ord> History<History<T>> {
    /// `P (P e) → P e`: every inner value at its outer key then its inner
    /// one. Renames no value; only keys gain a prefix.
    #[must_use]
    pub fn join(self) -> History<T> {
        self.0
            .into_iter()
            .flat_map(|(outer, inner)| {
                inner
                    .0
                    .into_iter()
                    .map(move |(key, value)| (outer.then(&key), value))
            })
            .collect()
    }
}
