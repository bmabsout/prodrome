//! SPEC law 40: a store reads the largest down-set it holds.
//!
//! A history is a down-set of the causal order (design §1), and a set of
//! files need not be one: an object set aside, or one that has not arrived,
//! leaves what rests on it naming a parent nobody holds. [`Dag::interior`]
//! is the history such a set holds, every object whose ancestors are all
//! there, and it is an interior operator: never more than the set,
//! idempotent and monotone, the whole of a set closed under parents, and so
//! a function of the objects alone. It is the right adjoint of including
//! histories among sets of objects, so it preserves meets. A store whose files are any subset of a
//! history reads (its objects, its fold, its tips) exactly as a store holding
//! only that subset's interior, whichever handle reads it and whatever it
//! read before.
//!
//! And quarantine is that reading, said explicitly: `fsck` sets aside an
//! object whose bytes do not hash to its name, names it in its report, keeps
//! its bytes, and the store reads the down-set without it AND WITHOUT
//! EVERYTHING RESTING ON IT, the history of a replica that never received it.

use crate::common;
use crate::memory::{copy_store, replicas, Scratch};

use std::collections::BTreeSet;
use std::fs;

use prodrome::dag::{Dag, Finding};
use prodrome::event::{Hash, TodoEvent};
use prodrome::reference::Todo;
use prodrome::registers::fold;
use prodrome::store::EventStore;
use proptest::prelude::*;
use sha2::{Digest, Sha256};

use common::{a_log, seal, two_writers};

type Event = TodoEvent<Todo>;
type Store = EventStore<Event, prodrome::policy::Untrusted>;

/// Two writers' changes over one genesis, unioned into the first replica.
fn changes(scratch: &Scratch, logs: &(Vec<Event>, Vec<Event>, Vec<Event>)) -> Store {
    let (here, there) = replicas(scratch, logs);
    for tip in there.tips().expect("the tips derive") {
        here.adopt(&there, &tip).expect("adopts");
    }
    here
}

/// A log as legacy objects, each sealed on every tip.
fn legacy(scratch: &Scratch, log: &[Event]) -> Store {
    let store = scratch.store("legacy");
    for event in log {
        seal(&store, event.clone());
    }
    store
}

/// The objects of `dag` whose bit in `mask` is set, by their place in name
/// order (modulo 64).
fn chosen(dag: &Dag<Event>, mask: u64) -> BTreeSet<Hash> {
    dag.objects()
        .keys()
        .enumerate()
        .filter(|(i, _)| mask >> (i % 64) & 1 == 1)
        .map(|(_, name)| name.clone())
        .collect()
}

/// The name `bytes` would have, computed apart from the crate.
fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// `dag` without `removed`.
fn without(dag: &Dag<Event>, removed: &BTreeSet<Hash>) -> Dag<Event> {
    dag.objects()
        .iter()
        .filter(|(name, _)| !removed.contains(*name))
        .map(|(name, object)| (name.clone(), object.clone()))
        .collect()
}

fn names(dag: &Dag<Event>) -> BTreeSet<Hash> {
    dag.objects().keys().cloned().collect()
}

/// The interior laws at one history and one subset of it.
fn is_interior(whole: &Dag<Event>, mask: u64, more: u64) -> Result<(), TestCaseError> {
    prop_assert_eq!(&whole.interior(), whole, "a history is its own interior");
    let removed = chosen(whole, mask);
    let set = without(whole, &removed);
    let interior = set.interior();
    // Exactly the objects nothing removed is beneath: each object with its
    // ancestors (`closure` holds its seed) misses every removed one.
    let expected: BTreeSet<Hash> = names(&set)
        .into_iter()
        .filter(|name| whole.closure([name.clone()]).is_disjoint(&removed))
        .collect();
    prop_assert_eq!(names(&interior), expected);
    prop_assert!(names(&interior).is_subset(&names(&set)), "deflationary");
    prop_assert_eq!(&interior.interior(), &interior, "idempotent");
    prop_assert!(
        interior.nodes().is_ok(),
        "closed under parents, so it reads"
    );
    let smaller = without(&set, &chosen(&set, more));
    prop_assert!(
        names(&smaller.interior()).is_subset(&names(&interior)),
        "monotone"
    );
    // The right adjoint of including down-sets among sets, so it preserves
    // meets: the history two sets share is the meet of their histories.
    let other = without(whole, &chosen(whole, more));
    let apart: BTreeSet<Hash> = names(whole)
        .into_iter()
        .filter(|name| !(names(&set).contains(name) && names(&other).contains(name)))
        .collect();
    let both = without(whole, &apart);
    prop_assert_eq!(
        names(&both.interior()),
        names(&interior)
            .intersection(&names(&other.interior()))
            .cloned()
            .collect::<BTreeSet<_>>(),
        "meets"
    );
    Ok(())
}

/// The store law at one history: its files less `removed`, read by a fresh
/// handle and by one that read the whole store first, read as a store given
/// only the interior.
fn reads_its_interior(
    scratch: &Scratch,
    store: &Store,
    removed: &BTreeSet<Hash>,
) -> Result<(), TestCaseError> {
    let whole = store.dag().expect("a history reads");
    let interior = without(&whole, removed).interior();
    let warm = scratch.store("warm");
    copy_store(store.root(), warm.root());
    let _ = warm.dag().expect("the whole store reads");
    let gappy = scratch.store("gappy");
    copy_store(store.root(), gappy.root());
    for name in removed {
        for at in [&warm, &gappy] {
            fs::remove_file(
                at.root()
                    .join("objects")
                    .join(format!("{}.py", name.as_str())),
            )
            .expect("removes");
        }
    }
    let only = scratch.store("only");
    fs::create_dir_all(only.root().join("objects")).expect("creates objects/");
    for name in interior.objects().keys() {
        let file = format!("{}.py", name.as_str());
        fs::copy(
            store.root().join("objects").join(&file),
            only.root().join("objects").join(&file),
        )
        .expect("copies");
    }
    let folded = fold(&interior.nodes().expect("an interior reads"));
    for at in [&warm, &gappy, &only] {
        prop_assert_eq!(&*at.dag().expect("reads"), &interior);
        prop_assert_eq!(&*at.folded().expect("folds"), &folded);
        prop_assert_eq!(at.tips().expect("derives"), interior.tips());
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Law 40, the operator: over two writers' changes and over a legacy
    /// chain, the interior of any subset is exactly the objects nothing
    /// removed is beneath, and it is deflationary, idempotent, monotone and
    /// the whole of a history.
    #[test]
    fn the_interior_is_the_largest_down_set(
        logs in two_writers(),
        log in a_log(),
        mask in any::<u64>(),
        more in any::<u64>(),
    ) {
        let scratch = Scratch::new("interior");
        is_interior(&changes(&scratch, &logs).dag().expect("reads"), mask, more)?;
        is_interior(&legacy(&scratch, &log).dag().expect("reads"), mask, more)?;
    }

    /// Law 40, the store: a store missing any objects of a history, read cold
    /// or by a handle that held them all, reads its objects, its fold and its
    /// tips as a store given only the interior.
    #[test]
    fn a_store_reads_its_largest_down_set(
        logs in two_writers(),
        log in a_log(),
        mask in any::<u64>(),
    ) {
        let scratch = Scratch::new("down-set");
        let store = changes(&scratch, &logs);
        let removed = chosen(&store.dag().expect("reads"), mask);
        reads_its_interior(&Scratch::new("down-set-changes"), &store, &removed)?;
        let store = legacy(&scratch, &log);
        let removed = chosen(&store.dag().expect("reads"), mask);
        reads_its_interior(&Scratch::new("down-set-legacy"), &store, &removed)?;
    }
}

/// The quarantine law at one history and one object of it.
fn quarantine_reads_without_it(
    store: &Store,
    pick: prop::sample::Index,
) -> Result<(), TestCaseError> {
    let whole = store.dag().expect("a history reads");
    let names: Vec<&Hash> = whole.objects().keys().collect();
    let damaged = (*pick.get(&names)).clone();
    let resting: BTreeSet<Hash> = whole
        .objects()
        .keys()
        .filter(|name| whole.closure([(*name).clone()]).contains(&damaged))
        .cloned()
        .collect();
    let expected = without(&whole, &resting);
    let file = format!("{}.py", damaged.as_str());
    let path = store.root().join("objects").join(&file);
    let mut bytes = fs::read(&path).expect("reads");
    bytes.push(b' ');
    fs::write(&path, &bytes).expect("damages");
    prop_assert!(store.dag().is_err(), "a read refuses what fails its hash");

    let report = store.fsck();
    prop_assert!(
        report.contains(&Finding::Quarantined(file.clone())),
        "{:?}",
        report
    );
    prop_assert!(
        !report
            .iter()
            .any(|finding| matches!(finding, Finding::Unread { .. })),
        "nothing failing its hash is left among the objects: {:?}",
        report
    );
    let aside = store.root().join("quarantine");
    prop_assert_eq!(fs::read(aside.join(&file)).expect("kept"), bytes.clone());
    let folded = fold(&expected.nodes().expect("a down-set reads"));
    let fresh = Store::new(store.root(), store.policy().clone());
    for at in [store, &fresh] {
        prop_assert_eq!(&*at.dag().expect("reads"), &expected);
        prop_assert_eq!(&*at.folded().expect("folds"), &folded);
        prop_assert_eq!(at.tips().expect("derives"), expected.tips());
    }

    // Damaged again under the same name: set aside beside the first, which
    // is never overwritten, under a name that is its bytes' hash.
    let again = b"not an object".to_vec();
    fs::write(&path, &again).expect("damages again");
    let report = store.fsck();
    let receipts: Vec<String> = fs::read_dir(&aside)
        .expect("lists")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    prop_assert_eq!(receipts.len(), 2);
    prop_assert_eq!(fs::read(aside.join(&file)).expect("kept"), bytes);
    let second = receipts
        .iter()
        .find(|name| **name != file)
        .expect("the second");
    prop_assert_eq!(
        second,
        &format!("{}.{}.py", damaged.as_str(), sha256(&again))
    );
    prop_assert_eq!(fs::read(aside.join(second)).expect("kept"), again.clone());
    prop_assert!(report.contains(&Finding::Quarantined(second.clone())));
    prop_assert_eq!(&*store.dag().expect("reads"), &expected);

    // The same bytes set aside again are the same file: nothing new.
    fs::write(&path, &again).expect("damages the same way");
    let _ = store.fsck();
    prop_assert_eq!(fs::read_dir(&aside).expect("lists").count(), 2);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Law 40, quarantine: over two writers' changes and over a legacy
    /// chain, an object damaged on disk is set aside by `fsck`, named in its
    /// report and kept byte for byte, and the store, through the handle that
    /// read it whole and a fresh one, reads its objects, its fold and its
    /// tips as the history without it and everything resting on it. Damaged
    /// again, the second file is set aside beside the first.
    #[test]
    fn quarantine_is_the_down_set_without_what_rests_on_it(
        logs in two_writers(),
        log in a_log(),
        pick in any::<prop::sample::Index>(),
    ) {
        prop_assume!(!log.is_empty());
        let scratch = Scratch::new("quarantine");
        quarantine_reads_without_it(&changes(&scratch, &logs), pick)?;
        quarantine_reads_without_it(&legacy(&scratch, &log), pick)?;
    }
}
