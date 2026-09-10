//! SPEC §9.5, on random terms: `mk_piecewise` is a normal form (unit, join,
//! no adjacent repeats, strictly increasing, idempotent) and `normalize`
//! preserves every reading, buries no schedule, and is idempotent.
//!
//! And §9.13 over the same generators: COMPILING preserves every reading too,
//! with the compiled side read against the EMPTY environment (§7.1).

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};
use prodrome::chain::compile;
use prodrome::fpl::{
    self, fulfillment, mk_piecewise, normalize, Env, Instant, Outcome, Term, TermF,
};
use proptest::prelude::*;

const EVENTS: [&str; 3] = ["alpha", "beta", "gamma"];

fn origin() -> Instant {
    NaiveDate::from_ymd_opt(2026, 9, 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .expect("a real date")
}

/// Instants live on a coarse grid so that collisions, ties and adjacent
/// duplicates actually occur — the cases the normal form is about.
fn moment(hours: i64) -> Instant {
    origin() + Duration::hours(hours)
}

fn ok(t: Result<Term, fpl::FplError>) -> Term {
    t.expect("the generator only builds terms the constructors admit")
}

fn hours() -> impl Strategy<Value = i64> {
    -400i64..400
}

/// Distinct, sorted grid instants — what a schedule or a curve needs.
fn instants(n: usize) -> impl Strategy<Value = Vec<i64>> {
    prop::collection::btree_set(hours(), 1..=n).prop_map(|s| s.into_iter().collect())
}

fn a_term() -> impl Strategy<Value = Term> {
    let leaf = prop_oneof![
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
                    .map(|(at, v)| fpl::CurvePoint {
                        at: moment(*at),
                        value: *v,
                        label: String::new(),
                    })
                    .collect(),
            ))
        }),
    ];
    leaf.prop_recursive(4, 48, 3, |inner| {
        prop_oneof![
            (
                prop::collection::vec(inner.clone(), 1..3),
                prop::sample::select(vec![-8.0, -4.0, -1.0, 0.0])
            )
                .prop_map(|(ts, p)| ok(fpl::mk_conj(ts, p))),
            (-0.9f64..0.9, inner.clone()).prop_map(|(d, t)| ok(fpl::mk_offset(d, t))),
            (inner.clone(), inner.clone()).prop_map(|(g, b)| ok(fpl::mk_gate(g, b))),
            (inner.clone(), inner.clone()).prop_map(|(d, t)| ok(fpl::mk_offset_by(d, t))),
            (hours(), inner.clone()).prop_map(|(h, t)| ok(fpl::mk_shift(Duration::hours(h), t))),
            (
                1i64..72,
                prop::sample::select(vec![-4.0, -1.0, 0.0]),
                inner.clone()
            )
                .prop_map(|(w, p, t)| ok(fpl::mk_within(Duration::hours(w), p, t))),
            (0.3f64..2.5, inner.clone()).prop_map(|(w, t)| ok(fpl::mk_importance(w, t))),
            (
                prop::sample::select(EVENTS.to_vec()),
                hours(),
                inner.clone(),
                inner.clone()
            )
                .prop_map(|(e, a, t, p)| ok(fpl::mk_after(
                    e.to_string(),
                    moment(a),
                    t,
                    p,
                    None
                ))),
            (inner.clone(), instants(3), prop::collection::vec(inner, 3)).prop_map(
                |(head, ats, ts)| ok(mk_piecewise(
                    head,
                    ats.iter().zip(ts).map(|(at, t)| (moment(*at), t)).collect()
                ))
            ),
        ]
    })
}

fn an_env() -> impl Strategy<Value = Env> {
    prop::collection::vec(prop::option::of((any::<bool>(), hours())), 3).prop_map(|choices| {
        let mut env: Env = BTreeMap::new();
        for (name, choice) in EVENTS.iter().zip(choices) {
            if let Some((completed, at)) = choice {
                let at = moment(at);
                env.insert(
                    (*name).to_string(),
                    if completed {
                        Outcome::Completed(at)
                    } else {
                        Outcome::Cancelled(at)
                    },
                );
            }
        }
        env
    })
}

/// Every node of a term, the root included.
fn nodes(term: &Term, out: &mut Vec<Term>) {
    out.push(term.clone());
    for child in term.out().children() {
        nodes(child, out);
    }
}

fn is_piecewise(t: &Term) -> bool {
    matches!(t.out(), TermF::Piecewise { .. })
}

/// A schedule at this node is well formed: strictly increasing instants, no
/// adjacent repeats, and nothing nested under it.
fn schedule_is_normal(term: &Term) -> Result<(), String> {
    let TermF::Piecewise { head, pieces } = term.out() else {
        return Ok(());
    };
    if pieces.is_empty() {
        return Err("unit: a Piecewise with no pieces should be its head".into());
    }
    if is_piecewise(head) {
        return Err("join: a Piecewise head is still a Piecewise".into());
    }
    let mut previous = head;
    for (i, (at, t)) in pieces.iter().enumerate() {
        if is_piecewise(t) {
            return Err(format!("join: piece {i} is still a Piecewise"));
        }
        if t == previous {
            return Err(format!("piece {i} repeats the function before it"));
        }
        if i > 0 && pieces[i - 1].0 >= *at {
            return Err(format!("piece {i} does not follow the one before it"));
        }
        previous = t;
    }
    Ok(())
}

/// Join, on a case chosen rather than sampled: a piece whose term is itself a
/// schedule is SPLICED — the inner function in force at the outer instant takes
/// over there, the inner transitions after it become transitions of the whole,
/// and no Piecewise survives inside another.
#[test]
fn a_nested_schedule_is_spliced_flat() {
    let inner = mk_piecewise(
        ok(fpl::mk_flat(0.1)),
        vec![
            (moment(10), ok(fpl::mk_flat(0.2))),
            (moment(30), ok(fpl::mk_flat(0.3))),
        ],
    )
    .expect("ordered");
    let outer = mk_piecewise(ok(fpl::mk_flat(0.9)), vec![(moment(20), inner)]).expect("ordered");
    assert_eq!(
        fpl::print_term(&outer),
        "Piecewise(head=Flat(value=0.9), pieces=(Piece(at=datetime(2026, 9, 1, 20, 0, 0), \
         term=Flat(value=0.2)), Piece(at=datetime(2026, 9, 2, 6, 0, 0), term=Flat(value=0.3))))"
    );
    assert_eq!(schedule_is_normal(&outer), Ok(()));
}

/// A pointwise operator over two schedules normalises to ONE schedule at the
/// root, over the merged partition, with the operator inside each piece.
#[test]
fn a_conjunction_of_schedules_lifts_to_the_root() {
    let left = mk_piecewise(
        ok(fpl::mk_flat(0.4)),
        vec![(moment(10), ok(fpl::mk_flat(0.5)))],
    )
    .expect("ordered");
    let right = mk_piecewise(
        ok(fpl::mk_flat(0.6)),
        vec![(moment(20), ok(fpl::mk_flat(0.7)))],
    )
    .expect("ordered");
    let normal = normalize(&ok(fpl::mk_conj(vec![left, right], -4.0)));
    let TermF::Piecewise { pieces, .. } = normal.out() else {
        panic!(
            "the schedule should be at the root, got {}",
            normal.out().kind()
        );
    };
    assert_eq!(
        pieces.len(),
        2,
        "one piece per instant in the merged partition"
    );
    assert_eq!(schedule_is_normal(&normal), Ok(()));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// `mk_piecewise` IS the normal form: it emits nothing but well-formed
    /// schedules, at every depth, on any term the constructors admit.
    #[test]
    fn mk_piecewise_is_a_normal_form(term in a_term()) {
        let mut all = vec![];
        nodes(&term, &mut all);
        for node in &all {
            prop_assert!(schedule_is_normal(node).is_ok(), "{:?}", schedule_is_normal(node));
        }
    }

    /// Unit: a schedule with no transitions IS its head, and is returned as such.
    #[test]
    fn mk_piecewise_unit(term in a_term()) {
        prop_assert_eq!(mk_piecewise(term.clone(), vec![]).expect("no instants to order"), term);
    }

    /// Idempotent: re-splitting a normal form at its own instants changes nothing.
    #[test]
    fn mk_piecewise_is_idempotent(term in a_term()) {
        if let TermF::Piecewise { head, pieces } = term.out() {
            let again = mk_piecewise(head.clone(), pieces.clone()).expect("already ordered");
            prop_assert_eq!(again, term.clone());
        }
    }

    /// §9.5: `normalize` preserves every reading — it moves the schedule to the
    /// root, it does not re-price anything.
    #[test]
    fn normalize_preserves_every_reading(
        term in a_term(),
        env in an_env(),
        probes in prop::collection::vec(-500i64..500, 8),
    ) {
        let normal = normalize(&term);
        for h in probes {
            let now = moment(h);
            let (before, after) = (fulfillment(&term, now, &env), fulfillment(&normal, now, &env));
            prop_assert!(
                (before - after).abs() <= 1e-12,
                "at {now}: {before} became {after}",
            );
        }
    }

    /// Idempotent, and structurally so — not merely equal at every instant.
    #[test]
    fn normalize_is_idempotent(term in a_term()) {
        let once = normalize(&term);
        prop_assert_eq!(normalize(&once), once);
    }

    /// Nothing buried: after `normalize` no schedule hides under an operator
    /// that reads its subterms pointwise, nor under a Shift, which translates
    /// the instants it crosses. Within and After are the stated exceptions —
    /// neither commutes with a partition.
    #[test]
    fn normalize_buries_no_schedule(term in a_term()) {
        let normal = normalize(&term);
        let mut all = vec![];
        nodes(&normal, &mut all);
        for node in &all {
            let buried = match node.out() {
                TermF::Conj { terms, .. } => terms.iter().any(is_piecewise),
                TermF::Offset { term, .. }
                | TermF::Importance { term, .. }
                | TermF::Shift { term, .. } => is_piecewise(term),
                TermF::Gate { gate, body } => is_piecewise(gate) || is_piecewise(body),
                TermF::OffsetBy { delta, term } => is_piecewise(delta) || is_piecewise(term),
                _ => false,
            };
            prop_assert!(!buried, "a schedule stayed under {}", node.out().kind());
            prop_assert!(schedule_is_normal(node).is_ok(), "{:?}", schedule_is_normal(node));
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// §9.13: COMPILATION PRESERVES EVERY READING. The compiled term, read
    /// against the EMPTY environment, answers what the interpreted one
    /// answered against the real one — at every instant, on every term the
    /// constructors admit and every snapshot §9.5 is checked over. This is the
    /// whole justification for §7.1: an optimisation with an equivalence law,
    /// and not a second semantics.
    #[test]
    fn compilation_preserves_every_reading(
        term in a_term(),
        env in an_env(),
        probes in prop::collection::vec(-500i64..500, 8),
    ) {
        let compiled = compile(&term, &env);
        for h in probes {
            let now = moment(h);
            let (interpreted, value) = (fulfillment(&term, now, &env), compiled.fulfillment(now));
            prop_assert!(
                (interpreted - value).abs() <= 1e-9,
                "at {now}: {interpreted} became {value}",
            );
        }
    }

    /// The law's TEETH: nothing survives compilation that could read history.
    /// `After` is the only constructor whose evaluation consults the
    /// environment — every other one is a function of `now` and its subterms —
    /// so no `After` anywhere is the proof that no lookup happens, and the
    /// environment that answers differently about everything is the same
    /// proof taken behaviourally.
    #[test]
    fn compilation_leaves_nothing_that_reads_history(
        term in a_term(),
        env in an_env(),
        probes in prop::collection::vec(-500i64..500, 4),
    ) {
        let compiled = compile(&term, &env);
        let mut all = vec![];
        nodes(compiled.term(), &mut all);
        for node in &all {
            prop_assert!(
                !matches!(node.out(), TermF::After { .. }),
                "an After survived compilation",
            );
            prop_assert!(schedule_is_normal(node).is_ok(), "{:?}", schedule_is_normal(node));
        }
        let poisoned: Env = EVENTS
            .iter()
            .map(|e| ((*e).to_string(), Outcome::Cancelled(moment(-10_000))))
            .collect();
        for h in probes {
            let now = moment(h);
            prop_assert_eq!(
                compiled.fulfillment(now),
                fulfillment(compiled.term(), now, &poisoned),
                "the compiled term consulted the environment",
            );
        }
    }

    /// Compiling under the environment that binds NOTHING is the identity on
    /// readings AND drops every unbound link's frozen branch: a term with no
    /// binding in force is exactly its pending branches, which is the
    /// optimisation at its plainest.
    #[test]
    fn compiling_against_nothing_is_the_pending_reading(
        term in a_term(),
        probes in prop::collection::vec(-500i64..500, 4),
    ) {
        let compiled = compile(&term, &Env::new());
        for h in probes {
            let now = moment(h);
            let (interpreted, value) =
                (fulfillment(&term, now, &Env::new()), compiled.fulfillment(now));
            prop_assert!((interpreted - value).abs() <= 1e-9, "at {now}: {interpreted} vs {value}");
        }
    }
}
