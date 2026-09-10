//! SPEC §9.7/§9.8 over `conformance/series.py`: 60 random terms drawn over a
//! window. The knot INSTANTS are exact — they are the term's own transitions —
//! and the values agree to 1e-9. Beside that, the law: between two adjacent
//! knots of an exact fragment, the straight line IS the evaluator.

mod common;

use chrono::Duration;
use common::vectors::{boolean, each, field, integer, moment, number, text_at, vectors};
use prodrome::breaks::series_knots;
use prodrome::fpl::{fulfillment, instant_of, parse_term, Env, Instant, Term};

/// The series vectors are drawn against an empty history, as the reference's
/// generator does: `history([])` binds nothing at any instant.
fn nothing(_: Instant) -> Env {
    Env::new()
}

/// One case's term, its window and its seed — the three every test here starts
/// from.
fn case_of(case: &prodrome::literal::Value) -> (i64, Term, Instant, Instant) {
    let seed = integer(field(case, "seed"));
    let term = parse_term(text_at(case, "term"))
        .unwrap_or_else(|e| panic!("seed {seed}: cannot parse: {e}"));
    (
        seed,
        term,
        instant_of(moment(field(case, "from"))),
        instant_of(moment(field(case, "to"))),
    )
}

#[test]
fn every_series_has_the_reference_knots() {
    let data = vectors("series.py");
    let cases = each(&data, "series");
    assert_eq!(cases.len(), 60, "the vector file lost cases");
    let (mut knots, mut exact_cases) = (0usize, 0usize);
    for case in cases {
        let (seed, term, from, to) = case_of(case);
        let series = series_knots(&term, from, to, nothing);
        assert_eq!(
            series.exact,
            boolean(field(case, "exact")),
            "seed {seed}: exactness"
        );
        let frozen = each(case, "knots");
        assert_eq!(series.knots.len(), frozen.len(), "seed {seed}: knot count");
        for (mine, theirs) in series.knots.iter().zip(frozen) {
            // The INSTANT exactly — it is the term's own transition, not a
            // sample — and the value to 1e-9.
            assert_eq!(
                mine.at,
                instant_of(moment(field(theirs, "at"))),
                "seed {seed}: knot instant"
            );
            common::close("knot", mine.value, number(field(theirs, "value")))
                .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            knots += 1;
        }
        if series.exact {
            exact_cases += 1;
        }
    }
    println!(
        "series.py: {} windows ({exact_cases} exact), {knots} knots; \
         largest float deviation {:e}",
        cases.len(),
        common::worst_seen()
    );
}

/// §9.7: on an EXACT fragment, interpolation between two adjacent knots equals
/// evaluation. Checked at nine points inside every interval of every exact
/// vector — this is the claim the graph rests on, and the reason `exact` is
/// reported rather than assumed.
///
/// The one-second interval that BRACKETS a jump is excluded, and deliberately:
/// that pair exists precisely so a discontinuity is drawn as a near-vertical
/// step instead of a ramp across the whole gap, and inside that second the line
/// is the step. Every other interval is the curve exactly.
#[test]
fn interpolation_between_exact_knots_equals_evaluation() {
    let data = vectors("series.py");
    let mut probes = 0usize;
    let mut brackets = 0usize;
    let mut worst = 0.0f64;
    for case in each(&data, "series") {
        if !boolean(field(case, "exact")) {
            continue;
        }
        let (seed, term, from, to) = case_of(case);
        let series = series_knots(&term, from, to, nothing);
        for pair in series.knots.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let span = (b.at - a.at)
                .num_microseconds()
                .expect("a knot interval fits");
            if span <= 1_000_000 {
                brackets += 1;
                continue;
            }
            for step in 1..10 {
                let at = a.at + Duration::microseconds(span * step / 10);
                let frac = (at - a.at).num_microseconds().expect("inside the interval") as f64
                    / span as f64;
                let drawn = a.value + (b.value - a.value) * frac;
                let real = fulfillment(&term, at, &Env::new());
                worst = worst.max((drawn - real).abs());
                assert!(
                    (drawn - real).abs() <= 1e-9,
                    "seed {seed} at {at}: the line reads {drawn}, the evaluator {real}"
                );
                probes += 1;
            }
        }
    }
    println!(
        "exact fragments: {probes} interpolation probes over the ramps \
         ({brackets} jump brackets skipped), worst gap {worst:e}"
    );
}
