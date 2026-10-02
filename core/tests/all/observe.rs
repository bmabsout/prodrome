//! An observation is a quantisation of `[0, 1] ∪ {∅}`: monotone on numbers,
//! onto its steps' ends, and `∅` only for `∅`.
//!
//! And law 9 (`docs/design-register-types.md` §7): composed with a term, the
//! observed value is a step function of time, and `next_change` answers its
//! next step exactly on the exact fragment and never late anywhere.

use std::num::NonZeroU32;

use crate::common::terms::{a_closed_leaf, an_env, an_exact_term, grown_to, hours, moment, ok};
use chrono::Duration;
use prodrome::fpl::{self, Closed, Env, Instant, Outcome};
use prodrome::observe::{next_change, NextChange, Observation, Rounding};
use prodrome::term::Term;
use proptest::prelude::*;

fn an_observation() -> impl Strategy<Value = Observation> {
    (
        1u32..=1000,
        prop::sample::select(vec![Rounding::Down, Rounding::Nearest, Rounding::Up]),
    )
        .prop_map(|(levels, rounding)| {
            Observation::new(NonZeroU32::new(levels).expect("positive"), rounding)
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// A larger value is never observed as a smaller step, so each step
    /// observes an interval of values.
    #[test]
    fn an_observation_is_monotone(
        observation in an_observation(),
        a in 0.0f64..=1.0,
        b in 0.0f64..=1.0,
    ) {
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        prop_assert!(observation.observe(Some(low)) <= observation.observe(Some(high)));
    }

    /// The ends of `[0, 1]` are the first and last steps, and `∅` is observed
    /// as itself and is the only value that is.
    #[test]
    fn an_observation_keeps_its_ends_and_absence(
        observation in an_observation(),
        x in 0.0f64..=1.0,
    ) {
        prop_assert_eq!(observation.observe(Some(0.0)), Some(0));
        prop_assert_eq!(observation.observe(Some(1.0)), Some(observation.levels().get()));
        prop_assert_eq!(observation.observe(None), None);
        prop_assert!(observation.observe(Some(x)).is_some());
    }

    /// Down and up bracket the nearest, and agree exactly on the steps' ends.
    #[test]
    fn the_roundings_bracket_one_another(levels in 1u32..=1000, x in 0.0f64..=1.0) {
        let levels = NonZeroU32::new(levels).expect("positive");
        let read = |rounding| Observation::new(levels, rounding).observe(Some(x));
        prop_assert!(read(Rounding::Down) <= read(Rounding::Nearest));
        prop_assert!(read(Rounding::Nearest) <= read(Rounding::Up));
        prop_assert!(read(Rounding::Up) <= read(Rounding::Down).map(|step| step + 1));
    }
}

#[test]
fn a_percent_is_a_hundred_steps_to_the_nearest() {
    let percent = Observation::PERCENT;
    assert_eq!(percent.levels().get(), 100);
    assert_eq!(percent.rounding(), Rounding::Nearest);
    assert_eq!(percent.observe(Some(0.424)), Some(42));
    assert_eq!(percent.observe(Some(0.4251)), Some(43));
    assert_eq!(percent.observe(Some(0.999)), Some(100));
}

// --- Law 9: the observed value is a step function, and `next_change` its next step

/// A term the dense samplings below can afford: every constructor, and at
/// most one `Within` (nested windows multiply their 65 samples).
fn a_sampled_term() -> impl Strategy<Value = Term> {
    let small = || grown_to(a_closed_leaf(), 3, 16, false);
    prop_oneof![
        3 => small(),
        1 => (1i64..72, prop::sample::select(vec![-4.0, -1.0, 0.0]), small())
            .prop_map(|(w, p, t)| ok(fpl::mk_within(Duration::hours(w), p, t))),
    ]
}

/// An instant on the hourly grid or between two of its points.
fn an_instant() -> impl Strategy<Value = Instant> {
    (hours(), prop_oneof![Just(0i64), 0i64..3_600_000_000])
        .prop_map(|(h, us)| moment(h) + Duration::microseconds(us))
}

fn closed(term: &Term) -> Closed {
    Closed::of(term.clone()).expect("the generators build no Ref")
}

fn seen(term: &Closed, at: Instant, env: &Env, observation: Observation) -> Option<u32> {
    observation.observe(fpl::fulfillment(term, at, env))
}

/// How far a check looks for a term that never changes.
fn horizon() -> Duration {
    Duration::hours(2000)
}

/// Law 9 (a) and (b): constant on `[t₀, t₁)`, read at evenly spaced instants
/// and the last microsecond before `t₁`; and on the exact fragment, different
/// at `t₁`.
fn steps(
    term: &Term,
    env: &Env,
    now: Instant,
    observation: Observation,
) -> Result<(), TestCaseError> {
    let term = closed(term);
    let here = seen(&term, now, env, observation);
    let next = next_change(&term, now, env, observation);
    prop_assert_eq!(
        next,
        next_change(&term, now, env, observation),
        "a function of its arguments"
    );
    let until = next.at.unwrap_or(now + horizon());
    prop_assert!(until > now);
    for i in 0..256 {
        let at = now + (until - now) * i / 256;
        prop_assert_eq!(
            seen(&term, at, env, observation),
            here,
            "changed at {} before {:?}",
            at,
            next
        );
    }
    prop_assert_eq!(
        seen(&term, until - Duration::microseconds(1), env, observation),
        here
    );
    if let (true, Some(at)) = (next.exact, next.at) {
        prop_assert_ne!(seen(&term, at, env, observation), here, "no step at {}", at);
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Law 9 over the exact fragment: `t₁` is the observed value's next step.
    #[test]
    fn an_exact_term_answers_its_next_step(
        term in an_exact_term(),
        now in an_instant(),
        observation in an_observation(),
    ) {
        steps(&term, &Env::new(), now, observation)?;
        let next = next_change(&closed(&term), now, &Env::new(), observation);
        prop_assert!(next.exact);
    }

    /// Law 9 over every constructor and an environment: constant up to the
    /// answer, and a step there wherever the answer says it is exact.
    #[test]
    fn every_term_is_constant_until_its_answer(
        term in a_sampled_term(),
        env in an_env(),
        now in an_instant(),
        observation in an_observation(),
    ) {
        steps(&term, &env, now, observation)?;
    }

    /// Never late: `t₁` is no later than the first change an hourly sampling
    /// finds.
    #[test]
    fn the_answer_is_never_later_than_a_sampled_change(
        term in a_sampled_term(),
        env in an_env(),
        now in an_instant(),
        observation in an_observation(),
    ) {
        let term = closed(&term);
        let here = seen(&term, now, &env, observation);
        let next = next_change(&term, now, &env, observation);
        let found = (1..=horizon().num_hours())
            .map(|h| now + Duration::hours(h))
            .find(|at| seen(&term, *at, &env, observation) != here);
        if let Some(found) = found {
            prop_assert!(next.at.is_some_and(|at| at <= found), "{:?} after a change at {}", next, found);
        }
    }

    /// No state: a step answers the same end from every instant inside it,
    /// so a client that asks again mid-step is told the same instant.
    #[test]
    fn every_instant_of_a_step_answers_its_end(
        term in an_exact_term(),
        now in an_instant(),
        observation in an_observation(),
        k in 0i32..16,
    ) {
        let term = closed(&term);
        let next = next_change(&term, now, &Env::new(), observation);
        let until = next.at.unwrap_or(now + horizon());
        let inside = now + (until - now) * k / 16;
        prop_assert_eq!(next_change(&term, inside, &Env::new(), observation).at, next.at);
    }
}

fn decay() -> Term {
    ok(fpl::mk_decay(
        0.9,
        0.1,
        moment(100),
        Duration::hours(100),
        None,
    ))
}

/// A `Decay` read in whole percents: 0.98 until its window opens, then 0.9
/// falling a percent every 1.25 hours, the first half-percent in 37.5
/// minutes; and `end` for ever after its date.
#[test]
fn a_decay_steps_where_its_line_crosses_a_percent() {
    let term = closed(&decay());
    let env = Env::new();
    let next = |at| next_change(&term, at, &env, Observation::PERCENT);
    assert_eq!(
        next(moment(-50)),
        NextChange {
            at: Some(moment(0)),
            exact: true
        }
    );
    let first = next(moment(0)).at.expect("a step");
    let crossing = moment(0) + Duration::seconds(37 * 60 + 30);
    assert!(
        (first - crossing)
            .num_microseconds()
            .is_some_and(|us| us.abs() <= 1),
        "{first} vs {crossing}"
    );
    assert_eq!(next(moment(100)).at, None);
    assert_eq!(next(moment(400)).at, None);
}

#[test]
fn a_flat_term_never_changes() {
    let term = closed(&ok(fpl::mk_flat(0.42)));
    let next = next_change(&term, moment(0), &Env::new(), Observation::PERCENT);
    assert_eq!(
        next,
        NextChange {
            at: None,
            exact: true
        }
    );
}

/// A schedule that starts absent changes where its first piece starts, and a
/// coarser observation sees fewer steps than a finer one.
#[test]
fn a_schedule_steps_at_its_knots_and_a_coarse_view_steps_less() {
    let schedule = ok(fpl::mk_piecewise(
        fpl::mk_absent(),
        vec![(moment(10), decay())],
    ));
    let term = closed(&schedule);
    let env = Env::new();
    assert_eq!(
        next_change(&term, moment(0), &env, Observation::PERCENT).at,
        Some(moment(10))
    );
    let tenths = Observation::new(NonZeroU32::new(10).expect("positive"), Rounding::Down);
    let fine = next_change(&term, moment(10), &env, Observation::PERCENT).at;
    let coarse = next_change(&term, moment(10), &env, tenths).at;
    assert!(fine < coarse, "{fine:?} vs {coarse:?}");
}

/// A lookup of history is conservative, and as sharp as its parts: an `After`
/// whose two readings are flat changes where its binding comes into force,
/// and never where none is held.
#[test]
fn an_after_changes_where_its_binding_comes_into_force() {
    let after = ok(fpl::mk_after(
        "alpha".into(),
        moment(0),
        ok(fpl::mk_flat(0.3)),
        ok(fpl::mk_flat(0.6)),
        None,
    ));
    let term = closed(&after);
    let mut env = Env::new();
    assert_eq!(
        next_change(&term, moment(0), &env, Observation::PERCENT),
        NextChange {
            at: None,
            exact: true
        },
        "an enclosure that proves never is the step itself"
    );
    env.bind("alpha", Outcome::Completed(moment(50)));
    assert_eq!(
        next_change(&term, moment(0), &env, Observation::PERCENT),
        NextChange {
            at: Some(moment(50)),
            exact: false
        }
    );
}

/// A sampled composite of parts that move together is enclosed exactly by
/// its parts' ends, so its conservative answer is its step.
#[test]
fn a_conjunction_of_falling_parts_answers_its_step() {
    let falling = |end| {
        ok(fpl::mk_decay(
            0.9,
            0.1,
            moment(end),
            Duration::hours(100),
            None,
        ))
    };
    let term = closed(&ok(fpl::mk_conj(vec![falling(100), falling(150)], -4.0)));
    let env = Env::new();
    for now in [moment(10), moment(60), moment(120)] {
        let next = next_change(&term, now, &env, Observation::PERCENT);
        assert!(!next.exact);
        let at = next.at.expect("a falling term changes");
        let seen = |at| seen(&term, at, &env, Observation::PERCENT);
        assert_ne!(seen(at), seen(now), "a step at {at}");
        assert_eq!(seen(at - Duration::microseconds(1)), seen(now));
    }
}

/// Folding time onto a cycle, or windowing a flat term, moves nothing.
#[test]
fn a_cycle_or_a_window_of_a_flat_term_never_changes() {
    let flat = ok(fpl::mk_flat(0.25));
    let cycle = ok(fpl::mk_periodic(
        Duration::hours(24),
        moment(0),
        flat.clone(),
    ));
    let window = ok(fpl::mk_within(Duration::hours(24), -4.0, flat));
    for term in [cycle, window] {
        let next = next_change(&closed(&term), moment(3), &Env::new(), Observation::PERCENT);
        assert_eq!(
            next,
            NextChange {
                at: None,
                exact: true
            },
            "{term:?}"
        );
    }
}
