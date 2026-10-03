//! SPEC law 41, the design's law 12: a set of prodromes is a prodrome.
//!
//! Here, the qualified reference (§7.2): `RefIn(store, entity)` reads the
//! entity's function in the member of the set a host passes that `store`
//! names, as that member reads it; an unqualified `Ref` keeps meaning what
//! it meant; a store or an entity the set does not hold reads `∅`.

use crate::common;

use std::collections::BTreeMap;

use common::terms::{a_spec_onto, acyclic_specs, an_env, moment, ok, TODOS};
use prodrome::fpl::{self, link, link_in, Closed, Env, LinkError, Member, Stores};
use proptest::prelude::*;

/// Two prodromes' names, as their geneses would be.
const HERE: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const THERE: &str = "2222222222222222222222222222222222222222222222222222222222222222";

fn readings(linked: &Closed, env: &Env, probes: &[i64]) -> Vec<Option<f64>> {
    probes
        .iter()
        .map(|h| fpl::fulfillment(linked, moment(*h), env))
        .collect()
}

/// `RefIn`'s store obeys `TodoId`'s rule, as a genesis's name does, and its
/// entity is a path segment, in the constructor and so in the parse.
#[test]
fn a_qualified_ref_names_a_store_and_an_entity_or_is_refused() {
    let good = ok(fpl::mk_ref_in(THERE.to_owned(), "Any Key".to_owned()));
    let print = fpl::print_term(&good);
    assert_eq!(print, format!("RefIn(store='{THERE}', entity='Any Key')"));
    assert_eq!(fpl::parse_term(&print).expect("parses"), good);
    for (store, entity) in [("", "a"), ("Todos", "a"), ("todos", ""), ("todos", "a/b")] {
        assert!(fpl::mk_ref_in(store.to_owned(), entity.to_owned()).is_err());
        let print = format!("RefIn(store={store:?}, entity={entity:?})");
        assert!(fpl::parse_term(&print).is_err(), "{print} parsed");
    }
}

/// A store the set does not hold, or an entity its member does not, links
/// to `Absent` and reads `∅` at every instant, under any environment.
#[test]
fn a_qualified_ref_to_what_the_set_lacks_is_absent() {
    let member = Member {
        specs: [("a".to_owned(), ok(fpl::mk_flat(0.5)))].into(),
        env: Env::new(),
    };
    let stores = Stores::new().with(THERE, member).named("todos", THERE);
    for (store, entity) in [("todos", "b"), (HERE, "a"), ("elsewhere", "a")] {
        let reference = ok(fpl::mk_ref_in(store.to_owned(), entity.to_owned()));
        let linked = link_in(&reference, &BTreeMap::new(), &stores).expect("links");
        assert_eq!(linked.term(), &fpl::mk_absent(), "{store}/{entity}");
    }
    let present = ok(fpl::mk_ref_in("todos".to_owned(), "a".to_owned()));
    let linked = link_in(&present, &BTreeMap::new(), &stores).expect("links");
    assert_eq!(linked.term(), &ok(fpl::mk_flat(0.5)));
    // With no stores at all, `link` itself: every qualified reference is `∅`.
    assert_eq!(
        link(&present, &BTreeMap::new()).expect("links").term(),
        &fpl::mk_absent()
    );
}

/// References that loop through two prodromes are refused as one loop, an
/// entity of another prodrome named `genesis/entity`.
#[test]
fn a_loop_across_prodromes_is_a_cycle() {
    let back = ok(fpl::mk_offset(
        0.5,
        ok(fpl::mk_ref_in(HERE.to_owned(), "a".to_owned())),
    ));
    let there = Member {
        specs: [("b".to_owned(), back)].into(),
        env: Env::new(),
    };
    let forth = ok(fpl::mk_ref_in(THERE.to_owned(), "b".to_owned()));
    let here = Member {
        specs: [("a".to_owned(), forth.clone())].into(),
        env: Env::new(),
    };
    let stores = Stores::new().with(HERE, here).with(THERE, there);
    let refusal = link_in(&forth, &BTreeMap::new(), &stores).expect_err("a loop");
    assert_eq!(
        refusal,
        LinkError::Cycle(vec![
            format!("{THERE}/b"),
            format!("{HERE}/a"),
            format!("{THERE}/b"),
        ])
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// EVERY UNQUALIFIED REF KEEPS MEANING WHAT IT MEANS: a term that holds
    /// no qualified reference links alike under any set of stores.
    #[test]
    fn an_unqualified_term_links_alike_under_any_set(
        term in a_spec_onto(TODOS.to_vec()),
        specs in acyclic_specs(),
        theirs in acyclic_specs(),
        env in an_env(),
    ) {
        let stores = Stores::new()
            .with(THERE, Member { specs: theirs, env })
            .named("todos", THERE);
        prop_assert_eq!(link_in(&term, &specs, &stores), link(&term, &specs));
    }

    /// A QUALIFIED REF READS ITS ENTITY AS ITS OWN PRODROME DOES: linked
    /// against that member's functions and read under that member's
    /// environment, whatever the environment of the term it lands in. By a
    /// declared name or by the genesis, alike.
    #[test]
    fn a_qualified_ref_reads_what_its_member_reads(
        specs in acyclic_specs(),
        todo in prop::sample::select(TODOS.to_vec()),
        theirs in an_env(),
        ours in an_env(),
        probes in prop::collection::vec(-500i64..500, 6),
    ) {
        let member = Member { specs: specs.clone(), env: theirs.clone() };
        let stores = Stores::new().with(THERE, member).named("todos", THERE);
        let at_home = readings(&link(&specs[todo], &specs).expect("acyclic"), &theirs, &probes);
        for store in ["todos", THERE] {
            let reference = ok(fpl::mk_ref_in(store.to_owned(), todo.to_owned()));
            let linked = link_in(&reference, &BTreeMap::new(), &stores).expect("acyclic");
            let away = readings(&linked, &ours, &probes);
            prop_assert!(
                at_home.iter().zip(&away).all(|(a, b)| match (a, b) {
                    (Some(a), Some(b)) => (a - b).abs() <= 1e-9,
                    (a, b) => a == b,
                }),
                "{store}: {at_home:?} vs {away:?}"
            );
        }
    }
}
