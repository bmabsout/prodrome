//! SPEC §9.8 over `conformance/fpl.py`: 150 random terms, each as a literal
//! print, six `fulfillment` samples, the `normalized` print, and `explain` at
//! one moment. Prints are compared EXACTLY; floats to 1e-9.
//!
//! WHAT `explain` IS HERE. The vector holds the DECORATION — a kind tag, a
//! value and the notes at each node — and not the term's shape, which is the
//! case's own `term` print. The JSON spelling a browser reads is
//! `prodrome-wasm`'s (0.4: the codec went to the boundary JSON is for), and
//! `wasm/conformance/term-json.json` freezes that, unchanged, for the same 150.

mod common;

use common::vectors::{each, field, integer, moment, number, text_at, vectors};
use prodrome::fpl::{
    explained, fulfillment, instant_of, normalize, parse_term, print_term, Env, Outcome,
};
use prodrome::literal::Value;

fn env_of(bounds: &[Value]) -> Env {
    bounds
        .iter()
        .map(|bound| {
            let at = instant_of(moment(field(bound, "at")));
            let outcome = match text_at(bound, "kind") {
                "Completed" => Outcome::Completed(at),
                "Cancelled" => Outcome::Cancelled(at),
                other => panic!("unknown env kind {other:?}"),
            };
            (text_at(bound, "todo").to_owned(), outcome)
        })
        .collect()
}

#[test]
fn every_term_prints_evaluates_normalises_and_explains_as_the_reference() {
    let data = vectors("fpl.py");
    let terms = each(&data, "terms");
    assert_eq!(terms.len(), 150, "the vector file lost cases");
    let (mut samples, mut explains) = (0usize, 0usize);
    for case in terms {
        let seed = integer(field(case, "seed"));
        let text = text_at(case, "term");
        let term = parse_term(text).unwrap_or_else(|e| panic!("seed {seed}: cannot parse: {e}"));

        // §2: the print is the identity — parse then print is byte-identical.
        assert_eq!(print_term(&term), text, "seed {seed}: print ∘ parse");

        let env = env_of(each(case, "env"));
        for sample in each(case, "samples") {
            let now = instant_of(moment(field(sample, "now")));
            common::close(
                "fulfillment",
                fulfillment(&term, now, &env),
                number(field(sample, "value")),
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
            &common::explanation_value(&explained(&term, at, &env)),
            field(case, "explain"),
        )
        .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        explains += 1;
    }
    println!(
        "fpl.py: {} terms, {samples} fulfillment samples, {explains} explain trees; \
         largest float deviation {:e}",
        terms.len(),
        common::worst_seen()
    );
}
