//! SPEC §9.8 over `conformance/fpl.json`: 150 random terms, each as a literal
//! print AND as `to_json`, and six `fulfillment` samples. Prints are compared
//! EXACTLY; floats to 1e-9.

mod common;

use std::collections::BTreeMap;

use prodrome::fpl::{self, fulfillment, parse_term, print_term, to_json, Env, Outcome, Term};
use serde_json::Value;

fn env_of(v: &Value) -> Env {
    let mut env: Env = BTreeMap::new();
    for (name, binding) in v.as_object().expect("env is an object") {
        let at = parse_iso(binding["at"].as_str().expect("env.at is a string"));
        env.insert(
            name.clone(),
            match binding["kind"].as_str().expect("env.kind is a string") {
                "Completed" => Outcome::Completed(at),
                "Cancelled" => Outcome::Cancelled(at),
                other => panic!("unknown env kind {other:?}"),
            },
        );
    }
    env
}

fn parse_iso(s: &str) -> fpl::Instant {
    fpl::parse_iso(s).unwrap_or_else(|e| panic!("bad instant {s:?}: {e}"))
}

fn parsed(text: &str) -> Term {
    parse_term(text).unwrap_or_else(|e| panic!("cannot parse {text}: {e}"))
}

#[test]
fn every_term_prints_round_trips_and_evaluates_as_the_reference() {
    let data = common::vectors("fpl.json");
    let terms = data["terms"]
        .as_array()
        .expect("fpl.json has a terms array");
    assert_eq!(terms.len(), 150, "the vector file lost cases");
    let mut samples = 0usize;
    for case in terms {
        let seed = &case["seed"];
        let text = case["term"].as_str().expect("term is a literal string");
        let term = parsed(text);

        // §2: the print is the identity — parse then print is byte-identical.
        assert_eq!(print_term(&term), text, "seed {seed}: print ∘ parse");

        // §7 JSON: the kind tags and the shape.
        common::agrees("json", &to_json(&term), &case["json"])
            .unwrap_or_else(|e| panic!("seed {seed}: to_json {e}"));

        // from_json is the other door into the same term.
        let from_wire =
            fpl::from_json(&case["json"]).unwrap_or_else(|e| panic!("seed {seed}: from_json {e}"));
        assert_eq!(
            print_term(&from_wire),
            text,
            "seed {seed}: from_json ∘ to_json"
        );

        let env = env_of(&case["env"]);
        for sample in case["samples"].as_array().expect("samples is an array") {
            let now = parse_iso(sample["now"].as_str().expect("now is a string"));
            common::agrees(
                "fulfillment",
                &Value::from(fulfillment(&term, now, &env)),
                &sample["value"],
            )
            .unwrap_or_else(|e| panic!("seed {seed} at {now}: {e}"));
            samples += 1;
        }
    }
    println!(
        "fpl.json: {} terms, {samples} fulfillment samples; \
         largest float deviation {:e}",
        terms.len(),
        common::worst_seen()
    );
}
