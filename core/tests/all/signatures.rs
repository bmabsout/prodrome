//! Law 41 (SPEC §3, §5): who wrote an object, proven by the history.
//!
//! A generated history is a log of writes by two actors, and then keys
//! added and revoked (each by a root's signature or by nobody's), honest
//! signatures by registered keys, by unregistered ones and by a root, and
//! forged ones, over any object written so far, signatures included. The
//! proof it gives is a function of its objects: replicas that received
//! them one at a time in any order, or in two halves joined by sync, read
//! the same proof as the whole.

use std::collections::{BTreeMap, BTreeSet};

use prodrome::dag::Dag;
use prodrome::event::{canonical_envelope, seal_hash, Actor, Envelope, Hash, TodoEvent};
use prodrome::genesis::mk_genesis;
use prodrome::reference::Todo;
use prodrome::sign::{mk_key_revoked, KeyAdded, Proof, Registrar, Secret, Signed};
use prodrome::store::{sync, MemoryStore, Replica};
use proptest::prelude::*;

use crate::common::a_log;

type Event = TodoEvent<Todo>;
type Object = Envelope<Event>;

const ACTORS: [&str; 2] = ["bassel", "triage"];

/// The devices' keys, and the root's, which registers them.
fn keys() -> [Secret; 3] {
    ['1', '2', '3'].map(|seed| Secret::new(&seed.to_string().repeat(64)).expect("a seed"))
}

fn root() -> Secret {
    Secret::new(&"9".repeat(64)).expect("a seed")
}

fn roots() -> Registrar {
    Registrar::Roots([root().public()].into())
}

/// One step after the writes.
#[derive(Debug, Clone)]
pub enum Step {
    Add {
        actor: usize,
        key: usize,
        rooted: bool,
    },
    Revoke {
        actor: usize,
        key: usize,
        keep: Vec<prop::sample::Index>,
        rooted: bool,
    },
    /// By a device's key, or by the root's (`key == 3`).
    Sign {
        key: usize,
        object: prop::sample::Index,
    },
    Forge {
        key: usize,
        object: prop::sample::Index,
    },
}

fn a_step() -> impl Strategy<Value = Step> {
    prop_oneof![
        2 => (0..2usize, 0..3usize, prop::bool::weighted(0.8))
            .prop_map(|(actor, key, rooted)| Step::Add { actor, key, rooted }),
        1 => (
            0..2usize,
            0..3usize,
            prop::collection::vec(any::<prop::sample::Index>(), 0..4),
            prop::bool::weighted(0.8),
        )
            .prop_map(|(actor, key, keep, rooted)| Step::Revoke { actor, key, keep, rooted }),
        6 => (0..4usize, any::<prop::sample::Index>())
            .prop_map(|(key, object)| Step::Sign { key, object }),
        1 => (0..3usize, any::<prop::sample::Index>())
            .prop_map(|(key, object)| Step::Forge { key, object }),
    ]
}

/// A history's objects by name, each with its print.
#[derive(Debug, Clone)]
pub struct History {
    pub prints: BTreeMap<Hash, Vec<u8>>,
}

impl History {
    pub fn dag(&self) -> Dag<Event> {
        Dag::from_prints(
            &Default::default(),
            self.prints
                .iter()
                .map(|(name, print)| (name.clone(), Ok(print.clone()))),
        )
    }

    /// A replica in memory given `names`, one at a time in this order: each
    /// receipt takes in what its object rests on first.
    pub fn received(&self, names: impl IntoIterator<Item = Hash>) -> MemoryStore<Event> {
        let replica = MemoryStore::default();
        for name in names {
            replica
                .receive([name].into(), &|name| self.prints.get(name).cloned())
                .expect("every print is held");
        }
        replica
    }

    /// Two replicas, each given the names `halves` deals it (the second in
    /// reverse), the first then synced into the second.
    pub fn joined(&self, halves: &[bool]) -> MemoryStore<Event> {
        let (mut mine, mut theirs) = (Vec::new(), Vec::new());
        for (name, to_mine) in self.prints.keys().zip(halves.iter().cycle()) {
            if *to_mine {
                mine.push(name.clone());
            } else {
                theirs.push(name.clone());
            }
        }
        let mine = self.received(mine);
        let theirs = self.received(theirs.into_iter().rev());
        sync(&mine, &theirs).expect("syncs");
        theirs
    }
}

fn put(prints: &mut BTreeMap<Hash, Vec<u8>>, object: &Object) -> Hash {
    let name = seal_hash(object);
    prints.insert(name.clone(), canonical_envelope(object).into_bytes());
    name
}

/// The writes of `log` appended over one genesis, then `steps`.
pub fn build(log: &[Event], steps: &[Step]) -> History {
    let mut prints = BTreeMap::new();
    let genesis = put(
        &mut prints,
        &Envelope::Genesis(mk_genesis("signed", &"cd".repeat(16)).expect("a genesis")),
    );
    let store = MemoryStore::<Event>::default();
    store
        .receive([genesis.clone()].into(), &|name| prints.get(name).cloned())
        .expect("a genesis");
    let store = store.in_genesis(genesis.clone());
    for event in log {
        let name = store.append(event.clone()).expect("appends");
        prints.insert(name.clone(), store.print(&name).expect("held"));
    }
    let (keys, root) = (keys(), root());
    let mut signatures: Vec<Hash> = Vec::new();
    for step in steps {
        let objects: Vec<Hash> = prints.keys().cloned().collect();
        match step {
            Step::Add { actor, key, rooted } => {
                let added = put(
                    &mut prints,
                    &Envelope::KeyAdded(KeyAdded {
                        genesis: genesis.clone(),
                        actor: Actor::new(ACTORS[*actor]).expect("an actor"),
                        key: keys[*key].public(),
                    }),
                );
                if *rooted {
                    signatures.push(put(&mut prints, &Envelope::Signed(root.sign(&added))));
                }
            }
            Step::Revoke {
                actor,
                key,
                keep,
                rooted,
            } => {
                let deps: BTreeSet<Hash> = keep
                    .iter()
                    .filter(|_| !signatures.is_empty())
                    .map(|index| index.get(&signatures).clone())
                    .collect();
                let revoked = mk_key_revoked(
                    genesis.clone(),
                    deps.into_iter().collect(),
                    Actor::new(ACTORS[*actor]).expect("an actor"),
                    keys[*key].public(),
                )
                .expect("distinct");
                let revoked = put(&mut prints, &Envelope::KeyRevoked(revoked));
                if *rooted {
                    signatures.push(put(&mut prints, &Envelope::Signed(root.sign(&revoked))));
                }
            }
            Step::Sign { key, object } => {
                let by = keys.get(*key).unwrap_or(&root);
                let signed = by.sign(object.get(&objects));
                signatures.push(put(&mut prints, &Envelope::Signed(signed)));
            }
            Step::Forge { key, object } => {
                let forged = Signed {
                    object: object.get(&objects).clone(),
                    ..keys[*key].sign(&Hash::new("0".repeat(64)).expect("a name"))
                };
                signatures.push(put(&mut prints, &Envelope::Signed(forged)));
            }
        }
    }
    History { prints }
}

pub fn a_signed_history() -> impl Strategy<Value = History> {
    (a_log(), prop::collection::vec(a_step(), 0..24)).prop_map(|(log, steps)| build(&log, &steps))
}

/// A permutation of `names`, drawn by `draws`.
pub fn shuffled(names: &[Hash], draws: &[prop::sample::Index]) -> Vec<Hash> {
    let mut left = names.to_vec();
    let mut out = Vec::with_capacity(left.len());
    for draw in draws.iter().cycle().take(left.len()) {
        out.push(left.remove(draw.index(left.len())));
    }
    out
}

fn proofs(dag: &Dag<Event>) -> (Proof, Proof) {
    (Proof::of(dag, &Registrar::Anyone), Proof::of(dag, &roots()))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// A signature's validity is a function of the object set: a replica
    /// that received the objects one at a time in any order, and one that
    /// received two halves apart and joined them by sync, prove what the
    /// whole set proves, under either reading of the keys.
    #[test]
    fn a_proof_is_a_function_of_the_object_set(
        history in a_signed_history(),
        draws in prop::collection::vec(any::<prop::sample::Index>(), 1..16),
        halves in prop::collection::vec(any::<bool>(), 1..16),
    ) {
        let whole = proofs(&history.dag());
        let names: Vec<Hash> = history.prints.keys().cloned().collect();
        let one_at_a_time = history.received(shuffled(&names, &draws));
        prop_assert_eq!(&proofs(&one_at_a_time.held().expect("reads").dag), &whole);

        let joined = history.joined(&halves);
        prop_assert_eq!(&proofs(&joined.held().expect("reads").dag), &whole);
    }
}
