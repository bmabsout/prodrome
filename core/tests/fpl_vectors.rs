//! SPEC §9.8 over `conformance/fpl.json`: 150 random terms, each as a literal
//! print, six `fulfillment` samples and the `normalized` print. Prints are
//! compared EXACTLY; floats to 1e-9.
//!
//! The `json` and `explain` halves of these vectors went to `prodrome-wasm`
//! with the JSON codec (0.4): JSON is the browser boundary's and its evidence
//! belongs where it is produced. `wasm/conformance/term-json.json` holds them,
//! unchanged, and `wasm/src/json.rs`'s test replays the same 150.

mod common;

use std::collections::BTreeMap;

use prodrome::fpl::{self, fulfillment, normalize, parse_term, print_term, Env, Outcome, Term};
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
fn every_term_prints_evaluates_and_normalises_as_the_reference() {
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

        // §7 normal form: an EXACT print, not a reading.
        assert_eq!(
            print_term(&normalize(&term)),
            case["normalized"].as_str().expect("normalized is a string"),
            "seed {seed}: normalize"
        );
    }
    println!(
        "fpl.json: {} terms, {samples} fulfillment samples; \
         largest float deviation {:e}",
        terms.len(),
        common::worst_seen()
    );
}
