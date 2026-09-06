//! SPEC §9.7/§9.8 over `conformance/series.json`: 60 random terms drawn over a
//! window. The knot INSTANTS are exact — they are the term's own transitions —
//! and the values agree to 1e-9. Beside that, the law: between two adjacent
//! knots of an exact fragment, the straight line IS the evaluator.

mod common;

use chrono::Duration;
use prodrome::breaks::series_knots;
use prodrome::fpl::{self, fulfillment, iso, parse_term, Env, Instant};
use serde_json::Value;

/// The series vectors are drawn against an empty history, as the reference's
/// generator does: `history([])` binds nothing at any instant.
fn nothing(_: Instant) -> Env {
    Env::new()
}

fn parse_iso(s: &str) -> Instant {
    fpl::parse_iso(s).unwrap_or_else(|e| panic!("bad instant {s:?}: {e}"))
}

#[test]
fn every_series_has_the_reference_knots() {
    let data = common::vectors("series.json");
    let cases = data["series"]
        .as_array()
        .expect("series.json has a series array");
    assert_eq!(cases.len(), 60, "the vector file lost cases");
    let (mut knots, mut exact_cases) = (0usize, 0usize);
    for case in cases {
        let seed = &case["seed"];
        let text = case["term"].as_str().expect("term is a literal string");
        let term = parse_term(text).unwrap_or_else(|e| panic!("seed {seed}: cannot parse: {e}"));
        let from = parse_iso(case["from"].as_str().expect("from is a string"));
        let to = parse_iso(case["to"].as_str().expect("to is a string"));

        let series = series_knots(&term, from, to, nothing);
        assert_eq!(
            series.exact,
            case["exact"].as_bool().expect("exact is a bool"),
            "seed {seed}: exactness"
        );
        let mine: Value = Value::Array(
            series
                .knots
                .iter()
                .map(|k| Value::Array(vec![Value::String(iso(k.at)), Value::from(k.value)]))
                .collect(),
        );
        common::agrees("knots", &mine, &case["knots"])
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        knots += series.knots.len();
        if series.exact {
            exact_cases += 1;
        }
    }
    println!(
        "series.json: {} windows ({exact_cases} exact), {knots} knots; \
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
    let data = common::vectors("series.json");
    let mut probes = 0usize;
    let mut brackets = 0usize;
    let mut worst = 0.0f64;
    for case in data["series"]
        .as_array()
        .expect("series.json has a series array")
    {
        if !case["exact"].as_bool().expect("exact is a bool") {
            continue;
        }
        let seed = &case["seed"];
        let term = parse_term(case["term"].as_str().expect("term is a string"))
            .unwrap_or_else(|e| panic!("seed {seed}: cannot parse: {e}"));
        let from = parse_iso(case["from"].as_str().expect("from is a string"));
        let to = parse_iso(case["to"].as_str().expect("to is a string"));
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
