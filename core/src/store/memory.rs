//! What a store has read (design §6.1): its objects, each verified once, and
//! their fold, extended by the objects it has not read yet and never rebuilt
//! from the files.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::dag::{decode, Dag, Unread};
use crate::event::{canonical, canonical_envelope, parents_of, Envelope, Hash};
use crate::literal::{Datetime, ProdromeError};
use crate::registers::{fold, Folded, Genesis};
use crate::schema::Schema;

/// A store's objects as it last read them, and their fold.
///
/// VERIFIED, NOT TRUSTED FOR BEING HELD. An object enters only as
/// [`Verified`]: its bytes rehashed to its name, or sealed here. Its FILE is
/// only held: [`Seen`] is how it looked then, and a file that no longer looks
/// so is read and verified again before anything is read from memory.
///
/// The fold is the fold of the objects, always, taken the first time it is
/// asked for: an object arriving after that is [`Folded::insert`]ed where the
/// linearisation of the union puts it, and what insertion cannot do (an
/// object gone, one that something held names, one resting on another scope)
/// sets the fold aside, to be taken again from memory, never from the files.
///
/// INCREMENTAL EQUALS COLD: after any looks, admissions and forgettings, a
/// memory is what one fresh look at the same files makes, its fold
/// `fold(&dag.nodes_across_gaps()?)`, its tips `dag.tips()` and its geneses
/// `dag.geneses()` (`core/tests/all/memory.rs`).
pub struct Memory<E: Schema> {
    dag: Arc<Dag<E>>,
    folded: Option<Arc<Folded<E>>>,
    tips: BTreeSet<Hash>,
    /// What a held object names (a parent, a genesis) that is not held.
    wanted: BTreeSet<Hash>,
    /// Each held object's file as it was seen, `None` while a look could
    /// not tell a change from none.
    files: BTreeMap<Hash, Option<Seen>>,
    /// The objects whose events name each entity at each instant, which
    /// two events that print the same do: the twin check prints these and
    /// no other.
    twins: BTreeMap<(E::Key, Datetime), BTreeSet<Hash>>,
    /// Each held `Genesis`, and the least legacy root: [`Dag::geneses`].
    geneses: BTreeSet<Hash>,
}

/// An object whose name is the hash of its bytes, which is the only way into
/// a [`Memory`].
pub struct Verified<E> {
    name: Hash,
    object: Envelope<E>,
}

impl<E: Schema> Verified<E> {
    /// An object sealed here, and the print it is named by, which is what
    /// its file holds.
    pub fn sealed(object: Envelope<E>) -> (Verified<E>, String) {
        let print = canonical_envelope(&object);
        let name = Hash::of_bytes(print.as_bytes());
        (Verified { name, object }, print)
    }

    pub fn name(&self) -> &Hash {
        &self.name
    }

    pub fn object(&self) -> &Envelope<E> {
        &self.object
    }

    /// `bytes` under `name`, rehashed and parsed.
    pub fn read(name: &Hash, bytes: &[u8]) -> Result<Verified<E>, Unread> {
        decode(name, bytes).map(|object| Verified {
            name: name.clone(),
            object,
        })
    }
}

/// How long after a file changed at `modified` its `stat` may still not tell
/// a second change from none: a scheduler tick, where timestamps carry
/// fractions of a second, and FAT's two seconds where they do not (ext3 and
/// HFS+ keep one).
fn racy(modified: SystemTime) -> Duration {
    let fine = modified
        .duration_since(SystemTime::UNIX_EPOCH)
        .is_ok_and(|since| since.subsec_nanos() != 0);
    if fine {
        Duration::from_millis(50)
    } else {
        Duration::from_secs(2)
    }
}

/// A file as `stat` saw it: while it looks the same it holds the same bytes,
/// the test git's index makes of a worktree. Its size, its times and, on
/// unix, its device, inode and change time, which no writer sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

impl Seen {
    /// A file as `meta` says it looks at `now`, or `None` where that says
    /// nothing: it could not be read, or it changed within [`racy`] of
    /// `now`, so a change after this look could leave it looking the same
    /// (git's "racily clean"). A file seen as `None` is read again at the
    /// next look.
    pub fn of(meta: Option<fs::Metadata>, now: SystemTime) -> Option<Seen> {
        let meta = meta?;
        let modified = meta.modified().ok();
        let settled =
            modified.is_some_and(|at| now.duration_since(at).is_ok_and(|age| age >= racy(at)));
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            (meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec())
        };
        settled.then_some(Seen {
            len: meta.len(),
            modified,
            #[cfg(unix)]
            identity,
        })
    }
}

impl<E: Schema> Memory<E> {
    pub fn empty() -> Memory<E> {
        Memory {
            dag: Arc::new(Dag::from_iter([])),
            folded: None,
            tips: BTreeSet::new(),
            wanted: BTreeSet::new(),
            files: BTreeMap::new(),
            twins: BTreeMap::new(),
            geneses: BTreeSet::new(),
        }
    }

    pub fn dag(&self) -> &Arc<Dag<E>> {
        &self.dag
    }

    /// The fold of what is held, across a parent set aside: taken from
    /// memory the first time, and extended since.
    ///
    /// # Errors
    ///
    /// A cycle among what is held, which only a hash collision could make.
    pub fn folded(&mut self) -> Result<&Arc<Folded<E>>, ProdromeError> {
        if self.folded.is_none() {
            self.folded = Some(Arc::new(fold(&self.dag.nodes_across_gaps()?)));
        }
        Ok(self.folded.as_ref().expect("folded above"))
    }

    /// The first object, in the linearisation's order, of the prodrome
    /// `prodrome` whose event prints as `event` does: what an append of
    /// `event` answers instead of writing.
    ///
    /// # Errors
    ///
    /// [`Memory::folded`]'s.
    pub fn twin(&mut self, prodrome: &Genesis, event: &E) -> Result<Option<Hash>, ProdromeError> {
        let print = canonical(event);
        let twins: BTreeSet<Hash> = self
            .twins
            .get(&(event.key().clone(), event.at()))
            .into_iter()
            .flatten()
            .filter(|name| {
                self.dag
                    .get(name)
                    .and_then(Envelope::event)
                    .is_some_and(|held| canonical(held) == print)
            })
            .cloned()
            .collect();
        if twins.is_empty() {
            return Ok(None);
        }
        Ok(self
            .folded()?
            .prodromes()
            .get(prodrome)
            .and_then(|entities| entities.get(event.key()))
            .and_then(|stream| stream.iter().find(|stamp| twins.contains(&stamp.name)))
            .map(|stamp| stamp.name.clone()))
    }

    pub fn holds(&self, name: &Hash) -> bool {
        self.files.contains_key(name)
    }

    pub fn tips(&self) -> &BTreeSet<Hash> {
        &self.tips
    }

    /// Every prodrome's genesis, as [`Dag::geneses`] answers it.
    pub fn geneses(&self) -> &BTreeSet<Hash> {
        &self.geneses
    }

    /// The held names, each with its file as it was seen.
    pub fn files(&self) -> &BTreeMap<Hash, Option<Seen>> {
        &self.files
    }

    /// A held object's file, read again and found to hold its bytes, as it
    /// looks now.
    pub fn reseen(&mut self, name: &Hash, seen: Option<Seen>) {
        if let Some(held) = self.files.get_mut(name) {
            *held = seen;
        }
    }

    /// Forget `gone` and take in `fresh`, each with its file as it was seen
    /// before it was read. The tips, the geneses and the fold follow, each
    /// fresh object in turn, parents first: the fold by [`Folded::insert`].
    /// Where that cannot say (an object gone, one a held object names, one
    /// resting on another scope), the tips and the geneses are taken again
    /// from the held DAG and the fold is set aside until it is asked for.
    pub fn admit(&mut self, gone: &[Hash], fresh: Vec<(Verified<E>, Option<Seen>)>) {
        if gone.is_empty() && fresh.is_empty() {
            return;
        }
        let extends = gone.is_empty()
            && fresh
                .iter()
                .all(|(verified, _)| !self.wanted.contains(&verified.name));
        let dag = Arc::make_mut(&mut self.dag);
        for name in gone {
            if let Some(event) = dag.get(name).and_then(Envelope::event) {
                if let Some(twins) = self.twins.get_mut(&(event.key().clone(), event.at())) {
                    twins.remove(name);
                }
            }
            dag.remove(name);
            self.files.remove(name);
        }
        let mut names = BTreeSet::new();
        for (Verified { name, object }, seen) in fresh {
            if let Some(event) = object.event() {
                self.twins
                    .entry((event.key().clone(), event.at()))
                    .or_default()
                    .insert(name.clone());
            }
            self.files.insert(name.clone(), seen);
            dag.insert(name.clone(), object);
            names.insert(name);
        }
        let order = match dag.order_among(&names.iter().collect()) {
            Ok(order) if extends => order,
            _ => {
                self.folded = None;
                self.tips = dag.tips();
                self.geneses = dag.geneses();
                self.wanted = dag
                    .objects()
                    .values()
                    .flat_map(named)
                    .filter(|named| dag.get(named).is_none())
                    .collect();
                return;
            }
        };
        for name in &order {
            let object = dag.get(name).expect("admitted above");
            for parent in parents_of(object) {
                self.tips.remove(&parent);
            }
            self.tips.insert(name.clone());
            self.wanted.extend(
                named(object)
                    .into_iter()
                    .filter(|named| dag.get(named).is_none()),
            );
            match object {
                Envelope::Genesis(_) => {
                    self.geneses.insert(name.clone());
                }
                // A legacy root: `geneses` keeps the least.
                Envelope::Sealed { prev: None, .. } => {
                    let root = self.geneses.iter().find(|held| dag.is_root(held)).cloned();
                    if root.as_ref().is_none_or(|root| name < root) {
                        self.geneses.insert(name.clone());
                        if let Some(root) = root {
                            self.geneses.remove(&root);
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(folded) = &mut self.folded {
            let folded = Arc::make_mut(folded);
            let placed = order
                .iter()
                .filter_map(|name| dag.node(name))
                .all(|node| folded.insert(&node).is_ok());
            if !placed {
                self.folded = None;
            }
        }
    }
}

/// What an object names: its parents, and the genesis of its prodrome.
fn named<E>(object: &Envelope<E>) -> Vec<Hash> {
    let mut names = parents_of(object);
    names.extend(object.genesis().cloned());
    names
}

/// Counts, not contents: a memory is the whole store.
impl<E: Schema> fmt::Debug for Memory<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Memory")
            .field("objects", &self.files.len())
            .field("tips", &self.tips.len())
            .finish_non_exhaustive()
    }
}
