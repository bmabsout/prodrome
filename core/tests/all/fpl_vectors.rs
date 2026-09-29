//! SPEC §9.8 over `conformance/fpl.py`: 150 random terms, each as a literal
//! print, six `fulfillment` samples, the `normalized` print, and `explain` at
//! one moment. Prints are compared EXACTLY; floats to 1e-9. And §9.18 over
//! `conformance/absent/fpl.py`, the same shape for 40 seeded terms that hold
//! an `Absent`, where a reading or a node may be `∅` (`None`).
//!
//! WHAT `explain` IS HERE. The vector holds the DECORATION — a kind tag, a
//! value and the notes at each node — and not the term's shape, which is the
//! case's own `term` print. The JSON spelling a browser reads is
//! `prodrome-wasm`'s (0.4: the codec went to the boundary JSON is for), and
//! `wasm/conformance/term-json.json` freezes that, unchanged, for the same 150.

use crate::common;

use common::vectors::{each, field, integer, moment, text_at, vectors};
use prodrome::fpl::{
    explained, fulfillment, instant_of, parse_term, print_term, Closed, Env, Outcome,
};
use prodrome::literal::Value;
use prodrome::term::normalize;

fn env_of(bounds: &[Value]) -> Env {
    Env {
        outcomes: bounds
            .iter()
            .map(|bound| {
                let at = instant_of(moment(field(bound, "at")));
                let outcome = match text_at(bound, "kind") {
                    "Completed" => Outcome::Completed(at),
                    "Cancelled" => Outcome::Cancelled(at),
                    other => panic!("unknown env kind {other:?}"),
                };
                (text_at(bound, "todo").to_owned(), [Some(outcome)].into())
            })
            .collect(),
        ..Env::new()
    }
}

/// Every case of `file`: the print, the readings, the normal form and the
/// explanation.
fn replay(file: &str, expected: usize) {
    let data = vectors(file);
    let terms = each(&data, "terms");
    assert_eq!(terms.len(), expected, "{file} lost cases");
    let (mut samples, mut explains) = (0usize, 0usize);
    for case in terms {
        let seed = integer(field(case, "seed"));
        let text = text_at(case, "term");
        let term = parse_term(text).unwrap_or_else(|e| panic!("seed {seed}: cannot parse: {e}"));

        // §2: the print is the identity — parse then print is byte-identical.
        assert_eq!(print_term(&term), text, "seed {seed}: print ∘ parse");

        let closed = Closed::of(term.clone()).expect("the vectors hold no Ref");
        let env = env_of(each(case, "env"));
        for sample in each(case, "samples") {
            let now = instant_of(moment(field(sample, "now")));
            common::reading(
                "fulfillment",
                fulfillment(&closed, now, &env),
                field(sample, "value"),
            )
            .unwrap_or_else(|e| panic!("seed {seed} at {now}: {e}"));
            samples += 1;
        }

        // §7 normal form: an EXACT print, not a reading.
        assert_eq!(
            print_term(&normalize(&term)),
            text_at(case, "normalized"),
            "seed {seed}: normalize"
        );

        let at = instant_of(moment(field(case, "explain_at")));
        common::agrees(
            "explain",
            &common::explanation_value(&explained(&closed, at, &env)),
            field(case, "explain"),
        )
        .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        explains += 1;
    }
    println!(
        "{file}: {} terms, {samples} fulfillment samples, {explains} explain trees; \
         largest float deviation {:e}",
        terms.len(),
        common::worst_seen()
    );
}

#[test]
fn every_term_prints_evaluates_normalises_and_explains_as_the_reference() {
    replay("fpl.py", 150);
}

#[test]
fn every_absent_term_prints_evaluates_normalises_and_explains_as_frozen() {
    replay("absent/fpl.py", 40);
}
