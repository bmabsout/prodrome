//! Calibration harness: pins what the composed numbers MEAN, not what the
//! evaluator computes. The golden vectors (`fpl_vectors.rs`) prove equivalence
//! with production history; these prove the properties an author leans on
//! when trusting a briefing's ordering:
//!
//!   monotone — more slippage never lowers urgency;
//!   spanning — the scenario set actually uses the range;
//!   smooth   — no cliffs except at declared breakpoints.
//!
//! Protocol adopted 2026-08-31 from the FPL research repo's calibrate.py
//! review: calibration failures, not composition choices, are what make a
//! spec silently wrong (its 004/025/046). Scenarios below are todo-shaped on
//! purpose — each is a situation the briefing must rank correctly, not an
//! abstract property.
//!
//! If a number here ever disagrees with the Python this ported from
//! (`tests/test_fpl_calibration.py`), that disagreement is the exact failure
//! this file exists to catch — it gets reported, not adjusted away.

use chrono::{Duration, NaiveDate};
use prodrome::fpl::{
    self, fulfillment, mk_after, mk_conj, mk_decay, mk_flat, mk_gate, mk_within, Env, FplError,
    Instant, Outcome, Term, PRIORITY_POWER,
};

fn ok(t: Result<Term, FplError>) -> Term {
    t.expect("the harness only builds terms the constructors admit")
}

fn now() -> Instant {
    NaiveDate::from_ymd_opt(2026, 8, 31)
        .and_then(|d| d.and_hms_opt(12, 0, 0))
        .expect("a real date")
}

fn in_days(n: i64) -> Instant {
    now() + Duration::days(n)
}

/// A Decay with the production defaults (`start=0.55, end=0.05,
/// lead_up=1 week`), due `days_out` from `now`.
fn deadline(days_out: i64) -> Term {
    deadline_with(days_out, 0.55, 0.05, Duration::weeks(1))
}

fn deadline_with(days_out: i64, start: f64, end: f64, lead_up: Duration) -> Term {
    ok(mk_decay(start, end, in_days(days_out), lead_up, None))
}

// --- OrderingAcrossTodos -----------------------------------------------
// The briefing sorts ascending by fulfillment; these scenarios must come out
// in stakes order or every morning's list is quietly wrong.

/// The scenario set ORDERS the way a briefing must rank it: a blown deadline
/// reads worst, a far-future todo reads best, and every step between is
/// strictly distinct — no two todos may tie for a slot in the list.
#[test]
fn stakes_order_the_list() {
    let env = Env::new();
    let blown = deadline(-2); // missed: pinned at end (screams, forever)
    let tomorrow = deadline(1);
    let mid_window = deadline(4);
    let far_out = deadline(30); // not yet in play: 0.98
    let vals: Vec<f64> = [&blown, &tomorrow, &mid_window, &far_out]
        .iter()
        .map(|t| fulfillment(t, now(), &env))
        .collect();
    let mut sorted = vals.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in a fulfillment"));
    assert_eq!(vals, sorted, "the scenario set is out of stakes order");
    for w in vals.windows(2) {
        assert!(
            w[0] < w[1],
            "a tie breaks the ordering: {} vs {}",
            w[0],
            w[1]
        );
    }
}

/// The range is actually used: a blown deadline and a far-future todo must
/// not huddle in the middle of `[0, 1]`.
#[test]
fn spanning() {
    let env = Env::new();
    let lo = fulfillment(&deadline(-2), now(), &env);
    let hi = fulfillment(&deadline(30), now(), &env);
    assert!(hi - lo >= 0.8, "the scenario set huddles: lo={lo} hi={hi}");
}

// --- MonotoneSlippage ----------------------------------------------------
// More slippage never lowers urgency: fulfillment is non-increasing in time
// for every authored decay shape, sampled daily from a month before the
// window to a month past the deadline.

fn monotone_specs() -> Vec<Term> {
    vec![
        deadline(10),
        deadline_with(10, 0.72, 0.0, Duration::weeks(8)),
        ok(mk_decay(
            0.60,
            0.35,
            in_days(10),
            Duration::weeks(3),
            Some(in_days(-8)),
        )),
        // start AT the pre-window value: legal edge.
        ok(mk_decay(0.98, 0.05, in_days(10), Duration::weeks(1), None)),
    ]
}

#[test]
fn decay_urgency_never_decreases() {
    let env = Env::new();
    for spec in monotone_specs() {
        let grid: Vec<f64> = (-30..45)
            .map(|d| fulfillment(&spec, in_days(d), &env))
            .collect();
        for w in grid.windows(2) {
            assert!(
                w[1] <= w[0] + 1e-12,
                "urgency dropped as time passed: {} -> {}",
                w[0],
                w[1]
            );
        }
    }
}

/// `start > 0.98` would make urgency DROP on window entry — the one
/// monotonicity hole in the shape, closed at the constructor.
#[test]
fn start_above_pre_window_value_is_rejected() {
    assert!(fpl::mk_decay(0.99, 0.05, in_days(10), Duration::weeks(1), None).is_err());
}

// --- Smoothness ------------------------------------------------------------
// No cliffs except at declared breakpoints. Inside the window the daily step
// is the analytic slope; the only jumps allowed are window entry (0.98 →
// start) and the start_date boundary — both stated semantics.

#[test]
fn daily_step_matches_slope_inside_window() {
    let spec = ok(mk_decay(0.70, 0.0, in_days(28), Duration::weeks(8), None));
    let slope_per_day = (0.70 - 0.0) / 56.0;
    let env = Env::new();
    let vals: Vec<f64> = (-27..28)
        .map(|d| fulfillment(&spec, in_days(d), &env))
        .collect();
    for w in vals.windows(2) {
        assert!(
            (w[0] - w[1] - slope_per_day).abs() < 1e-9,
            "the daily step ({}) does not match the analytic slope ({slope_per_day})",
            w[0] - w[1]
        );
    }
}

#[test]
fn cliffs_only_at_declared_breakpoints() {
    let spec = ok(mk_decay(0.55, 0.05, in_days(10), Duration::weeks(1), None)); // window opens at day 3
    let slope_per_day = (0.55 - 0.05) / 7.0;
    let env = Env::new();
    for d in -30..45 {
        let step = fulfillment(&spec, in_days(d), &env) - fulfillment(&spec, in_days(d + 1), &env);
        if !(3..=4).contains(&(d + 1)) {
            // the sole window-entry crossing
            assert!(
                step <= slope_per_day + 1e-9,
                "a cliff appeared outside the declared breakpoint, at day {d}: step {step}"
            );
        }
    }
}

// --- ZeroSemantics -----------------------------------------------------
// A blown sub-goal at strict p: the decision, pinned — it SCREAMS, it does
// not vanish. One 0-member drives the conjunction to the floor.

#[test]
fn zero_member_screams_not_vanishes() {
    let c = ok(mk_conj(
        vec![ok(mk_flat(0.0)), ok(mk_flat(0.9))],
        PRIORITY_POWER,
    ));
    let env = Env::new();
    assert!(fulfillment(&c, now(), &env) < 0.02);
}

/// The H7 flat band under the 0.001 clamp: ordering among conjunctions
/// sharing a 0-member survives, but only at ~1e-11 relative — this test pins
/// that it DOES survive. If authored specs start living here, port the
/// thesis slack form (see `power_mean`'s docstring in the Python) and re-pin.
#[test]
fn near_zero_band_stays_ordered() {
    let env = Env::new();
    let worse = ok(mk_conj(
        vec![ok(mk_flat(0.0)), ok(mk_flat(0.5))],
        PRIORITY_POWER,
    ));
    let better = ok(mk_conj(
        vec![ok(mk_flat(0.0)), ok(mk_flat(0.9))],
        PRIORITY_POWER,
    ));
    assert!(fulfillment(&worse, now(), &env) < fulfillment(&better, now(), &env));
}

/// Dominated-by-least is the point of p = −4: the composed value sits near
/// the min, far below the arithmetic mean.
#[test]
fn conjunction_tracks_worst_member() {
    let c = ok(mk_conj(
        vec![ok(mk_flat(0.2)), ok(mk_flat(0.9))],
        PRIORITY_POWER,
    ));
    let env = Env::new();
    let v = fulfillment(&c, now(), &env);
    assert!(v < 0.3);
    assert!(v >= 0.2);
}

// --- GateCalibration -----------------------------------------------------
// Gate's provisional Kleene semantics under a DECAYING gate: as the
// prerequisite grows more urgent (less fulfilled), the gated body reads MORE
// satisfied — less actionable, because its prerequisite is further from
// done. Pinned here as the current, stated choice; the authored-blend
// parameter on After (research-repo rec 2) is the designed successor.

#[test]
fn decaying_gate_releases_body_over_time() {
    let gated = ok(mk_gate(deadline(10), ok(mk_flat(0.3))));
    let env = Env::new();
    let early = fulfillment(&gated, in_days(4), &env); // gate mid-decay
    let late = fulfillment(&gated, in_days(20), &env); // gate blown, pinned at 0.05
    assert!(early <= late);
    assert!(
        (late - 0.95).abs() < 0.005,
        "expected 1 − 0.05, not body's 0.3: got {late}"
    );
}

#[test]
fn fulfilled_gate_hands_over_to_body() {
    let gated = ok(mk_gate(ok(mk_flat(1.0)), ok(mk_flat(0.3))));
    let env = Env::new();
    assert_eq!(fulfillment(&gated, now(), &env), 0.3);
}

// --- WithinCliff -----------------------------------------------------------
// onpolicy-fpl 053, arriving by the todo route: at strict p, a long window
// containing one near-zero stretch reads ~0 regardless of the rest — the
// flat-then-cliff landscape. Pinned so the failure is a documented shape,
// and so the 057 remedy (a conjunction of horizons) is demonstrated to
// actually restore the ordering the long window destroys.

// Both hit zero at their deadline; A is comfortable now, B is urgent now.
fn horizon_a() -> Term {
    deadline_with(9, 0.72, 0.0, Duration::weeks(2))
}
fn horizon_b() -> Term {
    deadline_with(9, 0.25, 0.0, Duration::weeks(2))
}

#[test]
fn long_strict_window_collapses_the_ordering() {
    let env = Env::new();
    // A 3-week window at p=-8 swallows the post-deadline zeros: both todos
    // read the floor and the between-todos gradient is gone.
    let long_a = fulfillment(
        &ok(mk_within(Duration::weeks(3), -8.0, horizon_a())),
        now(),
        &env,
    );
    let long_b = fulfillment(
        &ok(mk_within(Duration::weeks(3), -8.0, horizon_b())),
        now(),
        &env,
    );
    assert!(long_a < 0.01);
    assert!(long_b < 0.01);
    // Not merely low — INDISTINGUISHABLE: the comfortable and the urgent
    // todo agree to within 5%, which is the actual damage.
    assert!(long_a < 1.05 * long_b);
}

#[test]
fn multi_horizon_conjunction_restores_the_gradient() {
    // Harness finding while writing this test (carried over from the Python):
    // a STRICT outer p re-admits the cliff — at p=-4 across horizons the
    // collapsed long window dominates the meta-conjunction and A/B agreed to
    // 3e-7. Strictness belongs inside each horizon; ACROSS horizons the
    // meta-conjunction wants the geometric mean (their 055 rhymes:
    // strictness through the window read is ruinous, it must enter
    // elsewhere).
    fn horizons(t: &Term) -> f64 {
        let env = Env::new();
        let c = ok(mk_conj(
            vec![
                ok(mk_within(Duration::days(1), -4.0, t.clone())),
                ok(mk_within(Duration::days(7), -4.0, t.clone())),
                ok(mk_within(Duration::weeks(3), -4.0, t.clone())),
            ],
            0.0,
        ));
        fulfillment(&c, now(), &env)
    }
    assert!(horizons(&horizon_a()) > 1.5 * horizons(&horizon_b()));
}

// --- AfterCalibration --------------------------------------------------
// The dependency combinator's meaning under slippage, plus the
// toward-failure axis calibrate.py insists on: degrade toward the specific
// failure the construct exists to handle, not just away from competence.

fn after_body() -> Term {
    // due ~1mo after anchor
    ok(mk_decay(
        0.6,
        0.05,
        now() + Duration::days(31),
        Duration::weeks(1),
        None,
    ))
}

fn after_dep() -> Term {
    ok(mk_after(
        "exercise".to_string(),
        now() + Duration::days(1),
        after_body(),
        ok(mk_flat(0.7)),
        None,
    ))
}

/// A later completion only ever slides the deadline later, so at a fixed
/// reading time more slippage never reads MORE urgent.
#[test]
fn slippage_monotone() {
    let dep = after_dep();
    let mut readings = Vec::new();
    for k in 1..14i64 {
        let mut env = Env::new();
        env.insert("exercise".to_string(), Outcome::Completed(in_days(k)));
        readings.push(fulfillment(&dep, in_days(28), &env));
    }
    for w in readings.windows(2) {
        assert!(
            w[1] >= w[0] - 1e-12,
            "more slippage read more urgent: {} -> {}",
            w[0],
            w[1]
        );
    }
}

/// Pricing says moot (1.0) — and the calibration point is that this is
/// DISTINCT from both unbound (pending: 0.7) and completed (body's own
/// value), so the reporting layer has something to key its "confirm moot"
/// surfacing on. Collapsing these fates is the mispricing onpolicy-fpl
/// measured at +47% (059).
#[test]
fn toward_failure_cancellation_is_moot_not_silent_urgency() {
    let dep = after_dep();
    let reading_at = in_days(28);

    let mut cancelled_env = Env::new();
    cancelled_env.insert("exercise".to_string(), Outcome::Cancelled(in_days(2)));
    let cancelled = fulfillment(&dep, reading_at, &cancelled_env);

    let unbound = fulfillment(&dep, reading_at, &Env::new());

    let mut completed_env = Env::new();
    completed_env.insert("exercise".to_string(), Outcome::Completed(in_days(1)));
    let completed = fulfillment(&dep, reading_at, &completed_env);
    let completed_rounded = (completed * 1e6).round() / 1e6;

    assert_eq!(cancelled, 1.0);
    assert_ne!(
        cancelled, unbound,
        "cancelled and unbound must not collapse"
    );
    assert_ne!(
        cancelled, completed_rounded,
        "cancelled and completed must not collapse"
    );
    assert_ne!(
        unbound, completed_rounded,
        "unbound and completed must not collapse"
    );
}
