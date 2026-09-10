//! SPEC §7.1 — the chain compiler, on cases chosen rather than sampled: what
//! each link compiles to, that a cancellation is reported and not silent, that
//! `needs` is the compiler's field and not the evaluator's, that a cyclic
//! chain is refused as a value, and what the whole thing costs.
//!
//! §9.13 — the equivalence law itself — is in `fpl_laws.rs`, beside the
//! generators it quantifies over.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant as Clock;

use chrono::{Duration, NaiveDate};
use prodrome::chain::{chain_order, compile, compile_chain, ChainError, Link};
use prodrome::fpl::{
    self, delta_from_hours, fulfillment, iso, print_term, Env, Instant, Note, Outcome, Scalar,
    Term, TermF, WITHIN_SAMPLES,
};

fn origin() -> Instant {
    NaiveDate::from_ymd_opt(2026, 9, 1)
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .expect("a real date")
}

fn moment(hours: i64) -> Instant {
    origin() + Duration::hours(hours)
}

fn ok(term: Result<Term, fpl::FplError>) -> Term {
    term.expect("the constructors admit these arguments")
}

fn flat(value: f64) -> Term {
    ok(fpl::mk_flat(value))
}

/// A body whose value actually MOVES with time, so a slippage is visible as a
/// different number and not merely as a different print.
fn sloping() -> Term {
    ok(fpl::mk_decay(
        0.9,
        0.1,
        moment(200),
        Duration::hours(100),
        None,
    ))
}

fn after(event: &str, anchor: i64, term: Term, pending: Term, needs: Option<Duration>) -> Term {
    ok(fpl::mk_after(
        event.to_owned(),
        moment(anchor),
        term,
        pending,
        needs,
    ))
}

fn env(bindings: &[(&str, Outcome)]) -> Env {
    bindings
        .iter()
        .map(|(name, outcome)| ((*name).to_owned(), *outcome))
        .collect()
}

/// Every node of a term, the root included.
fn nodes(term: &Term, out: &mut Vec<Term>) {
    out.push(term.clone());
    for child in term.out().children() {
        nodes(child, out);
    }
}

/// THE COUNTING ENV, statically: how many nodes of this term CAN consult the
/// environment. `After` is the only one — every other constructor of §7 is a
/// function of `now` and its subterms — so this counts `After` nodes, and zero
/// is the proof that an evaluation makes no lookup at all.
fn env_readers(term: &Term) -> usize {
    let mut all = vec![];
    nodes(term, &mut all);
    all.iter()
        .filter(|node| matches!(node.out(), TermF::After { .. }))
        .count()
}

/// THE COUNTING ENV, behaviourally: an environment that answers DIFFERENTLY
/// about everything. A term that consulted it would move; one that cannot,
/// cannot.
fn poison(term: &Term) -> Env {
    let mut events = BTreeSet::new();
    let mut all = vec![];
    nodes(term, &mut all);
    for node in &all {
        if let TermF::After { event, .. } = node.out() {
            events.insert(event.clone());
        }
    }
    events
        .into_iter()
        .map(|event| (event, Outcome::Cancelled(moment(-10_000))))
        .collect()
}

const PROBES: [i64; 14] = [-300, -1, 0, 1, 10, 11, 24, 31, 55, 61, 65, 100, 250, 600];

/// The compiled term reads what the interpreted one read, at every probe, and
/// it reads the same under the empty environment and under one that answers
/// differently about everything — because it holds nothing that could ask.
fn agrees(term: &Term, snapshot: &Env) {
    let compiled = compile(term, snapshot);
    assert_eq!(
        env_readers(compiled.term()),
        0,
        "a compiled term still holds something that reads history: {}",
        print_term(compiled.term())
    );
    let poisoned = poison(term);
    for hours in PROBES {
        let now = moment(hours);
        let interpreted = fulfillment(term, now, snapshot);
        let value = compiled.fulfillment(now);
        assert!(
            (interpreted - value).abs() <= 1e-9,
            "at {}: interpreted {interpreted}, compiled {value}",
            iso(now)
        );
        assert_eq!(
            value,
            fulfillment(compiled.term(), now, &poisoned),
            "at {}: the compiled term consulted the environment",
            iso(now)
        );
    }
}

#[test]
fn an_unbound_link_is_its_pending_branch_and_says_so() {
    let term = after("beta", 0, sloping(), flat(0.35), None);
    let compiled = compile(&term, &Env::new());
    assert_eq!(print_term(compiled.term()), "Flat(value=0.35)");
    assert_eq!(
        compiled.links(),
        [Link::Pending {
            event: "beta".to_owned()
        }]
    );
    assert_eq!(compiled.links()[0].grade(), 0.0);
    agrees(&term, &Env::new());
}

#[test]
fn a_completed_link_is_a_schedule_whose_piece_shifts_by_the_slippage() {
    // Authored against hour 0, actually done at hour 24: the body slides back
    // a day, so from hour 24 the link reads what the body read at hour 0.
    let term = after("beta", 0, sloping(), flat(0.35), None);
    let snapshot = env(&[("beta", Outcome::Completed(moment(24)))]);
    let compiled = compile(&term, &snapshot);
    assert_eq!(
        print_term(compiled.term()),
        "Piecewise(head=Flat(value=0.35), pieces=(Piece(at=datetime(2026, 9, 2, 0, 0, 0), \
         term=Shift(delta=timedelta(days=-1), term=Decay(start=0.9, end=0.1, \
         end_date=datetime(2026, 9, 9, 8, 0, 0), lead_up=timedelta(days=4, seconds=14400), \
         start_date=None))),))"
    );
    assert_eq!(
        compiled.links(),
        [Link::Completed {
            event: "beta".to_owned(),
            at: moment(24),
            slip: Duration::hours(24),
            needs: None,
            ready: None,
        }]
    );
    agrees(&term, &snapshot);
}

#[test]
fn a_link_completed_on_its_anchor_writes_no_shift() {
    let term = after("beta", 24, sloping(), flat(0.35), None);
    let snapshot = env(&[("beta", Outcome::Completed(moment(24)))]);
    let compiled = compile(&term, &snapshot);
    assert!(
        !print_term(compiled.term()).contains("Shift"),
        "Shift(0, x) is x and is not written: {}",
        print_term(compiled.term())
    );
    agrees(&term, &snapshot);
}

#[test]
fn a_cancelled_upstream_is_a_graded_offset_at_one_and_is_reported() {
    let term = after("beta", 0, flat(0.2), flat(0.35), None);
    let snapshot = env(&[("beta", Outcome::Cancelled(moment(24)))]);
    let compiled = compile(&term, &snapshot);
    // The moot constant, WRITTEN AS THE OFFSET IT IS: x·(1−|1|) + max(0, 1) = 1
    // for every x, and the demand that was dropped is still in the tree.
    assert_eq!(
        print_term(compiled.term()),
        "Piecewise(head=Flat(value=0.35), pieces=(Piece(at=datetime(2026, 9, 2, 0, 0, 0), \
         term=Offset(delta=1.0, term=Flat(value=0.2))),))"
    );
    assert_eq!(compiled.fulfillment(moment(25)), 1.0);
    assert_eq!(compiled.fulfillment(moment(23)), 0.35);

    // NEVER SILENTLY (§7): the 1.0 above is indistinguishable from a demand
    // met, and these are what tell them apart.
    let moot: Vec<&str> = compiled.moot().map(Link::event).collect();
    assert_eq!(moot, ["beta"]);
    assert_eq!(compiled.links()[0].grade(), 1.0);
    assert_eq!(
        compiled.notes().get("moot"),
        Some(&Note::Many(vec![Scalar::Text("beta".to_owned())]))
    );
    assert_eq!(
        compiled.explain(moment(25)).notes.get("moot"),
        Some(&Note::Many(vec![Scalar::Text("beta".to_owned())])),
        "explain carries the cancellation to the reader at the root"
    );
    agrees(&term, &snapshot);
}

#[test]
fn explain_over_a_compiled_term_is_still_a_tree() {
    let term = after("beta", 0, sloping(), flat(0.35), None);
    let snapshot = env(&[("beta", Outcome::Completed(moment(24)))]);
    let compiled = compile(&term, &snapshot);
    let explanation = compiled.explain(moment(100));
    assert_eq!(explanation.value, compiled.fulfillment(moment(100)));
    // The schedule's decoration is its piece in force (§7's documented bend),
    // and the piece is the shifted body — so the reader walks down to the
    // Decay that actually answered.
    assert_eq!(explanation.node.kind(), "piecewise");
    let TermF::Piecewise { head, .. } = &*explanation.node else {
        panic!("the root is the schedule the compiler built");
    };
    assert_eq!(head.node.kind(), "shift");
    assert_eq!(
        compiled.notes().get("links").map(|_| ()),
        Some(()),
        "every link is a note, bound or not"
    );
}

#[test]
fn needs_is_the_compilers_field_and_never_a_value() {
    let lead = delta_from_hours(48.0);
    let bare = after("beta", 0, sloping(), flat(0.35), None);
    let declared = after("beta", 0, sloping(), flat(0.35), Some(lead));
    let snapshot = env(&[("beta", Outcome::Completed(moment(24)))]);

    // Same term, same reading, with and without the lead time: §7 has never
    // read `needs` and compiling does not start.
    for hours in PROBES {
        let now = moment(hours);
        assert_eq!(
            compile(&bare, &snapshot).fulfillment(now),
            compile(&declared, &snapshot).fulfillment(now)
        );
    }
    assert_eq!(
        print_term(compile(&bare, &snapshot).term()),
        print_term(compile(&declared, &snapshot).term())
    );

    // What it DOES buy: the earliest moment the link's own demand could be
    // met, which the compiler is the first reader able to say.
    let compiled = compile(&declared, &snapshot);
    assert_eq!(
        compiled.links(),
        [Link::Completed {
            event: "beta".to_owned(),
            at: moment(24),
            slip: Duration::hours(24),
            needs: Some(lead),
            ready: Some(moment(72)),
        }]
    );
}

/// A needs B needs C, as one term: three freeze quantifiers become ONE
/// schedule over the merged partition of the three instants.
fn a_chain() -> Term {
    let innermost = after("gamma", 0, sloping(), flat(0.2), None);
    let middle = after("beta", 0, innermost, flat(0.3), None);
    after("alpha", 0, middle, flat(0.4), None)
}

#[test]
fn a_chain_of_three_links_composes_into_one_schedule() {
    let snapshot = env(&[
        ("alpha", Outcome::Completed(moment(10))),
        ("beta", Outcome::Completed(moment(20))),
        ("gamma", Outcome::Completed(moment(30))),
    ]);
    let compiled = compile(&a_chain(), &snapshot);
    assert_eq!(env_readers(compiled.term()), 0);
    let TermF::Piecewise { pieces, .. } = compiled.term().out() else {
        panic!(
            "the chain should be one schedule at the root, got {}",
            compiled.term().out().kind()
        );
    };
    // ONE PIECE PER LINK, each at the instant the links ABOVE it see: gamma
    // binds at 30, beta shifts the view of it back 20 hours so it lands at 50,
    // and alpha shifts that back another 10, so it lands at 60. That
    // arithmetic is the whole of what the compiler did, and it did it once.
    assert_eq!(
        pieces.iter().map(|(at, _)| *at).collect::<Vec<_>>(),
        [moment(10), moment(30), moment(60)],
        "one piece per link, over the merged partition"
    );
    assert_eq!(compiled.links().len(), 3);
    agrees(&a_chain(), &snapshot);
}

#[test]
fn a_chain_with_a_cancellation_in_the_middle_is_moot_from_there() {
    let snapshot = env(&[
        ("alpha", Outcome::Completed(moment(10))),
        ("beta", Outcome::Cancelled(moment(20))),
    ]);
    let compiled = compile(&a_chain(), &snapshot);
    // Moot from hour 30 and not from hour 20: alpha slipped 10 hours, so the
    // whole subchain under it is read 10 hours behind the clock.
    assert_eq!(compiled.fulfillment(moment(35)), 1.0);
    assert_eq!(compiled.fulfillment(moment(25)), 0.3);
    assert_eq!(
        compiled.moot().map(Link::event).collect::<Vec<_>>(),
        ["beta"]
    );
    // gamma is under beta's mooted body and IS still compiled, unbound.
    assert_eq!(compiled.links().len(), 3);
    agrees(&a_chain(), &snapshot);
}

// --- The chain as a graph ----------------------------------------------------

fn functions(pairs: Vec<(&str, Term)>) -> BTreeMap<String, Term> {
    pairs
        .into_iter()
        .map(|(name, term)| (name.to_owned(), term))
        .collect()
}

#[test]
fn a_chain_compiles_upstreams_first() {
    let chain = functions(vec![
        ("a", after("b", 0, sloping(), flat(0.4), None)),
        ("b", after("c", 0, sloping(), flat(0.3), None)),
        ("c", sloping()),
    ]);
    assert_eq!(chain_order(&chain).expect("acyclic"), ["c", "b", "a"]);
    let compiled = compile_chain(&chain, &Env::new()).expect("acyclic");
    assert_eq!(compiled.len(), 3);
    for (todo, one) in &compiled {
        assert_eq!(env_readers(one.term()), 0, "{todo} still reads history");
    }
}

#[test]
fn a_cycle_is_refused_as_a_value_and_names_the_loop() {
    let chain = functions(vec![
        ("a", after("b", 0, sloping(), flat(0.4), None)),
        ("b", after("a", 0, sloping(), flat(0.3), None)),
    ]);
    let refusal = compile_chain(&chain, &Env::new()).expect_err("a depends on b depends on a");
    assert_eq!(
        refusal,
        ChainError::Cycle(vec!["a".to_owned(), "b".to_owned(), "a".to_owned()])
    );
    assert_eq!(
        refusal.to_string(),
        "the chain depends on itself: a → b → a"
    );
    assert_eq!(chain_order(&chain), Err(refusal));
}

#[test]
fn a_todo_that_needs_itself_is_a_cycle_of_one() {
    let chain = functions(vec![("a", after("a", 0, sloping(), flat(0.4), None))]);
    assert_eq!(
        chain_order(&chain),
        Err(ChainError::Cycle(vec!["a".to_owned(), "a".to_owned()]))
    );
}

#[test]
fn a_link_onto_something_outside_the_chain_is_not_an_edge() {
    // `zeta` is not a todo of this chain: the snapshot answers it, and it
    // constrains no order and closes no loop.
    let chain = functions(vec![("a", after("zeta", 0, sloping(), flat(0.4), None))]);
    assert_eq!(chain_order(&chain).expect("acyclic"), ["a"]);
}

// --- The cost ----------------------------------------------------------------

/// THE COST MODEL, as a test rather than as a claim (§7.1).
///
/// The benchmark term is a chain of three links under one `Within`, which is
/// the shape the compiler exists for: the interpreter consults the environment
/// once per `After` it reaches, `Within` reaches its subterm 65 times, so this
/// term costs up to 65·3 = 195 lookups at EVERY evaluation and the compiled
/// one costs none — not fewer, none, because there is nothing left in it that
/// could ask.
///
/// The ASSERTIONS are on the counting env (both readings of it) and never on
/// the clock; the times are printed, because a wall-clock threshold in a test
/// is a flake waiting for a loaded machine.
///
/// AND THE CLOCK IS NOT THE POINT, which the printed numbers say plainly: in a
/// release build this shape spends about 7% less wall time compiled, because a
/// lookup in a three-entry map is cheap. What the compiler buys is that the
/// environment leaves the query path — a compiled term can be cached, stored,
/// shipped and evaluated where no history exists — not a constant factor.
#[test]
fn the_compiled_term_makes_no_lookup_and_the_walk_is_paid_once() {
    let term = ok(fpl::mk_within(Duration::hours(72), -4.0, a_chain()));
    let snapshot = env(&[
        ("alpha", Outcome::Completed(moment(10))),
        ("beta", Outcome::Completed(moment(20))),
        ("gamma", Outcome::Cancelled(moment(30))),
    ]);
    let samples = WITHIN_SAMPLES as usize + 1;
    assert_eq!(env_readers(&term), 3, "three links under one window");

    let started = Clock::now();
    let compiled = compile(&term, &snapshot);
    let compiling = started.elapsed();

    assert_eq!(
        env_readers(compiled.term()),
        0,
        "the compiled term still holds something that reads history"
    );
    agrees(&term, &snapshot);

    let rounds = 2_000;
    let started = Clock::now();
    for i in 0..rounds {
        std::hint::black_box(fulfillment(&term, moment(i % 400), &snapshot));
    }
    let interpreted = started.elapsed();
    let started = Clock::now();
    for i in 0..rounds {
        std::hint::black_box(compiled.fulfillment(moment(i % 400)));
    }
    let evaluated = started.elapsed();

    println!(
        "chain of {} links under a {samples}-sample window, {rounds} evaluations:\n  \
         interpreted {interpreted:?} ({} environment lookups)\n  \
         compiled    {evaluated:?} (0 environment lookups), after one {compiling:?} compile",
        env_readers(&term),
        env_readers(&term) * samples * rounds as usize,
    );
}
