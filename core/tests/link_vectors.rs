//! SPEC §7.2 over `conformance/link.py`: a few specs, the terms that name
//! them, the EXACT print each links to, its readings at a few instants to
//! 1e-9, and the refusals — an unknown todo and a loop. Written by hand from
//! the laws in `fpl_laws.rs`, and frozen like every other vector.

mod common;

use std::collections::BTreeMap;

use common::vectors::{call, each, field, maybe_text, moment, number, strings, text_at, vectors};
use prodrome::fpl::{
    fulfillment, instant_of, link, parse_term, print_term, Env, LinkError, Outcome, Term,
};

/// A print, parsed, and checked to print back byte for byte (§9.1).
fn term_of(print: &str) -> Term {
    let term = parse_term(print).unwrap_or_else(|e| panic!("cannot parse {print}: {e}"));
    assert_eq!(print_term(&term), print, "print ∘ parse");
    term
}

#[test]
fn every_reference_links_to_its_print_and_reads_its_samples() {
    let data = vectors("link.py");
    let specs: BTreeMap<String, Term> = each(&data, "specs")
        .iter()
        .map(|spec| {
            (
                text_at(spec, "todo").to_owned(),
                term_of(text_at(spec, "term")),
            )
        })
        .collect();
    let env: Env = each(&data, "env")
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
        .collect();

    let cases = each(&data, "cases");
    assert_eq!(cases.len(), 9, "the vector file lost cases");
    let (mut linked, mut refused, mut samples) = (0usize, 0usize, 0usize);
    for case in cases {
        let print = text_at(case, "term");
        let result = link(&term_of(print), &specs);
        match field(case, "refused").as_call() {
            None => {
                let closed = result.unwrap_or_else(|e| panic!("{print}: {e}"));
                let expected = maybe_text(field(case, "linked")).expect("a linked print");
                assert_eq!(print_term(closed.term()), expected, "{print}: linked");
                for sample in each(case, "samples") {
                    let now = instant_of(moment(field(sample, "now")));
                    common::close(
                        "fulfillment",
                        fulfillment(&closed, now, &env),
                        number(field(sample, "value")),
                    )
                    .unwrap_or_else(|e| panic!("{print} at {now}: {e}"));
                    samples += 1;
                }
                linked += 1;
            }
            Some(_) => {
                let refusal = field(case, "refused");
                call(refusal, "Refused");
                let todos = strings(refusal, "todos");
                let expected = match text_at(refusal, "kind") {
                    "unknown" => LinkError::Unknown(todos[0].clone()),
                    "cycle" => LinkError::Cycle(todos),
                    other => panic!("unknown refusal kind {other:?}"),
                };
                assert_eq!(result, Err(expected), "{print}: refused");
                assert_eq!(maybe_text(field(case, "linked")), None, "{print}");
                assert!(
                    each(case, "samples").is_empty(),
                    "{print}: a refusal has no value"
                );
                refused += 1;
            }
        }
    }
    assert_eq!((linked, refused), (7, 2));
    println!("link.py: {linked} linked, {refused} refused, {samples} samples");
}
