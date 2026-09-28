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
//!
//! And the laws of `∅` (§9.18): `Absent` is the identity of composition, a
//! term without one has a value everywhere, and `explain` and the series
//! carry `∅` where a node has none. Every generator here draws `Absent` among
//! its leaves, so every law above quantifies over it too.
//!
//! And law 26: `Least` is a semilattice with `Absent` as its identity; the
//! generators draw it, so every law above covers it.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, NaiveDate};
use common::terms::{
    a_spec_onto, a_term, a_valued_term, acyclic_specs, an_env, an_exact_term, an_open_term, hours,
    moment, ok, EVENTS, TODOS,
};
use prodrome::chain::{self, Compiled};
use prodrome::fpl::{self, link, mk_piecewise, Closed, Env, Instant, LinkError, Outcome};
use prodrome::term::{normalize, Term, TermF};
use proptest::prelude::*;

fn closed(term: &Term) -> Closed {
    Closed::of(term.clone()).expect("a_term() builds no Ref")
}

fn fulfillment(term: &Term, now: Instant, env: &Env) -> Option<f64> {
    fpl::fulfillment(&closed(term), now, env)
}

/// Two readings agree: both `∅`, or both numbers within `tolerance`.
fn near(left: Option<f64>, right: Option<f64>, tolerance: f64) -> bool {
    match (left, right) {
        (Some(a), Some(b)) => (a - b).abs() <= tolerance,
        (a, b) => a == b,
    }
}

fn compile(term: &Term, env: &Env) -> Compiled {
    chain::compile(&closed(term), env)
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

    /// The functor's folds: `cata` of the constructor and `para` of the
    /// subterms it sees each rebuild the term.
    #[test]
    fn folding_with_the_constructor_is_the_identity(term in an_open_term()) {
        prop_assert_eq!(term.cata(Term::new), term.clone());
        prop_assert_eq!(
            term.para(|layer| Term::new(layer.map(|(below, _)| (*below).clone()))),
            term
        );
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
                near(before, after, 1e-12),
                "at {now}: {before:?} became {after:?}",
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
                TermF::Conj { terms, .. } | TermF::Least { terms } => terms.iter().any(is_piecewise),
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
                near(interpreted, value, 1e-9),
                "at {now}: {interpreted:?} became {value:?}",
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
            prop_assert!(
                near(interpreted, value, 1e-9),
                "at {now}: {interpreted:?} vs {value:?}"
            );
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
) -> Result<Vec<Option<f64>>, LinkError> {
    let linked = link(term, specs)?;
    Ok(probes
        .iter()
        .map(|h| fpl::fulfillment(&linked, moment(*h), env))
        .collect())
}

fn close_enough(left: &[Option<f64>], right: &[Option<f64>]) -> bool {
    left.len() == right.len() && left.iter().zip(right).all(|(a, b)| near(*a, *b, 1e-12))
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
    assert_eq!(refusal.to_string(), "Ref(\"zeta\") names no known todo");
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
    assert!(near(
        explanation.value,
        Some(0.98 - 0.68 * 44.0 / 60.0),
        1e-12
    ));
    assert_eq!(fpl::fulfillment(&closed(&term), day(8, 1), &env), Some(0.3));
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

// --- §7's empty term: the laws of ∅ ---------------------------------------------

fn absent() -> Term {
    fpl::mk_absent()
}

/// Every node of an explanation, the root included.
fn explained_nodes<'a>(node: &'a fpl::Explanation, out: &mut Vec<&'a fpl::Explanation>) {
    out.push(node);
    for child in node.node.children() {
        explained_nodes(child, out);
    }
}

/// `Conj([])` keeps its stored meaning of 0.5, and a conjunction whose every
/// member is absent is `∅`: the first has no member to be absent.
#[test]
fn an_empty_conjunction_is_one_half_and_an_absent_one_is_empty() {
    let env = Env::new();
    let conj = |terms| ok(fpl::mk_conj(terms, fpl::PRIORITY_POWER));
    assert_eq!(fulfillment(&conj(vec![]), moment(0), &env), Some(0.5));
    assert_eq!(fulfillment(&conj(vec![absent()]), moment(0), &env), None);
    assert_eq!(fulfillment(&absent(), moment(0), &env), None);
    assert_eq!(fpl::print_term(&absent()), "Absent()");
    assert_eq!(fpl::parse_term("Absent()"), Ok(absent()));
}

/// `Absent` is exact and has no breakpoints: its series is its two window
/// edges, both `∅`.
#[test]
fn absent_is_exact_with_no_breakpoints() {
    let breaks = prodrome::breaks::breakpoints(&absent());
    assert!(breaks.exact && breaks.slopes.is_empty() && breaks.jumps.is_empty());
    let series =
        prodrome::breaks::series_knots(&closed(&absent()), moment(0), moment(48), |_| Env::new());
    assert!(series.exact);
    assert_eq!(
        series
            .knots
            .iter()
            .map(|knot| knot.value)
            .collect::<Vec<_>>(),
        [None, None]
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// `∅` IS THE IDENTITY OF COMPOSITION: `Conj(ts ++ [Absent], p) =
    /// Conj(ts, p)` whenever some member of `ts` has a value — bit for bit,
    /// since the mean is over the same numbers — and `∅` with it when none
    /// does.
    #[test]
    fn an_absent_member_changes_no_conjunction(
        terms in prop::collection::vec(a_term(), 1..4),
        p in prop::sample::select(vec![-8.0, -4.0, -1.0, 0.0]),
        env in an_env(),
        probes in prop::collection::vec(-500i64..500, 8),
    ) {
        let with = {
            let mut terms = terms.clone();
            terms.push(absent());
            ok(fpl::mk_conj(terms, p))
        };
        let without = ok(fpl::mk_conj(terms, p));
        for h in probes {
            let now = moment(h);
            prop_assert_eq!(fulfillment(&with, now, &env), fulfillment(&without, now, &env));
        }
    }

    /// A term with no `Absent` evaluates as it did before `∅` existed: it has
    /// a value at every instant, in [0, 1] (and `conformance/*.py`, which
    /// holds none, reads exactly as it did).
    #[test]
    fn a_term_without_absent_has_a_value_everywhere(
        term in a_valued_term(),
        env in an_env(),
        probes in prop::collection::vec(-500i64..500, 8),
    ) {
        for h in probes {
            let value = fulfillment(&term, moment(h), &env);
            prop_assert!(value.is_some_and(|v| (0.0..=1.0).contains(&v)), "{:?}", value);
        }
    }

    /// The table of §7, operator by operator: an absent gate or offset is
    /// none, an absent body or term is `∅`, and every unary operator passes
    /// `∅` through.
    #[test]
    fn absent_composes_as_the_table_says(
        term in a_term(),
        env in an_env(),
        w in 0.3f64..2.5,
        delta in -0.9f64..0.9,
        shift in hours(),
        h in -500i64..500,
    ) {
        let now = moment(h);
        let reads = |t: Term| fulfillment(&t, now, &env);
        let x = reads(term.clone());
        prop_assert_eq!(reads(ok(fpl::mk_gate(absent(), term.clone()))), x);
        prop_assert_eq!(reads(ok(fpl::mk_gate(term.clone(), absent()))), None);
        prop_assert_eq!(reads(ok(fpl::mk_offset_by(absent(), term.clone()))), x);
        prop_assert_eq!(reads(ok(fpl::mk_offset_by(term, absent()))), None);
        for passed in [
            ok(fpl::mk_offset(delta, absent())),
            ok(fpl::mk_importance(w, absent())),
            ok(fpl::mk_shift(Duration::hours(shift), absent())),
            ok(fpl::mk_within(Duration::hours(12), -1.0, absent())),
            ok(fpl::mk_periodic(Duration::hours(24), moment(0), absent())),
        ] {
            prop_assert_eq!(reads(passed), None);
        }
    }

    /// `explain` carries `∅` where a node has no value: the root reads what
    /// `fulfillment` reads, every `Absent` node is `∅`, and a conjunction
    /// with a value apportions its shares over the members that have one —
    /// an absent member's share is 0 and the rest still sum to 1.
    #[test]
    fn explain_carries_absent_where_a_node_has_no_value(
        term in a_term(),
        env in an_env(),
        h in -500i64..500,
    ) {
        let now = moment(h);
        let tree = fpl::explained(&closed(&term), now, &env);
        prop_assert_eq!(tree.value, fulfillment(&term, now, &env));
        let mut all = vec![];
        explained_nodes(&tree, &mut all);
        for node in all {
            if matches!(*node.node, TermF::Absent) {
                prop_assert_eq!(node.value, None);
            }
            if let (TermF::Conj { terms, .. }, Some(_)) = (&*node.node, node.value) {
                let Some(fpl::Note::Many(shares)) = node.notes.get("shares") else {
                    return Err(TestCaseError::fail("a valued conjunction has shares"));
                };
                prop_assert_eq!(shares.len(), terms.len());
                let mut sum = 0.0;
                for (share, member) in shares.iter().zip(terms) {
                    let fpl::Scalar::Float(share) = share else {
                        return Err(TestCaseError::fail("a share is a float"));
                    };
                    if member.value.is_none() {
                        prop_assert_eq!(*share, 0.0);
                    }
                    sum += share;
                }
                prop_assert!(terms.is_empty() || (sum - 1.0).abs() < 1e-9, "shares sum to {}", sum);
            }
            if let (TermF::Conj { .. }, None) = (&*node.node, node.value) {
                prop_assert!(!node.notes.contains_key("shares") && !node.notes.contains_key("certifies"));
            }
        }
    }

    /// The series carry `∅` where the term has none, and on the EXACT
    /// fragment — `Absent` among it — the knots are the curve (§9.7): a line
    /// between two numbers, `∅` between two `∅`s, and a knot pair with one
    /// absent end is only ever the second that brackets a jump.
    #[test]
    fn the_knots_of_an_exact_term_are_its_curve_absent_included(
        term in an_exact_term(),
        from in -500i64..0,
        span in 1i64..500,
    ) {
        let term = closed(&term);
        let series =
            prodrome::breaks::series_knots(&term, moment(from), moment(from + span), |_| Env::new());
        prop_assert!(series.exact);
        for knot in &series.knots {
            prop_assert_eq!(knot.value, fpl::fulfillment(&term, knot.at, &Env::new()));
        }
        for pair in series.knots.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if b.at - a.at <= Duration::seconds(1) {
                continue;
            }
            let middle = a.at + (b.at - a.at) / 2;
            let real = fpl::fulfillment(&term, middle, &Env::new());
            match (a.value, b.value) {
                (Some(x), Some(y)) => {
                    let real = real.expect("a value between two values");
                    prop_assert!((x + (y - x) / 2.0 - real).abs() <= 1e-9, "{} vs {}", x + (y - x) / 2.0, real);
                }
                (None, None) => prop_assert_eq!(real, None),
                ends => return Err(TestCaseError::fail(format!("{ends:?} over more than a second"))),
            }
        }
    }
}

fn least(terms: Vec<Term>) -> Term {
    ok(fpl::mk_least(terms))
}

#[test]
fn a_least_has_a_member_and_prints_it() {
    assert!(fpl::mk_least(vec![]).is_err());
    assert!(fpl::parse_term("Least(terms=())").is_err());
    assert_eq!(
        fpl::print_term(&least(vec![absent()])),
        "Least(terms=(Absent(),))"
    );
}

/// Two lines that cross between their breakpoints bend the least there, and
/// the series puts a knot on the crossing.
#[test]
fn the_least_of_two_lines_has_a_knot_where_they_cross() {
    let falling = ok(fpl::mk_decay(
        0.9,
        0.1,
        moment(100),
        Duration::hours(100),
        None,
    ));
    let term = least(vec![falling, ok(fpl::mk_flat(0.5))]);
    let series =
        prodrome::breaks::series_knots(&closed(&term), moment(-10), moment(110), |_| Env::new());
    assert!(series.exact);
    assert!(series.knots.iter().any(|knot| knot.at == moment(50)));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Law 26: commutative, associative, idempotent, `Least([t]) = t`, and
    /// `Absent` is the identity.
    #[test]
    fn least_is_a_semilattice(
        a in a_term(),
        b in a_term(),
        c in a_term(),
        env in an_env(),
        h in -500i64..500,
    ) {
        let now = moment(h);
        let reads = |t: Term| fulfillment(&t, now, &env);
        let x = reads(a.clone());
        prop_assert_eq!(reads(least(vec![a.clone(), b.clone()])), reads(least(vec![b.clone(), a.clone()])));
        prop_assert_eq!(
            reads(least(vec![least(vec![a.clone(), b.clone()]), c.clone()])),
            reads(least(vec![a.clone(), least(vec![b.clone(), c])])),
        );
        prop_assert_eq!(reads(least(vec![a.clone(), a.clone()])), x);
        prop_assert_eq!(reads(least(vec![a.clone()])), x);
        prop_assert_eq!(reads(least(vec![a.clone(), absent()])), x);
        let both = reads(least(vec![a, b.clone()]));
        for member in [x, reads(b)].into_iter().flatten() {
            prop_assert!(both.is_some_and(|both| both <= member));
        }
    }
}
