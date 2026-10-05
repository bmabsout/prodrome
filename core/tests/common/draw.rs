//! THE HISTORY GENERATOR: a drawn history of any schema's events, as the
//! objects a store holds. One generator, read by the core's laws
//! (`all/schema_laws.rs`, `all/declared.rs`), and by any other crate's
//! tests that include it, never two.

use std::collections::BTreeSet;

use prodrome::change::mk_change;
use prodrome::event::{seal_hash, Envelope, Hash};
use prodrome::genesis::mk_genesis;
use prodrome::schema::Schema;
use proptest::prelude::*;

/// What one history is drawn as: each event with the earlier changes to its
/// entity it supersedes (a bit per earlier change), a rank per object for
/// the order it is folded in, the objects folded twice, and the objects one
/// replica starts from.
#[derive(Debug, Clone)]
pub struct Draw<E> {
    pub events: Vec<(E, u64)>,
    pub ranks: Vec<u32>,
    pub twice: Vec<usize>,
    pub replica: u64,
}

/// The objects: a genesis, and one change per event over the earlier
/// changes to its entity its bits name.
pub fn history<E: Schema>(events: &[(E, u64)]) -> Vec<(Hash, Envelope<E>)> {
    let genesis = Envelope::Genesis(mk_genesis("law 1", &"0".repeat(32)).expect("a genesis"));
    let root = seal_hash(&genesis);
    let mut objects = vec![(root.clone(), genesis)];
    let mut changes: Vec<(Hash, &E)> = Vec::new();
    for (event, bits) in events {
        let deps: BTreeSet<Hash> = changes
            .iter()
            .enumerate()
            .filter(|(i, (_, earlier))| bits >> (i % 64) & 1 == 1 && earlier.key() == event.key())
            .map(|(_, (name, _))| name.clone())
            .collect();
        let change = mk_change(root.clone(), deps.into_iter().collect(), event.clone())
            .expect("distinct deps");
        let object = Envelope::Change(change);
        let name = seal_hash(&object);
        changes.push((name.clone(), event));
        objects.push((name, object));
    }
    objects
}

/// A history of one to eleven of `event`'s draws, with what [`Draw`] folds it by.
pub fn a_draw<E: Schema>(event: impl Strategy<Value = E>) -> impl Strategy<Value = Draw<E>> {
    (
        prop::collection::vec((event, any::<u64>()), 1..12),
        prop::collection::vec(any::<u32>(), 13),
        prop::collection::vec(0usize..13, 0..4),
        any::<u64>(),
    )
        .prop_map(|(events, ranks, twice, replica)| Draw {
            events,
            ranks,
            twice,
            replica,
        })
}
