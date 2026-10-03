//! SPEC §7.3 over `conformance/recur.py`: `Tended` events folded under the
//! reference policy, and `Recur` and `Periodic` read against what they fold
//! to — the exact prints, the readings at a few instants to 1e-9, and the
//! same readings from the compiled term against no environment (§9.13).
//! Written by hand from the laws, and frozen like every other vector.

use crate::common;

use common::env_at;
use common::vectors::{each, field, moment, strings, text, text_at, vectors};
use prodrome::chain::compile;
use prodrome::event::{canonical, parse_event, Actor, TodoEvent};
use prodrome::fpl::{fulfillment, instant_of, parse_term, print_term, Closed};
use prodrome::policy::Untrusted;
use prodrome::reference::Todo;

#[test]
fn every_recurrence_reads_its_samples_against_the_tendings() {
    let data = vectors("recur.py");
    let policy = Untrusted::of(
        strings(&data, "untrusted")
            .into_iter()
            .map(|name| Actor::new(name).expect("an actor")),
    );
    let events: Vec<_> = each(&data, "events")
        .iter()
        .map(|print| {
            let event = parse_event::<TodoEvent<Todo>>(&Default::default(), text(print))
                .expect("a stored event");
            assert_eq!(canonical(&event), text(print), "print ∘ parse");
            event
        })
        .collect();
    let env = env_at(&events, moment(field(&data, "at")), &policy);
    assert_eq!(
        env.tended["toothbrushvinegar"].len(),
        2,
        "the claimed pass binds nothing"
    );
    assert!(env.outcomes.is_empty(), "a tending resolves nothing");

    let cases = each(&data, "cases");
    assert_eq!(cases.len(), 3, "the vector file lost cases");
    let mut samples = 0usize;
    for case in cases {
        let print = text_at(case, "term");
        let term = parse_term(print).unwrap_or_else(|e| panic!("{print}: {e}"));
        assert_eq!(print_term(&term), print, "print ∘ parse");
        let term = Closed::of(term).expect("no Ref");
        let compiled = compile(&term, &env);
        for sample in each(case, "samples") {
            let now = instant_of(moment(field(sample, "now")));
            let expected = field(sample, "value");
            common::reading("fulfillment", fulfillment(&term, now, &env), expected)
                .unwrap_or_else(|e| panic!("{print} at {now}: {e}"));
            common::reading("compiled", compiled.fulfillment(now), expected)
                .unwrap_or_else(|e| panic!("{print} compiled, at {now}: {e}"));
            samples += 1;
        }
    }
    assert_eq!(samples, 16);
}
