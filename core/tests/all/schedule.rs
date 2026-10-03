//! A schedule MEANS a step function of time, and every operation is stated as
//! that meaning, `at`: the functor, the applicative, the monoid, the monad's
//! join, a shift in time, the normal form and the next change.

use std::collections::BTreeMap;

use crate::common::terms::moment;
use chrono::Duration;
use prodrome::fpl::Instant;
use prodrome::schedule::Schedule;
use proptest::prelude::*;

/// A schedule of small numbers on a small grid, so knots collide, repeat
/// their predecessor and land on the instants the laws read.
fn a_schedule() -> impl Strategy<Value = Schedule<i64>> {
    (
        0i64..4,
        prop::collection::btree_map(-12i64..12, 0i64..4, 0..8),
    )
        .prop_map(|(head, knots)| {
            Schedule::of(
                head,
                knots.into_iter().map(|(at, v)| (moment(at), v)).collect(),
            )
        })
}

/// Every instant the laws read: each grid hour and the half hours between,
/// past both ends.
fn instants() -> impl Iterator<Item = Instant> {
    (-30..30).map(|half| moment(0) + Duration::minutes(30 * half))
}

/// The meaning written out: the last knot at or before `t`, else the head.
fn meaning<A>(schedule: &Schedule<A>, t: Instant) -> &A {
    schedule
        .knots()
        .iter()
        .rev()
        .find(|(at, _)| *at <= t)
        .map_or(schedule.head(), |(_, value)| value)
}

proptest! {
    #![proptest_config(crate::common::cases::cases(256))]

    #[test]
    fn at_is_the_last_knot_at_or_before(s in a_schedule()) {
        for t in instants() {
            prop_assert_eq!(s.at(t), meaning(&s, t));
        }
    }

    /// Knots out of order are refused as a value, and in order are taken.
    #[test]
    fn new_takes_exactly_increasing_knots(
        knots in prop::collection::vec((-12i64..12, 0i64..4), 0..8),
    ) {
        let knots: Vec<(Instant, i64)> = knots.into_iter().map(|(at, v)| (moment(at), v)).collect();
        let increasing = knots.windows(2).all(|pair| pair[0].0 < pair[1].0);
        prop_assert_eq!(Schedule::new(0, knots).is_ok(), increasing);
    }

    #[test]
    fn map_is_the_functor(s in a_schedule(), k in 0i64..5) {
        let f = |x: i64| x * k + 1;
        let mapped = s.clone().map(f);
        for t in instants() {
            prop_assert_eq!(*mapped.at(t), f(*s.at(t)));
        }
        prop_assert_eq!(s.clone().map(|x| x), s.clone());
        prop_assert_eq!(s.clone().map(f).map(|x| x - 1), s.map(|x| f(x) - 1));
    }

    #[test]
    fn zip_is_pointwise(a in a_schedule(), b in a_schedule()) {
        let both = a.clone().zip(b.clone());
        for t in instants() {
            prop_assert_eq!(*both.at(t), (*a.at(t), *b.at(t)));
        }
    }

    /// `constant` is zip's unit, and zip is associative, up to meaning.
    #[test]
    fn constant_and_zip_are_an_applicative(
        a in a_schedule(),
        b in a_schedule(),
        c in a_schedule(),
        k in 0i64..4,
    ) {
        let unit = Schedule::constant(k).zip(a.clone()).map(|(_, x)| x);
        prop_assert_eq!(unit.normal(), a.clone().normal());
        let left = a.clone().zip(b.clone()).zip(c.clone()).map(|((x, y), z)| (x, y, z));
        let right = a.zip(b.zip(c)).map(|(x, (y, z))| (x, y, z));
        prop_assert_eq!(left, right);
    }

    #[test]
    fn sequence_is_every_value_at_once(schedules in prop::collection::vec(a_schedule(), 0..4)) {
        let all = Schedule::sequence(schedules.clone());
        for t in instants() {
            let each: Vec<i64> = schedules.iter().map(|s| *s.at(t)).collect();
            prop_assert_eq!(all.at(t), &each);
        }
    }

    /// Where `A` is a monoid the schedule is one, and `at` is a monoid
    /// homomorphism.
    #[test]
    fn add_is_pointwise_and_a_monoid(a in a_schedule(), b in a_schedule(), c in a_schedule()) {
        let sum = a.clone() + b.clone();
        for t in instants() {
            prop_assert_eq!(*sum.at(t), a.at(t) + b.at(t));
        }
        prop_assert_eq!((Schedule::default() + a.clone()).normal(), a.clone().normal());
        prop_assert_eq!((a.clone() + Schedule::default()).normal(), a.clone().normal());
        prop_assert_eq!(
            ((a.clone() + b.clone()) + c.clone()).normal(),
            (a.clone() + (b.clone() + c.clone())).normal()
        );
        let total: Schedule<i64> = [a.clone(), b.clone(), c.clone()].into_iter().sum();
        prop_assert_eq!(total.normal(), (a + b + c).normal());
    }

    /// The diagonal: at `t`, the schedule in force at `t`, read at `t`.
    #[test]
    fn join_is_the_diagonal(
        head in a_schedule(),
        outer in prop::collection::btree_map(-12i64..12, a_schedule(), 0..5),
    ) {
        let nested = Schedule::of(
            head,
            outer.into_iter().map(|(at, s)| (moment(at), s)).collect(),
        );
        let joined = nested.clone().join();
        for t in instants() {
            prop_assert_eq!(joined.at(t), nested.at(t).at(t));
        }
    }

    /// The monad's laws, up to meaning: a constant outer or inner layer
    /// joins to the schedule, and nesting three deep joins either way.
    #[test]
    fn join_has_the_monads_laws(s in a_schedule(), n in prop::collection::vec(a_schedule(), 0..4)) {
        prop_assert_eq!(Schedule::constant(s.clone()).join(), s.clone());
        prop_assert_eq!(s.clone().map(Schedule::constant).join(), s.clone());
        let deep = Schedule::constant(Schedule::sequence(n.clone()).map(|_| s.clone()))
            .map(|inner| inner.map(Schedule::constant));
        prop_assert_eq!(
            deep.clone().join().join().normal(),
            deep.map(Schedule::join).join().normal()
        );
    }

    #[test]
    fn shift_reads_later(s in a_schedule(), by in -6i64..6) {
        let by = Duration::hours(by);
        let shifted = s.clone().shift(by);
        for t in instants() {
            prop_assert_eq!(shifted.at(t), s.at(t + by));
        }
    }

    /// The normal form reads as the schedule, is a fixed point, and is
    /// equal for two schedules exactly when they read alike everywhere.
    #[test]
    fn normal_is_the_quotient_by_meaning(a in a_schedule(), b in a_schedule()) {
        let normal = a.clone().normal();
        for t in instants() {
            prop_assert_eq!(normal.at(t), a.at(t));
        }
        prop_assert_eq!(normal.clone().normal(), normal.clone());
        let alike = instants().all(|t| a.at(t) == b.at(t));
        prop_assert_eq!(normal == b.normal(), alike);
    }

    /// The push half: constant from `t` until the answer, different there,
    /// and constant for ever where there is none.
    #[test]
    fn next_change_is_the_first_knot_that_differs(s in a_schedule()) {
        for t in instants() {
            let next = s.next_change(t);
            let later = instants().filter(|u| *u > t);
            match next {
                Some(next) => {
                    prop_assert!(next > t);
                    prop_assert_ne!(s.at(next), s.at(t));
                    for u in later.filter(|u| *u < next) {
                        prop_assert_eq!(s.at(u), s.at(t));
                    }
                }
                None => {
                    for u in later {
                        prop_assert_eq!(s.at(u), s.at(t));
                    }
                }
            }
            prop_assert_eq!(next, s.clone().normal().next_change(t));
        }
    }

    /// A schedule is a function of its knots as a set: built from them in
    /// any order, it is one schedule.
    #[test]
    fn of_is_order_free(
        (knots, shuffled) in prop::collection::btree_map(-12i64..12, 0i64..4, 0..8)
            .prop_flat_map(|knots| {
                let pairs: Vec<(i64, i64)> = knots.into_iter().collect();
                (Just(pairs.clone()), Just(pairs).prop_shuffle())
            }),
    ) {
        let build = |pairs: Vec<(i64, i64)>| {
            Schedule::of(0, pairs.into_iter().map(|(at, v)| (moment(at), v)).collect::<BTreeMap<_, _>>())
        };
        prop_assert_eq!(build(knots), build(shuffled));
    }
}
