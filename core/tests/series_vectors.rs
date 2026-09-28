//! SPEC §9.7/§9.8 over `conformance/series.py`: 60 random terms drawn over a
//! window. The knot INSTANTS are exact — they are the term's own transitions —
//! and the values agree to 1e-9. Beside that, the law: between two adjacent
//! knots of an exact fragment, the straight line IS the evaluator.

mod common;

use chrono::Duration;
use common::vectors::{boolean, each, field, integer, moment, text_at, vectors};
use prodrome::breaks::series_knots;
use prodrome::fpl::{fulfillment, instant_of, parse_term, Closed, Env, Instant};

/// The series vectors are drawn against an empty history, as the reference's
/// generator does: `history([])` binds nothing at any instant.
fn nothing(_: Instant) -> Env {
    Env::new()
}

/// One case's term, its window and its seed — the three every test here starts
/// from.
fn case_of(case: &prodrome::literal::Value) -> (i64, Closed, Instant, Instant) {
    let seed = integer(field(case, "seed"));
    let term = parse_term(text_at(case, "term"))
        .unwrap_or_else(|e| panic!("seed {seed}: cannot parse: {e}"));
    let term = Closed::of(term).expect("the vectors hold no Ref");
    (
        seed,
        term,
        instant_of(moment(field(case, "from"))),
        instant_of(moment(field(case, "to"))),
    )
}

/// Every case of `file` against its frozen knots: the instants exactly, the
/// readings as §9.8 compares them.
fn replay(file: &str, expected: usize) {
    let data = vectors(file);
    let cases = each(&data, "series");
    assert_eq!(cases.len(), expected, "{file} lost cases");
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
            // sample — and the value to 1e-9, or `∅` exactly.
            assert_eq!(
                mine.at,
                instant_of(moment(field(theirs, "at"))),
                "seed {seed}: knot instant"
            );
            common::reading("knot", mine.value, field(theirs, "value"))
                .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            knots += 1;
        }
        if series.exact {
            exact_cases += 1;
        }
    }
    println!(
        "{file}: {} windows ({exact_cases} exact), {knots} knots; \
         largest float deviation {:e}",
        cases.len(),
        common::worst_seen()
    );
}

/// §9.7 over every exact case of `file`: nine probes inside every interval,
/// the line against the evaluator.
///
/// The one-second interval that BRACKETS a jump is excluded, and deliberately:
/// that pair exists precisely so a discontinuity is drawn as a near-vertical
/// step instead of a ramp across the whole gap, and inside that second the line
/// is the step. Every other interval is the curve exactly — a line between two
/// numbers, and `∅` between two `∅`s. An interval with one absent end is a
/// jump into or out of `∅`, so it is always such a bracket.
fn interpolate(file: &str) {
    let data = vectors(file);
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
                let real = fulfillment(&term, at, &Env::new());
                match (a.value, b.value) {
                    (Some(x), Some(y)) => {
                        let drawn = x + (y - x) * frac;
                        let real = real.unwrap_or_else(|| {
                            panic!("seed {seed} at {at}: ∅ between two numbers")
                        });
                        worst = worst.max((drawn - real).abs());
                        assert!(
                            (drawn - real).abs() <= 1e-9,
                            "seed {seed} at {at}: the line reads {drawn}, the evaluator {real}"
                        );
                    }
                    (None, None) => assert_eq!(real, None, "seed {seed} at {at}: between ∅s"),
                    ends => panic!("seed {seed}: {ends:?} over more than a jump's second"),
                }
                probes += 1;
            }
        }
    }
    println!(
        "{file}, exact fragments: {probes} interpolation probes over the ramps \
         ({brackets} jump brackets skipped), worst gap {worst:e}"
    );
    assert!(probes > 0, "{file}: some exact interval was probed");
}

#[test]
fn every_series_has_the_reference_knots() {
    replay("series.py", 60);
}

/// §9.7: on an EXACT fragment, interpolation between two adjacent knots equals
/// evaluation. Checked at nine points inside every interval of every exact
/// vector — this is the claim the graph rests on, and the reason `exact` is
/// reported rather than assumed.
#[test]
fn interpolation_between_exact_knots_equals_evaluation() {
    interpolate("series.py");
}
