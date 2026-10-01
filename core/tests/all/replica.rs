//! Law 7 (design §6.1, §7): replicas, on disk and in memory, and sync, the
//! join of two.
//!
//! Two writers' histories over one genesis, each on its own replica, drawn
//! by the generator every law reads. Sync between any two of them, whatever
//! each is held in, is idempotent and order-free, completes when it was cut
//! short, and leaves the receiver reading exactly what a fresh store reads
//! of the union of both replicas' files. A store in memory, appended to as a
//! store on disk is and receiving what it receives, writes the same objects
//! byte for byte. An overlay over a store reads as the union of its own
//! objects and the store's, appends as a store holding that union appends,
//! writes nothing to the store until it is flushed, and its flush leaves the
//! store reading that union.

use crate::common;

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use prodrome::event::{Hash, TodoEvent};
use prodrome::literal::ProdromeError;
use prodrome::policy::Untrusted;
use prodrome::reference::Todo;
use prodrome::registers::Folded;
use prodrome::store::{sync, EventStore, Held, MemoryStore, Overlay, Replica};
use proptest::prelude::*;

use crate::memory::{copy_store, replicas, Scratch};
use common::two_writers;

type Event = TodoEvent<Todo>;
type Disk = EventStore<Event, Untrusted>;
type Memory = MemoryStore<Event>;

/// What a replica reads: its objects, their fold and its tips.
#[derive(Debug, PartialEq)]
struct Reading {
    objects: BTreeSet<Hash>,
    folded: Folded<Event>,
    tips: BTreeSet<Hash>,
}

fn reading(replica: &(impl Replica<Event> + ?Sized)) -> Reading {
    let Held { dag, folded, tips } = replica.held().expect("reads");
    Reading {
        objects: dag.objects().keys().cloned().collect(),
        folded: (*folded).clone(),
        tips,
    }
}

/// A store in memory holding what `from` holds.
fn in_memory(from: &impl Replica<Event>) -> Memory {
    let memory = Memory::default();
    sync(from, &memory).expect("syncs");
    memory
}

/// What a fresh store reads of a directory holding both replicas' files.
fn union(scratch: &Scratch, a: &Disk, b: &Disk) -> Reading {
    let union = scratch.store("union");
    copy_store(a.root(), union.root());
    copy_store(b.root(), union.root());
    reading(&union)
}

/// A replica whose wire is cut: after `left` prints, it has none to give.
struct Cut<'r, R: ?Sized> {
    replica: &'r R,
    left: AtomicUsize,
}

impl<R: Replica<Event> + ?Sized> Replica<Event> for Cut<'_, R> {
    fn held(&self) -> Result<Held<Event>, ProdromeError> {
        self.replica.held()
    }

    fn tips(&self) -> Result<BTreeSet<Hash>, ProdromeError> {
        self.replica.tips()
    }

    fn print(&self, name: &Hash) -> Option<Vec<u8>> {
        let left = self.left.load(Ordering::Relaxed);
        self.left.store(left.saturating_sub(1), Ordering::Relaxed);
        (left > 0).then(|| self.replica.print(name)).flatten()
    }

    fn append(&self, event: Event) -> Result<Hash, ProdromeError> {
        self.replica.append(event)
    }

    fn receive(
        &self,
        seeds: BTreeSet<Hash>,
        print_of: &dyn Fn(&Hash) -> Option<Vec<u8>>,
    ) -> Result<Vec<Hash>, ProdromeError> {
        self.replica.receive(seeds, print_of)
    }
}

/// Law 7 between `from` and `to`, which hold what `here` and `there` hold:
/// a cut sync takes nothing, a sync of a down-set and then a whole one
/// reads as the union, and a second sync takes nothing.
fn join(
    from: &(impl Replica<Event> + ?Sized),
    to: &impl Replica<Event>,
    union: &Reading,
    cut: usize,
    partial: prop::sample::Index,
) -> Result<(), TestCaseError> {
    let before = reading(to);
    let wire = Cut {
        replica: from,
        left: AtomicUsize::new(cut),
    };
    if sync(&wire, to).is_err() {
        prop_assert_eq!(&reading(to), &before, "a cut sync takes nothing");
    }
    let theirs: Vec<Hash> = reading(from).objects.into_iter().collect();
    to.receive([partial.get(&theirs).clone()].into(), &|name| {
        from.print(name)
    })
    .expect("a down-set");
    sync(from, to).expect("syncs");
    prop_assert_eq!(&reading(to), union);
    prop_assert_eq!(sync(from, to).expect("syncs"), Vec::<Hash>::new());
    prop_assert_eq!(&reading(to), union);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// SYNC IS THE JOIN, whatever each replica is held in. From disk to
    /// disk, disk to memory, memory to disk and memory to memory: an
    /// interrupted sync completes on the next, and the receiver reads as a
    /// fresh store reads the union of both directories. And in either
    /// order, two replicas synced into a third read the same.
    #[test]
    fn sync_is_the_join(
        logs in two_writers(),
        cut in 0usize..12,
        partial in any::<prop::sample::Index>(),
    ) {
        let scratch = Scratch::new("join");
        let (here, there) = replicas(&scratch, &logs);
        let union = union(&scratch, &here, &there);
        let (memory_here, memory_there) = (in_memory(&here), in_memory(&there));
        prop_assert_eq!(&reading(&memory_here), &reading(&here));
        for (n, from) in [&there as &dyn Replica<Event>, &memory_there].into_iter().enumerate() {
            let to = scratch.store(&format!("to-{n}"));
            copy_store(here.root(), to.root());
            join(from, &to, &union, cut, partial)?;
            join(from, &in_memory(&here), &union, cut, partial)?;
        }
        let (one, other) = (Memory::default(), Memory::default());
        for (a, b, into) in [(&here, &there, &one), (&there, &here, &other)] {
            sync(a, into).expect("syncs");
            sync(b, into).expect("syncs");
        }
        prop_assert_eq!(&reading(&one), &union);
        prop_assert_eq!(&reading(&other), &union);
    }
}

/// One thing that happens to both stores.
#[derive(Debug, Clone)]
enum Step {
    /// An event of the pool appended to each.
    Append(prop::sample::Index),
    /// What one of the other writer's objects rests on, received by each.
    Arrive(prop::sample::Index),
}

fn a_step() -> impl Strategy<Value = Step> {
    prop_oneof![
        3 => any::<prop::sample::Index>().prop_map(Step::Append),
        1 => any::<prop::sample::Index>().prop_map(Step::Arrive),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// A STORE IN MEMORY WRITES WHAT A STORE ON DISK WRITES. Over any
    /// history of appends (twins and conflicts among them) and of another
    /// writer's objects arriving, each append answers the same name or the
    /// same refusal in both, the object under that name is the same bytes,
    /// and both read the same.
    #[test]
    fn a_store_in_memory_appends_what_a_disk_store_appends(
        logs in two_writers(),
        steps in prop::collection::vec(a_step(), 1..40),
    ) {
        let (shared, mine, _) = &logs;
        let pool: Vec<Event> = shared.iter().chain(mine).cloned().collect();
        prop_assume!(!pool.is_empty());
        let scratch = Scratch::new("appends");
        let (disk, there) = replicas(&scratch, &logs);
        let memory = in_memory(&disk);
        let theirs: Vec<Hash> = reading(&there).objects.into_iter().collect();
        for step in &steps {
            match step {
                Step::Append(pick) => {
                    let event = pick.get(&pool).clone();
                    let on_disk = disk.append(event.clone()).map_err(|e| e.to_string());
                    let in_memory = memory.append(event).map_err(|e| e.to_string());
                    prop_assert_eq!(&in_memory, &on_disk);
                    if let Ok(name) = on_disk {
                        prop_assert_eq!(memory.print(&name), Replica::print(&disk, &name));
                    }
                }
                Step::Arrive(pick) => {
                    let seed: BTreeSet<Hash> = [pick.get(&theirs).clone()].into();
                    let print = |name: &Hash| there.print(name);
                    let to_disk = disk.receive(seed.clone(), &print).expect("receives");
                    let to_memory = memory.receive(seed, &print).expect("receives");
                    prop_assert_eq!(to_memory, to_disk);
                }
            }
            prop_assert_eq!(reading(&memory), reading(&disk), "after {:?}", step);
        }
    }
}

/// One thing that happens to an overlay, or under it.
#[derive(Debug, Clone)]
enum Layered {
    /// An event of the pool appended to the overlay.
    Append(prop::sample::Index),
    /// What one of the other writer's objects rests on, received by the
    /// overlay.
    Arrive(prop::sample::Index),
    /// The same, received by the store under it, as another writer would.
    Beneath(prop::sample::Index),
}

fn a_layered_step() -> impl Strategy<Value = Layered> {
    prop_oneof![
        3 => any::<prop::sample::Index>().prop_map(Layered::Append),
        1 => any::<prop::sample::Index>().prop_map(Layered::Arrive),
        1 => any::<prop::sample::Index>().prop_map(Layered::Beneath),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// AN OVERLAY READS AS ITS UNION, AND ITS FLUSH LEAVES THE STORE READING
    /// IT. Beside the overlay a store on disk is given everything the overlay
    /// and the store under it are given: after every step the overlay reads
    /// as it does and appends byte for byte as it does, while the store under
    /// the overlay reads as one given only what reached it beneath. The flush
    /// leaves that store reading the union, and a second flush takes nothing.
    #[test]
    fn an_overlay_reads_as_its_union_until_it_is_flushed(
        logs in two_writers(),
        steps in prop::collection::vec(a_layered_step(), 1..30),
    ) {
        let (shared, mine, _) = &logs;
        let pool: Vec<Event> = shared.iter().chain(mine).cloned().collect();
        prop_assume!(!pool.is_empty());
        let scratch = Scratch::new("overlay");
        let (disk, there) = replicas(&scratch, &logs);
        let union = scratch.store("union");
        let beneath = scratch.store("beneath");
        for copy in [&union, &beneath] {
            copy_store(disk.root(), copy.root());
        }
        let overlay = Overlay::new(&disk);
        prop_assert_eq!(reading(&overlay), reading(&disk));
        let theirs: Vec<Hash> = reading(&there).objects.into_iter().collect();
        let print = |name: &Hash| there.print(name);
        for step in &steps {
            match step {
                Layered::Append(pick) => {
                    let event = pick.get(&pool).clone();
                    let written = overlay.append(event.clone()).map_err(|e| e.to_string());
                    let expected = union.append(event).map_err(|e| e.to_string());
                    prop_assert_eq!(&written, &expected);
                    if let Ok(name) = written {
                        prop_assert_eq!(overlay.print(&name), Replica::print(&union, &name));
                    }
                }
                Layered::Arrive(pick) => {
                    let seed: BTreeSet<Hash> = [pick.get(&theirs).clone()].into();
                    overlay.receive(seed.clone(), &print).expect("receives");
                    union.receive(seed, &print).expect("receives");
                }
                Layered::Beneath(pick) => {
                    let seed: BTreeSet<Hash> = [pick.get(&theirs).clone()].into();
                    for store in [&disk, &union, &beneath] {
                        store.receive(seed.clone(), &print).expect("receives");
                    }
                }
            }
            prop_assert_eq!(reading(&overlay), reading(&union), "after {:?}", step);
            prop_assert_eq!(reading(&disk), reading(&beneath), "after {:?}", step);
        }
        overlay.flush().expect("flushes");
        prop_assert_eq!(reading(&disk), reading(&union));
        prop_assert_eq!(reading(&overlay), reading(&union));
        prop_assert_eq!(overlay.flush().expect("flushes"), Vec::<Hash>::new());
        prop_assert_eq!(reading(&scratch.store("here")), reading(&union), "and cold");
    }
}
