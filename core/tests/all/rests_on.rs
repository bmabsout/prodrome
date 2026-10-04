//! SPEC §3: WHAT AN OBJECT RESTS ON. Every object rests on the edges it
//! names and on the genesis of its prodrome where it names one, so a
//! `Genesis` is beneath everything begun from it and is a head only of a
//! prodrome with nothing else in it.

use std::collections::BTreeSet;

use prodrome::dag::Dag;
use prodrome::event::{canonical_envelope, mk_created, seal_hash, Envelope, Hash, TodoEvent};
use prodrome::genesis::mk_genesis;
use prodrome::literal::Datetime;
use prodrome::reference::Todo;
use prodrome::store::{Held, MemoryStore, Replica};

type Event = TodoEvent<Todo>;

fn created(todo: &str) -> Event {
    let at = Datetime::new(2026, 10, 3, 12, 0, 0, 0).expect("an instant");
    mk_created(todo, at, "writer", "", "").expect("valid")
}

/// A store in memory holding one genesis, whose name sorts AFTER the change
/// `created("first")` writes into it: the case a linearisation ordering
/// only by name would put backwards.
fn begun() -> (MemoryStore<Event>, Hash) {
    for nonce in 0u32.. {
        let genesis: Envelope<Event> =
            Envelope::Genesis(mk_genesis("rests-on", &format!("{nonce:032x}")).expect("a genesis"));
        let name = seal_hash(&genesis);
        let print = canonical_envelope(&genesis).into_bytes();
        let store = MemoryStore::default().in_genesis(name.clone());
        store
            .receive([name.clone()].into(), &|held| {
                (*held == name).then(|| print.clone())
            })
            .expect("receives its genesis");
        let probe = MemoryStore::default().in_genesis(name.clone());
        probe
            .receive([name.clone()].into(), &|held| {
                (*held == name).then(|| print.clone())
            })
            .expect("receives its genesis");
        if probe.append(created("first")).expect("appends") < name {
            return (store, name);
        }
    }
    unreachable!("some nonce names a genesis after its first change")
}

#[test]
fn a_genesis_is_beneath_its_first_change() {
    let (store, genesis) = begun();
    let change = store.append(created("first")).expect("appends");
    assert!(change < genesis, "the case under test");
    let Held { dag, tips, .. } = store.held().expect("reads");
    let only: BTreeSet<Hash> = [change.clone()].into();
    assert_eq!(tips, only, "the store's heads");
    assert_eq!(dag.tips(), only, "the DAG's tips");
    assert_eq!(
        dag.tips_in(&Some(genesis.clone())),
        only,
        "the prodrome's heads"
    );
    assert_eq!(
        dag.linearise().expect("orders"),
        vec![genesis.clone(), change.clone()],
        "the genesis first"
    );
    assert_eq!(
        dag.closure([change.clone()]),
        [genesis.clone(), change.clone()].into(),
        "a change's down-set holds its genesis"
    );
    let alone: Dag<Event> = dag
        .objects()
        .iter()
        .filter(|(name, _)| **name == change)
        .map(|(name, object)| (name.clone(), object.clone()))
        .collect();
    assert!(
        alone.interior().objects().is_empty(),
        "a change without its genesis is outside the history"
    );
}

/// A snapshot written before a genesis was beneath its changes names it
/// among its tips. It still verifies and attests what the snapshot written
/// now attests: the same down-set, so no stored object reads otherwise.
#[test]
fn a_snapshot_naming_its_genesis_as_a_tip_attests_as_it_did() {
    let (store, genesis) = begun();
    let change = store.append(created("first")).expect("appends");
    let snapshot = |tips: Vec<Hash>| {
        let object: Envelope<Event> = Envelope::Snapshot(
            prodrome::snapshot::mk_snapshot(genesis.clone(), tips, None).expect("a snapshot"),
        );
        (seal_hash(&object), object)
    };
    let (then, written_then) = snapshot(vec![genesis.clone(), change.clone()]);
    let (now, written_now) = snapshot(vec![change.clone()]);
    let Held { dag, .. } = store.held().expect("reads");
    let held: Dag<Event> = dag
        .objects()
        .iter()
        .map(|(name, object)| (name.clone(), object.clone()))
        .chain([(then.clone(), written_then), (now.clone(), written_now)])
        .collect();
    assert_eq!(held.verify(&prodrome::policy::Everything), Vec::new());
    let attests = |name: &Hash| {
        let mut down = held.closure([name.clone()]);
        down.remove(name);
        down
    };
    assert_eq!(attests(&then), attests(&now));
    assert_eq!(attests(&now), [genesis, change].into());
}
