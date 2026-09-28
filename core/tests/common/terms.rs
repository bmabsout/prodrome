//! THE TERM GENERATORS: random closed and open terms, specs that link, and
//! environments — what `tests/fpl_laws.rs`'s properties draw from and
//! `examples/generate_absent_vectors.rs` draws `conformance/absent/*.py`
//! from, over a fixed seed. Moved here out of `fpl_laws.rs` for the reason
//! `a_log` was moved beside it: one generator, read by a property and frozen
//! by a vector file, never two.
//!
//! Instants live on an HOURLY grid from `origin()`, so that collisions, ties
//! and adjacent duplicates actually occur.
#![allow(dead_code)]

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};
use prodrome::fpl::{self, mk_piecewise, Env, Instant, Outcome};
use prodrome::term::{CurvePoint, Term};
use proptest::prelude::*;

pub const EVENTS: [&str; 3] = ["alpha", "beta", "gamma"];
/// The todos a `Ref` may name. Two of them are `EVENTS` too, so a linked
/// term's `After` and its references can name the same todo.
pub const TODOS: [&str; 4] = ["alpha", "beta", "delta", "epsilon"];

pub fn origin() -> Instant {
    NaiveDate::from_ymd_opt(2026, 9, 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .expect("a real date")
}

/// Instants live on a coarse grid so that collisions, ties and adjacent
/// duplicates actually occur — the cases the normal form is about.
pub fn moment(hours: i64) -> Instant {
    origin() + Duration::hours(hours)
}

pub fn ok(t: Result<Term, fpl::FplError>) -> Term {
    t.expect("the generator only builds terms the constructors admit")
}

pub fn hours() -> impl Strategy<Value = i64> {
    -400i64..400
}

/// Distinct, sorted grid instants — what a schedule or a curve needs.
pub fn instants(n: usize) -> impl Strategy<Value = Vec<i64>> {
    prop::collection::btree_set(hours(), 1..=n).prop_map(|s| s.into_iter().collect())
}

/// A term with no `Ref`: what evaluation takes.
pub fn a_term() -> impl Strategy<Value = Term> {
    grown(a_closed_leaf())
}

/// A term whose leaves may be `Ref`s onto `TODOS`: what `link` takes.
pub fn an_open_term() -> impl Strategy<Value = Term> {
    grown(prop_oneof![3 => a_closed_leaf(), 1 => a_ref()].boxed())
}

pub fn a_ref() -> BoxedStrategy<Term> {
    refs_onto(TODOS.to_vec())
}

pub fn refs_onto(todos: Vec<&'static str>) -> BoxedStrategy<Term> {
    prop::sample::select(todos)
        .prop_map(|todo| ok(fpl::mk_ref(todo.to_owned())))
        .boxed()
}

/// A small term with no `Within`, whose references name only `todos`.
pub fn a_spec_onto(todos: Vec<&'static str>) -> BoxedStrategy<Term> {
    let leaf = if todos.is_empty() {
        a_closed_leaf()
    } else {
        prop_oneof![2 => a_closed_leaf(), 1 => refs_onto(todos)].boxed()
    };
    grown_to(leaf, 3, 12, false)
}

/// One spec per todo in `TODOS`, each referring only to the todos after it,
/// so the references draw a DAG and every one links.
pub fn acyclic_specs() -> impl Strategy<Value = BTreeMap<String, Term>> {
    (0..TODOS.len())
        .map(|i| a_spec_onto(TODOS[i + 1..].to_vec()))
        .collect::<Vec<_>>()
        .prop_map(|terms| {
            TODOS
                .iter()
                .map(|todo| (*todo).to_owned())
                .zip(terms)
                .collect()
        })
}

/// A leaf with a value at every instant: `Flat`, `Decay`, `Curve`.
pub fn a_valued_leaf() -> BoxedStrategy<Term> {
    prop_oneof![
        (0.02f64..0.98).prop_map(|v| ok(fpl::mk_flat(v))),
        (
            0.3f64..0.95,
            0.0f64..0.2,
            hours(),
            6i64..400,
            prop::option::of(hours())
        )
            .prop_map(|(start, end, at, lead, from)| ok(fpl::mk_decay(
                start,
                end,
                moment(at),
                Duration::hours(lead),
                from.map(moment),
            ))),
        (instants(4), prop::collection::vec(0.0f64..1.0, 4)).prop_map(|(ats, vs)| {
            ok(fpl::mk_curve(
                ats.iter()
                    .zip(&vs)
                    .map(|(at, v)| CurvePoint {
                        at: moment(*at),
                        value: *v,
                        label: String::new(),
                    })
                    .collect(),
            ))
        }),
    ]
    .boxed()
}

/// Any closed leaf: one with a value, or now and then `Absent`, so every law
/// drawn over these generators quantifies over `∅` too.
pub fn a_closed_leaf() -> BoxedStrategy<Term> {
    prop_oneof![6 => a_valued_leaf(), 1 => Just(fpl::mk_absent())].boxed()
}

/// A closed term holding no `Absent`: one that has a value at every instant.
pub fn a_valued_term() -> impl Strategy<Value = Term> {
    grown(a_valued_leaf())
}

/// A term of the EXACT fragment (§7 breakpoints) — `Flat`, `Decay`, `Curve`,
/// `Absent`, and `Piecewise`, `Offset`, `Shift` and `Least` over them — whose
/// series knots are the curve.
pub fn an_exact_term() -> impl Strategy<Value = Term> {
    a_closed_leaf().prop_recursive(3, 24, 3, |inner| {
        prop_oneof![
            (
                inner.clone(),
                instants(3),
                prop::collection::vec(inner.clone(), 3),
            )
                .prop_map(|(head, ats, ts)| {
                    ok(fpl::mk_piecewise(
                        head,
                        ats.iter().zip(ts).map(|(at, t)| (moment(*at), t)).collect(),
                    ))
                }),
            (-0.9f64..0.9, inner.clone()).prop_map(|(d, t)| ok(fpl::mk_offset(d, t))),
            (hours(), inner.clone()).prop_map(|(h, t)| ok(fpl::mk_shift(Duration::hours(h), t))),
            prop::collection::vec(inner, 1..4).prop_map(|ts| ok(fpl::mk_least(ts))),
        ]
    })
}

/// Every constructor over `leaf`, four levels deep.
pub fn grown(leaf: BoxedStrategy<Term>) -> BoxedStrategy<Term> {
    grown_to(leaf, 4, 48, true)
}

/// Every constructor over `leaf`, `Within` only where `windows`: a linked term
/// nests its specs, and windows nested through several references multiply
/// their 65 samples into a case that never finishes.
pub fn grown_to(
    leaf: BoxedStrategy<Term>,
    depth: u32,
    size: u32,
    windows: bool,
) -> BoxedStrategy<Term> {
    leaf.prop_recursive(depth, size, 3, move |inner| {
        let mut arms: Vec<BoxedStrategy<Term>> = vec![
            (
                prop::collection::vec(inner.clone(), 1..3),
                prop::sample::select(vec![-8.0, -4.0, -1.0, 0.0]),
            )
                .prop_map(|(ts, p)| ok(fpl::mk_conj(ts, p)))
                .boxed(),
            prop::collection::vec(inner.clone(), 1..3)
                .prop_map(|ts| ok(fpl::mk_least(ts)))
                .boxed(),
            (-0.9f64..0.9, inner.clone())
                .prop_map(|(d, t)| ok(fpl::mk_offset(d, t)))
                .boxed(),
            (inner.clone(), inner.clone())
                .prop_map(|(g, b)| ok(fpl::mk_gate(g, b)))
                .boxed(),
            (inner.clone(), inner.clone())
                .prop_map(|(d, t)| ok(fpl::mk_offset_by(d, t)))
                .boxed(),
            (hours(), inner.clone())
                .prop_map(|(h, t)| ok(fpl::mk_shift(Duration::hours(h), t)))
                .boxed(),
            (0.3f64..2.5, inner.clone())
                .prop_map(|(w, t)| ok(fpl::mk_importance(w, t)))
                .boxed(),
            (
                prop::sample::select(EVENTS.to_vec()),
                hours(),
                inner.clone(),
                inner.clone(),
            )
                .prop_map(|(e, a, t, p)| ok(fpl::mk_after(e.to_string(), moment(a), t, p, None)))
                .boxed(),
            (
                prop::sample::select(EVENTS.to_vec()),
                hours(),
                inner.clone(),
                inner.clone(),
            )
                .prop_map(|(e, a, t, p)| ok(fpl::mk_recur(e.to_string(), moment(a), t, p)))
                .boxed(),
            (1i64..400, hours(), inner.clone())
                .prop_map(|(p, a, t)| ok(fpl::mk_periodic(Duration::hours(p), moment(a), t)))
                .boxed(),
            (
                inner.clone(),
                instants(3),
                prop::collection::vec(inner.clone(), 3),
            )
                .prop_map(|(head, ats, ts)| {
                    ok(mk_piecewise(
                        head,
                        ats.iter().zip(ts).map(|(at, t)| (moment(*at), t)).collect(),
                    ))
                })
                .boxed(),
        ];
        if windows {
            arms.push(
                (1i64..72, prop::sample::select(vec![-4.0, -1.0, 0.0]), inner)
                    .prop_map(|(w, p, t)| ok(fpl::mk_within(Duration::hours(w), p, t)))
                    .boxed(),
            );
        }
        prop::strategy::Union::new(arms)
    })
    .boxed()
}

/// Each of `EVENTS` bound or not, and tended at a few grid instants or not.
pub fn an_env() -> impl Strategy<Value = Env> {
    (
        prop::collection::vec(prop::option::of((any::<bool>(), hours())), 3),
        prop::collection::vec(prop::collection::btree_set(hours(), 0..4), 3),
    )
        .prop_map(|(choices, tended)| {
            let mut env = Env::new();
            for ((name, choice), tendings) in EVENTS.iter().zip(choices).zip(tended) {
                if let Some((completed, at)) = choice {
                    let at = moment(at);
                    env.outcomes.insert(
                        (*name).to_string(),
                        if completed {
                            Outcome::Completed(at)
                        } else {
                            Outcome::Cancelled(at)
                        },
                    );
                }
                if !tendings.is_empty() {
                    env.tended.insert(
                        (*name).to_string(),
                        tendings.into_iter().map(moment).collect(),
                    );
                }
            }
            env
        })
}
