//! SPEC §9.5, on random terms: `mk_piecewise` is a normal form (unit, join,
//! no adjacent repeats, strictly increasing, idempotent) and `normalize`
//! preserves every reading, buries no schedule, and is idempotent.
//!
//! And §9.13 over the same generators: COMPILING preserves every reading too,
//! with the compiled side read against the EMPTY environment (§7.1).
//!
//! And §7's `link`, as bind: a closed term is its own link, a reference reads
//! what its spec reads, linking commutes with the order of substitution, and
//! a cycle is refused with its path. Print and parse round-trip `Ref`.
//!
//! And the recurrence laws: `last_tended` reads as of now, `Recur` re-anchors
//! to the last tending, waits for the first and ignores later ones, and
//! `Periodic` repeats its first cycle.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, NaiveDate};
use prodrome::chain::{self, Compiled};
use prodrome::fpl::{
    self, link, mk_piecewise, normalize, Closed, Env, Instant, LinkError, Outcome, Term, TermF,
};
use proptest::prelude::*;

const EVENTS: [&str; 3] = ["alpha", "beta", "gamma"];
/// The todos a `Ref` may name. Two of them are `EVENTS` too, so a linked
/// term's `After` and its references can name the same todo.
const TODOS: [&str; 4] = ["alpha", "beta", "delta", "epsilon"];

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

fn closed(term: &Term) -> Closed {
    Closed::of(term.clone()).expect("a_term() builds no Ref")
}

fn fulfillment(term: &Term, now: Instant, env: &Env) -> f64 {
    fpl::fulfillment(&closed(term), now, env)
}

fn compile(term: &Term, env: &Env) -> Compiled {
    chain::compile(&closed(term), env)
}

fn hours() -> impl Strategy<Value = i64> {
    -400i64..400
}

/// Distinct, sorted grid instants — what a schedule or a curve needs.
fn instants(n: usize) -> impl Strategy<Value = Vec<i64>> {
    prop::collection::btree_set(hours(), 1..=n).prop_map(|s| s.into_iter().collect())
}

/// A term with no `Ref`: what evaluation takes.
fn a_term() -> impl Strategy<Value = Term> {
    grown(a_closed_leaf())
}

/// A term whose leaves may be `Ref`s onto `TODOS`: what `link` takes.
fn an_open_term() -> impl Strategy<Value = Term> {
    grown(prop_oneof![3 => a_closed_leaf(), 1 => a_ref()].boxed())
}

fn a_ref() -> BoxedStrategy<Term> {
    refs_onto(TODOS.to_vec())
}

fn refs_onto(todos: Vec<&'static str>) -> BoxedStrategy<Term> {
    prop::sample::select(todos)
        .prop_map(|todo| ok(fpl::mk_ref(todo.to_owned())))
        .boxed()
}

/// A small term with no `Within`, whose references name only `todos`.
fn a_spec_onto(todos: Vec<&'static str>) -> BoxedStrategy<Term> {
    let leaf = if todos.is_empty() {
        a_closed_leaf()
    } else {
        prop_oneof![2 => a_closed_leaf(), 1 => refs_onto(todos)].boxed()
    };
    grown_to(leaf, 3, 12, false)
}

/// One spec per todo in `TODOS`, each referring only to the todos after it,
/// so the references draw a DAG and every one links.
fn acyclic_specs() -> impl Strategy<Value = BTreeMap<String, Term>> {
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

/// The todos a term's references name.
fn refs_of(term: &Term) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut all = vec![];
    nodes(term, &mut all);
    for node in all {
        if let TermF::Ref { todo } = node.out() {
            out.insert(todo.clone());
        }
    }
    out
}

fn a_closed_leaf() -> BoxedStrategy<Term> {
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
                    .map(|(at, v)| fpl::CurvePoint {
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

/// Every constructor over `leaf`, four levels deep.
fn grown(leaf: BoxedStrategy<Term>) -> BoxedStrategy<Term> {
    grown_to(leaf, 4, 48, true)
}

/// Every constructor over `leaf`, `Within` only where `windows`: a linked term
/// nests its specs, and windows nested through several references multiply
/// their 65 samples into a case that never finishes.
fn grown_to(
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
fn an_env() -> impl Strategy<Value = Env> {
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
    /// `After` and `Recur` are the only constructors whose evaluation consults
    /// the environment — every other one is a function of `now` and its
    /// subterms — so neither anywhere is the proof that no lookup happens, and
    /// the environment that answers differently about everything is the same
    /// proof taken behaviourally.
    #[test]
    fn compilation_leaves_nothing_that_reads_history(
        term in a_term(),
        env in an_env(),
        probes in prop::collection::vec(-500i64..500, 4),
    ) {
        let compiled = compile(&term, &env);
        let mut all = vec![];
        nodes(compiled.term().term(), &mut all);
        for node in &all {
            prop_assert!(
                !matches!(node.out(), TermF::After { .. } | TermF::Recur { .. }),
                "an After or a Recur survived compilation",
            );
            prop_assert!(schedule_is_normal(node).is_ok(), "{:?}", schedule_is_normal(node));
        }
        let poisoned = Env {
            outcomes: EVENTS
                .iter()
                .map(|e| ((*e).to_string(), Outcome::Cancelled(moment(-10_000))))
                .collect(),
            tended: EVENTS
                .iter()
                .map(|e| ((*e).to_string(), [moment(-10_000)].into()))
                .collect(),
        };
        for h in probes {
            let now = moment(h);
            prop_assert_eq!(
                compiled.fulfillment(now),
                fulfillment(compiled.term().term(), now, &poisoned),
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

/// `Ref`'s id obeys `TodoId`'s rule, in the constructor and so in the parse.
#[test]
fn a_ref_names_a_todo_or_is_refused() {
    let good = ok(fpl::mk_ref("todo-1".to_owned()));
    assert_eq!(fpl::print_term(&good), "Ref(todo='todo-1')");
    for bad in ["", "Todo", "a b", "x!"] {
        assert!(
            fpl::mk_ref(bad.to_owned()).is_err(),
            "{bad:?} is not a todo id"
        );
        let print = format!("Ref(todo={bad:?})");
        assert!(fpl::parse_term(&print).is_err(), "{print} parsed");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// §2: `parse ∘ print` is the identity on terms, `Ref` included.
    #[test]
    fn print_then_parse_is_the_identity(term in an_open_term()) {
        let print = fpl::print_term(&term);
        let back = fpl::parse_term(&print).expect("a canonical print parses");
        prop_assert_eq!(fpl::print_term(&back), print);
        prop_assert_eq!(back, term);
    }
}

// --- §7 linking: the laws of bind ---------------------------------------------

/// The linked reading of `term` at every probe, or why it does not link.
fn readings(
    term: &Term,
    specs: &BTreeMap<String, Term>,
    env: &Env,
    probes: &[i64],
) -> Result<Vec<f64>, LinkError> {
    let linked = link(term, specs)?;
    Ok(probes
        .iter()
        .map(|h| fpl::fulfillment(&linked, moment(*h), env))
        .collect())
}

fn close_enough(left: &[f64], right: &[f64]) -> bool {
    left.len() == right.len() && left.iter().zip(right).all(|(a, b)| (a - b).abs() <= 1e-12)
}

#[test]
fn a_reference_to_a_todo_the_specs_do_not_hold_is_unknown() {
    let specs: BTreeMap<String, Term> = [("alpha".to_owned(), ok(fpl::mk_flat(0.5)))].into();
    let term = ok(fpl::mk_conj(
        vec![
            ok(fpl::mk_ref("alpha".to_owned())),
            ok(fpl::mk_ref("zeta".to_owned())),
        ],
        -4.0,
    ));
    let refusal = link(&term, &specs).expect_err("zeta is not a todo here");
    assert_eq!(refusal, LinkError::Unknown("zeta".to_owned()));
    assert_eq!(
        refusal.to_string(),
        "Ref(\"zeta\") names no todo with a function"
    );
}

#[test]
fn a_todo_that_references_itself_is_a_cycle_of_one() {
    let looped = ok(fpl::mk_offset(0.5, ok(fpl::mk_ref("alpha".to_owned()))));
    let specs: BTreeMap<String, Term> = [("alpha".to_owned(), looped)].into();
    let refusal = link(&ok(fpl::mk_ref("alpha".to_owned())), &specs).expect_err("a loop");
    assert_eq!(
        refusal,
        LinkError::Cycle(vec!["alpha".to_owned(), "alpha".to_owned()])
    );
    assert_eq!(refusal.to_string(), "the references loop: alpha → alpha");
}

/// A reference whose spec is a schedule, in a piece of another schedule, is
/// spliced like any nested schedule: the linked term is in normal form.
#[test]
fn a_linked_schedule_is_spliced_into_the_one_it_lands_in() {
    let inner = mk_piecewise(
        ok(fpl::mk_flat(0.1)),
        vec![(moment(30), ok(fpl::mk_flat(0.3)))],
    )
    .expect("ordered");
    let outer = mk_piecewise(
        ok(fpl::mk_flat(0.9)),
        vec![(moment(20), ok(fpl::mk_ref("alpha".to_owned())))],
    )
    .expect("ordered");
    let specs: BTreeMap<String, Term> = [("alpha".to_owned(), inner)].into();
    let linked = link(&outer, &specs).expect("links");
    assert_eq!(
        fpl::print_term(linked.term()),
        "Piecewise(head=Flat(value=0.9), pieces=(Piece(at=datetime(2026, 9, 1, 20, 0, 0), \
         term=Flat(value=0.1)), Piece(at=datetime(2026, 9, 2, 6, 0, 0), term=Flat(value=0.3))))"
    );
    assert_eq!(schedule_is_normal(linked.term()), Ok(()));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Unit: a closed term links to itself, whatever the specs say.
    #[test]
    fn a_closed_term_links_to_itself(term in a_term(), specs in acyclic_specs()) {
        prop_assert_eq!(link(&term, &specs), Ok(closed(&term)));
        prop_assert_eq!(link(&term, &BTreeMap::new()), Ok(closed(&term)));
    }

    /// Linking keeps `mk_piecewise`'s normal form: a spec that is a schedule,
    /// landing in a piece, is spliced and not nested.
    #[test]
    fn a_linked_term_is_in_normal_form(
        term in a_spec_onto(TODOS.to_vec()),
        specs in acyclic_specs(),
    ) {
        let linked = link(&term, &specs).expect("acyclic");
        let mut all = vec![];
        nodes(linked.term(), &mut all);
        for node in &all {
            prop_assert!(schedule_is_normal(node).is_ok(), "{:?}", schedule_is_normal(node));
        }
    }

    /// A reference reads what the spec it names reads, at every instant.
    #[test]
    fn a_reference_reads_what_its_spec_reads(
        specs in acyclic_specs(),
        todo in prop::sample::select(TODOS.to_vec()),
        env in an_env(),
        probes in prop::collection::vec(-500i64..500, 6),
    ) {
        let reference = ok(fpl::mk_ref(todo.to_owned()));
        let by_name = readings(&reference, &specs, &env, &probes).expect("acyclic");
        let by_spec = readings(&specs[todo], &specs, &env, &probes).expect("acyclic");
        prop_assert!(close_enough(&by_name, &by_spec), "{by_name:?} vs {by_spec:?}");
    }

    /// Associativity of bind: linking a term against specs that still hold
    /// references reads the same as linking the specs first and the term
    /// against the closed ones.
    #[test]
    fn linking_commutes_with_substitution_order(
        term in a_spec_onto(TODOS.to_vec()),
        specs in acyclic_specs(),
        env in an_env(),
        probes in prop::collection::vec(-500i64..500, 6),
    ) {
        let inner_first: BTreeMap<String, Term> = specs
            .iter()
            .map(|(todo, spec)| {
                (todo.clone(), link(spec, &specs).expect("acyclic").into_term())
            })
            .collect();
        prop_assert!(inner_first.values().all(|spec| refs_of(spec).is_empty()));
        let outer = readings(&term, &specs, &env, &probes).expect("acyclic");
        let inner = readings(&term, &inner_first, &env, &probes).expect("closed specs");
        prop_assert!(close_enough(&outer, &inner), "{outer:?} vs {inner:?}");
    }

    /// A loop is refused, never evaluated, and the refusal is a real cycle:
    /// it closes on itself and every step is a reference its spec holds.
    #[test]
    fn a_cycle_is_refused_and_named(
        specs in acyclic_specs(),
        (from, to) in (0..TODOS.len()).prop_flat_map(|i| (Just(i), i..TODOS.len())),
    ) {
        // `to` is at or after `from`, so a reference back from `to` to `from`
        // closes a loop through the edge added from `from` to `to`.
        let (upstream, downstream) = (TODOS[from], TODOS[to]);
        let mut looped = specs.clone();
        let back = |spec: &Term, onto: &str| {
            ok(fpl::mk_conj(vec![spec.clone(), ok(fpl::mk_ref(onto.to_owned()))], -4.0))
        };
        looped.insert(upstream.to_owned(), back(&specs[upstream], downstream));
        looped.insert(downstream.to_owned(), back(&looped[downstream], upstream));
        let refusal = link(&ok(fpl::mk_ref(upstream.to_owned())), &looped);
        let Err(LinkError::Cycle(path)) = refusal else {
            return Err(TestCaseError::fail(format!("no cycle refused: {refusal:?}")));
        };
        prop_assert!(path.len() >= 2 && path.first() == path.last(), "{path:?}");
        for step in path.windows(2) {
            prop_assert!(
                refs_of(&looped[&step[0]]).contains(&step[1]),
                "{} does not reference {}", step[0], step[1]
            );
        }
    }
}

// --- tendings: the grow-only set, read as of now -----------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// `last_tended` reads AS OF `now`: the latest tending at or before it, none
    /// before the first, and a tending dated later changes nothing.
    #[test]
    fn the_last_tending_is_read_as_of_now(
        tendings in prop::collection::btree_set(hours(), 0..6),
        now in hours(),
        later in 1i64..400,
    ) {
        let mut env = Env::new();
        env.tended.insert("alpha".to_owned(), tendings.iter().map(|h| moment(*h)).collect());
        let expected = tendings.range(..=now).next_back().map(|h| moment(*h));
        prop_assert_eq!(fpl::last_tended(&env, "alpha", moment(now)), expected);
        env.tended.entry("alpha".to_owned()).or_default().insert(moment(now + later));
        prop_assert_eq!(fpl::last_tended(&env, "alpha", moment(now)), expected);
        prop_assert_eq!(fpl::last_tended(&env, "beta", moment(now)), None);
    }
}

// --- Recur and Periodic -------------------------------------------------------

/// `env` with `todo`'s tendings replaced by `tendings`.
fn tended(mut env: Env, todo: &str, tendings: &BTreeSet<i64>) -> Env {
    env.tended.insert(
        todo.to_owned(),
        tendings.iter().map(|h| moment(*h)).collect(),
    );
    env
}

fn recur(todo: &str, anchor: i64, body: &Term, pending: &Term) -> Term {
    ok(fpl::mk_recur(
        todo.to_owned(),
        moment(anchor),
        body.clone(),
        pending.clone(),
    ))
}

#[test]
fn recur_and_periodic_refuse_what_they_must() {
    let flat = ok(fpl::mk_flat(0.5));
    for bad in ["", "Todo", "a b"] {
        assert!(fpl::mk_recur(bad.to_owned(), moment(0), flat.clone(), flat.clone()).is_err());
    }
    for hours in [0, -24] {
        assert!(fpl::mk_periodic(Duration::hours(hours), moment(0), flat.clone()).is_err());
    }
    for print in [
        "Recur(todo='', anchor=datetime(2026, 9, 1, 0, 0, 0), term=Flat(value=0.5), pending=Flat(value=0.5))",
        "Periodic(period=timedelta(), anchor=datetime(2026, 9, 1, 0, 0, 0), term=Flat(value=0.5))",
        "Periodic(period=timedelta(days=-1), anchor=datetime(2026, 9, 1, 0, 0, 0), term=Flat(value=0.5))",
    ] {
        assert!(fpl::parse_term(print).is_err(), "{print} parsed");
    }
}

/// The toothbrush: vinegar every two months, a decay over sixty days that a
/// pass restarts. Its explanation names the pass and how long ago it was.
#[test]
fn a_recurrence_explains_its_last_tending() {
    let print = "Recur(todo='toothbrushvinegar', anchor=datetime(2026, 8, 12, 0, 0, 0), \
                 term=Decay(start=0.98, end=0.3, end_date=datetime(2026, 10, 11, 0, 0, 0), \
                 lead_up=timedelta(days=60), start_date=None), pending=Flat(value=0.3))";
    let term = fpl::parse_term(print).expect("parses");
    assert_eq!(fpl::print_term(&term), print);
    let day = |m, d| {
        NaiveDate::from_ymd_opt(2026, m, d)
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .expect("a real date")
    };
    let mut env = Env::new();
    env.tended
        .insert("toothbrushvinegar".to_owned(), [day(8, 12)].into());
    let explanation = fpl::explained(&closed(&term), day(9, 25), &env);
    let note = |key: &str| explanation.notes[key].clone();
    assert_eq!(
        note("bound"),
        fpl::Note::One(fpl::Scalar::Text("tended".into()))
    );
    assert_eq!(
        note("tended"),
        fpl::Note::One(fpl::Scalar::Text("2026-08-12T00:00:00".into()))
    );
    assert_eq!(
        note("agoHours"),
        fpl::Note::One(fpl::Scalar::Float(44.0 * 24.0))
    );
    assert!((explanation.value - (0.98 - 0.68 * 44.0 / 60.0)).abs() < 1e-12);
    assert_eq!(fpl::fulfillment(&closed(&term), day(8, 1), &env), 0.3);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// RECUR RE-ANCHORS: with the last tending at `s`, `Recur` at `s + d` is
    /// its body at `anchor + d` — `After`'s slide, from the last pass.
    #[test]
    fn recur_re_anchors_to_the_last_tending(
        body in a_term(),
        pending in a_term(),
        anchor in hours(),
        env in an_env(),
        tendings in prop::collection::btree_set(hours(), 1..5),
        d in 0i64..800,
    ) {
        let env = tended(env, "alpha", &tendings);
        let s = *tendings.last().expect("at least one tending");
        prop_assert_eq!(
            fulfillment(&recur("alpha", anchor, &body, &pending), moment(s + d), &env),
            fulfillment(&body, moment(anchor + d), &env)
        );
    }

    /// RECUR WAITS: with no tending at or before `now`, it is `pending`.
    #[test]
    fn recur_waits_for_the_first_tending(
        body in a_term(),
        pending in a_term(),
        anchor in hours(),
        env in an_env(),
        tendings in prop::collection::btree_set(hours(), 0..5),
        early in 1i64..400,
    ) {
        let env = tended(env, "alpha", &tendings);
        let now = moment(tendings.first().map_or(0, |first| first - early));
        prop_assert_eq!(
            fulfillment(&recur("alpha", anchor, &body, &pending), now, &env),
            fulfillment(&pending, now, &env)
        );
    }

    /// RECUR IGNORES THE FUTURE: a tending dated after `now` changes nothing
    /// at `now`. Stated for the Recur's OWN lookup — `delta` is a todo the
    /// generated bodies never read — because a tending before the anchor slides
    /// the body LATER than `now`, and a body that itself reads history there
    /// sees what is recorded there, exactly as under `After` or `Shift`.
    #[test]
    fn a_later_tending_changes_no_earlier_recur(
        body in a_term(),
        pending in a_term(),
        anchor in hours(),
        env in an_env(),
        tendings in prop::collection::btree_set(hours(), 0..5),
        now in hours(),
        later in 1i64..400,
    ) {
        let term = recur("delta", anchor, &body, &pending);
        let before = tended(env, "delta", &tendings);
        let mut more = tendings.clone();
        more.insert(now + later);
        let after = tended(before.clone(), "delta", &more);
        prop_assert_eq!(
            fulfillment(&term, moment(now), &after),
            fulfillment(&term, moment(now), &before)
        );
    }

    /// PERIODIC REPEATS: one period on it reads the same, and on its first
    /// cycle `[anchor, anchor + period)` it is its body.
    #[test]
    fn periodic_repeats_its_first_cycle(
        body in a_term(),
        period in 1i64..400,
        anchor in hours(),
        env in an_env(),
        now in hours(),
        into in 0.0f64..1.0,
    ) {
        let term = ok(fpl::mk_periodic(Duration::hours(period), moment(anchor), body.clone()));
        prop_assert_eq!(
            fulfillment(&term, moment(now + period), &env),
            fulfillment(&term, moment(now), &env)
        );
        let inside = moment(anchor) + Duration::seconds((into * (period * 3600) as f64) as i64);
        prop_assert_eq!(fulfillment(&term, inside, &env), fulfillment(&body, inside, &env));
    }
}
