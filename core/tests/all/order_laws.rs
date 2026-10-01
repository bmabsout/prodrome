//! The register types' laws (`docs/design-register-types.md` §7): each
//! order is a partial order, over generated values.
//!
//! Beside the todo vocabulary's orders, a small state machine that no
//! register of the vocabulary uses, so the laws see an order that is neither
//! discrete, total nor inclusion.

use std::collections::BTreeSet;

use prodrome::fold::{Discrete, Order, Total};
use proptest::prelude::*;

/// `Draft < Review < Merged` and `Draft < Closed`: further along is greater,
/// and `Merged` and `Closed` are the one incomparable pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Draft,
    Review,
    Merged,
    Closed,
}

impl Order for Phase {
    fn le(&self, other: &Self) -> bool {
        use Phase::*;
        matches!(
            (self, other),
            (Draft, _) | (Review, Review | Merged) | (Merged, Merged) | (Closed, Closed)
        )
    }
}

fn a_phase() -> impl Strategy<Value = Phase> {
    prop::sample::select(vec![
        Phase::Draft,
        Phase::Review,
        Phase::Merged,
        Phase::Closed,
    ])
}

/// Reflexive, antisymmetric and transitive at `a`, `b`, `c`.
fn is_partial_order<V: Order + PartialEq + std::fmt::Debug>(a: &V, b: &V, c: &V) {
    assert!(a.le(a), "reflexive at {a:?}");
    if a.le(b) && b.le(a) {
        assert_eq!(a, b, "antisymmetric");
    }
    if a.le(b) && b.le(c) {
        assert!(a.le(c), "transitive at {a:?} ≤ {b:?} ≤ {c:?}");
    }
}

fn a_set() -> impl Strategy<Value = BTreeSet<u8>> {
    prop::collection::btree_set(0u8..4, 0..4)
}

proptest! {
    /// Law 2: every order is a partial order.
    #[test]
    fn every_order_is_a_partial_order(
        discrete in prop::array::uniform3(0u8..3),
        total in prop::array::uniform3(0u8..3),
        sets in prop::array::uniform3(a_set()),
        phases in prop::array::uniform3(a_phase()),
    ) {
        let [a, b, c] = discrete.map(Discrete);
        is_partial_order(&a, &b, &c);
        let [a, b, c] = total.map(Total);
        is_partial_order(&a, &b, &c);
        let [a, b, c] = sets;
        is_partial_order(&a, &b, &c);
        let [a, b, c] = phases;
        is_partial_order(&a, &b, &c);
    }
}

#[test]
fn the_orders_are_the_ones_named() {
    assert!(!Discrete(1).le(&Discrete(2)) && !Discrete(2).le(&Discrete(1)));
    assert!(Total(1).le(&Total(2)) && !Total(2).le(&Total(1)));
    let (one, two, both) = (
        BTreeSet::from([1]),
        BTreeSet::from([2]),
        BTreeSet::from([1, 2]),
    );
    assert!(Order::le(&one, &both) && !Order::le(&one, &two));
    assert!(Phase::Draft.le(&Phase::Merged) && Phase::Review.le(&Phase::Merged));
    assert!(!Phase::Merged.le(&Phase::Closed) && !Phase::Closed.le(&Phase::Merged));
}
