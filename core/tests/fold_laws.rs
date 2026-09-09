//! SPEC §9.2–9.4, §9.6 and §9.9, as properties over random logs and random
//! two-replica DAGs.
//!
//! The vectors say this side agrees with the reference on the cases the
//! reference happened to generate. These say the AGREEMENT IS STRUCTURAL:
//! nothing rewrites history, order is causal and the stamp is data, the
//! history is the environment as a function of time, on any DAG the registers
//! are the folds with the conflicts named exactly, and the entry (§6.7) is the
//! composition of those folds and nothing else.
//!
//! The generators mirror the reference generator's `a_log`/`an_event` — the
//! same three todos, the same seven kinds in the same proportions, the same
//! two actors with one of them on the reference policy's roster — so a failure
//! here is a failure the
//! vector generator could have produced, and a fix is checkable against it.
//!
//! The DAG laws build REAL stores in temp directories and drive them the way a
//! second replica would: append, adopt, write concurrently, merge. Nothing
//! about a frontier may depend on this side having constructed the graph in
//! memory.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

use prodrome::event::{mk_completed, mk_spec_revised, Actor, TodoEvent, TodoId};
use prodrome::fold::{authored_at, env_at, flatten, history, specs_at, Binding, Env, History};
use prodrome::fpl::{self, print_term, Term};
use prodrome::literal::Datetime;
use prodrome::policy::{Everything, Policy, Untrusted};
use prodrome::reference::{mk_authored, mk_subtodo, Todo};
use prodrome::registers::{
    conflicts_of, content_of, env_of, extend, fold, nodes_of, since, specs_of, Folded, Kind, Node,
};
use prodrome::store::EventStore;
use prodrome::view;
use proptest::prelude::*;

use common::{a_draft, a_log, a_schedule, chain_of, far, moment, realise, Draft, WINDOW};

/// These laws are about the FOLDS, not about a record's fields, so the payload
/// they run under is the reference one — the shape the vector generator drew.
type Event = TodoEvent<Todo>;
type Chain = Node<Todo>;
type Store = EventStore<Todo, Untrusted>;
type State = Folded<Todo>;

/// The random log generator (`TODOS`, `ACTORS`, `WINDOW`, `origin`, `moment`,
/// `far`, `Draft`, `a_random_spec`, `a_draft`, `a_schedule`, `realise`,
/// `a_log`, `chain_of`) lives in `tests/common/mod.rs` now: it is also what
/// `examples/generate_view_vectors.rs` draws `conformance/view/*.json` from,
/// over a fixed seed, and a generator a vector file was taken from and a
/// property runs against had to be the same one.
fn roster() -> Untrusted {
    Untrusted::of([Actor::new("triage").expect("valid")])
}

/// A draft the reference policy can only let CLAIM: a lifecycle write or a
/// repricing, from the one actor on the roster. `Draft`'s rolls 3..7 are
/// `SpecRevised`, `Completed`, `Cancelled` and `Reopened` — the kinds §5 makes
/// provisional — and rolls 0..3 (`Created`, `Authored`) are the ones it does
/// not, which is why the range is exactly this one.
fn a_claim() -> impl Strategy<Value = Draft> {
    (a_draft(), 3u8..7).prop_map(|(draft, roll)| Draft {
        actor: "triage",
        roll,
        ..draft
    })
}

/// What every fold answers, as one comparable value. Terms and records go in
/// as their canonical PRINTS: a fold that agreed to nine decimals and printed
/// a different term would not be the same fold.
#[derive(Debug, PartialEq)]
struct Folds {
    env: Env,
    specs: BTreeMap<String, String>,
    content: BTreeMap<String, String>,
    flatten: BTreeMap<String, String>,
    history: History,
}

fn folds(log: &[Event], t: Datetime, policy: &impl Policy<Todo>) -> Folds {
    Folds {
        env: env_at(log, t, policy),
        specs: specs_at(log, t, policy)
            .iter()
            .map(|(todo, spec)| (todo.as_str().to_owned(), print_term(spec)))
            .collect(),
        content: authored_at(log, t)
            .iter()
            .map(|(todo, record)| {
                (
                    todo.as_str().to_owned(),
                    prodrome::literal::print_literal(&record.to_value()),
                )
            })
            .collect(),
        flatten: flatten(log, t, policy)
            .expect("the log folds")
            .iter()
            .map(|(todo, term)| (todo.as_str().to_owned(), print_term(term)))
            .collect(),
        history: history(log, policy),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// §9.4 — `history.at(t)` is `env_at(·, t)` at EVERY instant, not only at
    /// the ones a vector pinned.
    #[test]
    fn the_history_is_the_environment_as_a_function_of_time(
        log in a_log(),
        asked in prop::collection::vec(0i64..WINDOW, 1..6),
    ) {
        let policy = roster();
        let past = history(&log, &policy);
        for seconds in asked {
            let t = moment(seconds);
            prop_assert_eq!(past.at(t), env_at(&log, t, &policy));
        }
    }

    /// §9.2, THE PREFIX LAW — nothing rewrites history. Append an event later
    /// than the log's last, and at every earlier moment every todo's function
    /// reads exactly what it read before, against the environment of that
    /// moment.
    ///
    /// With ONE exception, which is `flatten`'s definition and not a bug: the
    /// HEAD is `checklist(first spec ever, first checklist ever)`, so an
    /// appended event that records EITHER half for the first time re-heads
    /// that todo's whole curve. It can only bite a todo that already had a
    /// function from the other half alone — a checklist with no spec, or a
    /// spec with no content record — and `a_late_first_half_of_the_head_re_heads_the_curve`
    /// below is the witness for both, read off the reference. Everything else
    /// is untouchable, which is what this asserts.
    #[test]
    fn appending_a_later_event_changes_no_earlier_moment(
        schedule in a_schedule(1..25),
        extra in a_draft(),
        gap in 1i64..3600,
        asked in prop::collection::vec(0i64..WINDOW, 1..6),
    ) {
        let policy = roster();
        let log = realise(&schedule, 0);
        let last = schedule.last().expect("a non-empty schedule").1;
        let tau = moment(last + gap);
        let mut longer = log.clone();
        longer.push(extra.at(tau));

        let before = flatten(&log, tau, &policy).expect("folds");
        let after = flatten(&longer, tau, &policy).expect("folds");
        // A later event can only ADD a function, never remove one.
        for todo in before.keys() {
            prop_assert!(after.contains_key(todo));
        }
        // A todo whose head the appended event DETERMINES: the first spec, or
        // the first content record (which fixes the first checklist length).
        // `specs_at`/`authored_at` hold a key exactly when the chain holds
        // such a write, which is the condition `flatten`'s head reads.
        let head_of = |log: &[Event]| -> (BTreeSet<TodoId>, BTreeSet<TodoId>) {
            (
                specs_at(log, tau, &policy).into_keys().collect(),
                authored_at(log, tau).into_keys().collect(),
            )
        };
        let (priced_before, written_before) = head_of(&log);
        let (priced_after, written_after) = head_of(&longer);
        let past_before = history(&log, &policy);
        let past_after = history(&longer, &policy);
        for seconds in asked {
            // STRICTLY before tau: the law is about EARLIER moments, and at
            // tau itself the appended event is in force by design — that is
            // the fold working, not history being rewritten.
            let t = moment(seconds.min(last + gap - 1));
            let now = fpl::instant_of(t);
            for (todo, term) in &before {
                let re_headed = (!priced_before.contains(todo) && priced_after.contains(todo))
                    || (!written_before.contains(todo) && written_after.contains(todo));
                if re_headed {
                    continue;
                }
                prop_assert_eq!(
                    fpl::fulfillment(term, now, &prodrome::fold::evaluation_env(&past_before.at(t))),
                    fpl::fulfillment(
                        &after[todo],
                        now,
                        &prodrome::fold::evaluation_env(&past_after.at(t))
                    ),
                    "{:?} at {:?}", todo, t
                );
            }
        }
    }

    /// §9.3, first half — INDEPENDENT EVENTS COMMUTE. Shuffle the log, then
    /// restore each todo's own order: that is a linearisation of the per-todo
    /// partial order, which is exactly what "independent" means. Every fold
    /// answers the same, so each is a function of the event SET and two
    /// replicas that merge in different orders converge.
    #[test]
    fn independent_events_commute(
        log in a_log(),
        keys in prop::collection::vec(0u32..1000, 0..25),
        at in 0i64..WINDOW,
    ) {
        let policy = roster();
        let t = moment(at);
        // A permutation of the positions, from keys the shrinker can shrink.
        let mut order: Vec<usize> = (0..log.len()).collect();
        order.sort_by_key(|i| (keys.get(*i).copied().unwrap_or(0), *i));

        let mut queues: BTreeMap<&TodoId, Vec<&Event>> = BTreeMap::new();
        for event in &log {
            queues.entry(event.todo()).or_default().push(event);
        }
        for queue in queues.values_mut() {
            queue.reverse(); // pop from the back keeps each todo's own order
        }
        let relinearised: Vec<Event> = order
            .iter()
            .map(|i| {
                queues
                    .get_mut(log[*i].todo())
                    .and_then(Vec::pop)
                    .expect("one event per position")
                    .clone()
            })
            .collect();
        prop_assert_eq!(relinearised.len(), log.len());
        prop_assert_eq!(folds(&relinearised, t, &policy), folds(&log, t, &policy));
    }

    /// §9.3, second half — THE STAMP IS DATA, NOT ORDER. Shift every `at` by
    /// the same amount and move the question by the same amount: the time
    /// machine's window moves, and no register's winner changes.
    #[test]
    fn a_uniform_shift_changes_no_winner(
        schedule in a_schedule(0..25),
        shift in -(WINDOW / 2)..(WINDOW / 2),
        at in 0i64..WINDOW,
    ) {
        let policy = roster();
        let here = realise(&schedule, 0);
        let there = realise(&schedule, shift);
        let t = moment(at);
        let moved = moment(at + shift);

        let kinds = |env: &Env| -> BTreeMap<String, &'static str> {
            env.iter()
                .map(|(todo, binding)| (todo.as_str().to_owned(), binding.kind()))
                .collect()
        };
        prop_assert_eq!(
            kinds(&env_at(&here, t, &policy)),
            kinds(&env_at(&there, moved, &policy))
        );
        let printed = |specs: BTreeMap<TodoId, Term>| -> BTreeMap<String, String> {
            specs
                .iter()
                .map(|(todo, spec)| (todo.as_str().to_owned(), print_term(spec)))
                .collect()
        };
        prop_assert_eq!(
            printed(specs_at(&here, t, &policy)),
            printed(specs_at(&there, moved, &policy))
        );
    }

    /// §6.6's monoid action on a log's chain of nodes, with no store in the
    /// way: `extend(extend(s, xs), ys) == extend(s, xs ++ ys)`, and extending
    /// with what is already folded changes nothing.
    #[test]
    fn extending_is_a_monoid_action(log in a_log(), split in 0usize..25) {
        let policy = roster();
        let nodes = chain_of(&log);
        let split = split.min(nodes.len());
        let whole = fold(&nodes, None, &policy);
        prop_assert_eq!(
            &extend(&fold(&nodes[..split], None, &policy), &nodes[split..], None, &policy),
            &whole
        );
        prop_assert_eq!(&extend(&whole, &nodes, None, &policy), &whole);
        prop_assert_eq!(
            since(&fold(&nodes[..split], None, &policy), &nodes),
            nodes[split..].iter().collect::<Vec<_>>()
        );
    }

    /// §9.10 — UNDER A POLICY THAT BINDS EVERYTHING THE TWO READINGS ARE ONE.
    ///
    /// [`Everything`]'s whole content is that no event claims, so the CLAIMED
    /// reading (§6.7, taken under it) and the CONFIRMED one (taken under the
    /// policy) coincide: every fold answers the same, no entry carries a claim,
    /// and no entry is provisional. It is the law that says the two readings
    /// are ONE reading asked twice and not two different pieces of machinery —
    /// and `Untrusted::none()`, the empty roster, is the same policy said with
    /// a roster, which is checked here too.
    #[test]
    fn the_two_readings_agree_under_a_policy_that_binds_everything(
        log in a_log(),
        at in 0i64..WINDOW,
    ) {
        let t = moment(at);
        prop_assert_eq!(folds(&log, t, &Everything), folds(&log, t, &Untrusted::none()));

        let nodes = chain_of(&log);
        for row in view::entries(&nodes, t, &Everything).expect("the log folds") {
            prop_assert_eq!(row.claim, None, "nothing is refused, so nothing is claimed");
            prop_assert_eq!(row.confidence, view::Confidence::Confirmed, "confidence");
        }
    }

    /// §9.11 — A CLAIMING EVENT NEVER MOVES THE CONFIRMED READING.
    ///
    /// Append one event the policy only lets CLAIM — a lifecycle write or a
    /// repricing from an actor on the roster, at any instant, about any todo —
    /// and every confirmed answer is the answer it was: the environment, the
    /// specs, the content, the functions and the whole history. That is what
    /// "stored and shown but not folded" MEANS, and it is the containment the
    /// roster is kept for.
    ///
    /// Asked at several instants, including instants before the claim, because
    /// a fold that let a claim through at one moment and not another would
    /// still pass at one moment.
    #[test]
    fn a_claiming_event_never_moves_the_confirmed_reading(
        log in a_log(),
        claim in a_claim(),
        when in 0i64..WINDOW,
        asked in prop::collection::vec(0i64..WINDOW, 1..5),
    ) {
        let policy = roster();
        let claim = claim.at(moment(when));
        prop_assert!(policy.standing(&claim).claims(), "the generator draws a claim");

        let mut extended = log.clone();
        extended.push(claim);
        for seconds in asked.into_iter().chain([when, WINDOW]) {
            let t = moment(seconds);
            prop_assert_eq!(folds(&extended, t, &policy), folds(&log, t, &policy));
        }
    }

    /// §9.12 — STANDING SELECTS EVENTS, NOT POSITIONS.
    ///
    /// [`Policy::standing`] is a function of the event alone: not of the log it
    /// sits in, not of where in the log it sits, not of the moment being asked
    /// about. So folding under a policy IS folding the sub-log of the events it
    /// binds — which is checked here against the ONE policy that binds
    /// everything — and that stays true under any permutation of the log,
    /// because filtering commutes with reordering. Which events count is a
    /// function of the event SET; only which of them WINS is a function of the
    /// order, and §9.3 is that half.
    #[test]
    fn standing_selects_events_and_not_positions(
        log in a_log(),
        keys in prop::collection::vec(0u32..1000, 0..25),
        at in 0i64..WINDOW,
    ) {
        let policy = roster();
        let t = moment(at);
        let mut order: Vec<usize> = (0..log.len()).collect();
        order.sort_by_key(|i| (keys.get(*i).copied().unwrap_or(0), *i));
        let shuffled: Vec<Event> = order.iter().map(|i| log[*i].clone()).collect();

        for log in [&log, &shuffled] {
            let kept: Vec<Event> = log
                .iter()
                .filter(|event| policy.standing(event).binds())
                .cloned()
                .collect();
            // `authored_at` takes no policy and reads EVERY record, so the
            // sub-log has to keep them; the reference policy binds them, which
            // is why this comparison is about the other four folds and this
            // assertion is what says so.
            prop_assert_eq!(
                kept.iter().filter(|e| matches!(e, TodoEvent::Authored(_))).count(),
                log.iter().filter(|e| matches!(e, TodoEvent::Authored(_))).count()
            );
            prop_assert_eq!(folds(&kept, t, &Everything), folds(log, t, &policy));
        }
    }
}

/// THE ONE PLACE A LATER EVENT REACHES BACK, named with its witnesses.
///
/// `flatten`'s head is `checklist(first spec ever, first checklist ever)`, and
/// a todo can have a function from EITHER half alone: a checklist with no spec
/// is `Conj(n × Flat(0.5))`, and a spec with no content record is the spec. So
/// the event that records the OTHER half for the first time re-heads the curve
/// at every earlier moment, once each way below.
///
/// The reference's generator draws exactly this — an `Authored`
/// carries a spec 85% of the time and subtodos some of the time, and a
/// `SpecRevised` prices a todo that has no content record at all — where
/// `tests/test_laws.py`'s generator never does (its authored records always
/// carry a spec and never a subtodo), which is why the reference's own prefix
/// law never meets it.
///
/// This is the REFERENCE's behaviour and not a porting error: every number and
/// print below was read off the reference's `events.py` on these exact
/// inputs. Pinning it keeps the exception a decision instead of a surprise.
#[test]
fn a_late_first_half_of_the_head_re_heads_the_curve() {
    let policy = roster();
    let start = moment(0);
    let tau = moment(1);
    let env = prodrome::fpl::Env::new();
    let now = fpl::instant_of(start);

    // A checklist with no spec, priced for the first time at tau.
    let checklist_only = mk_authored(
        "gamma",
        start,
        "bassel",
        "todo",
        start,
        "body",
        None,
        vec![],
        "",
        "",
        "",
        None,
        vec![
            mk_subtodo("item 0", true).expect("valid"),
            mk_subtodo("item 1", false).expect("valid"),
        ],
        vec![],
        "",
    )
    .expect("valid");
    let first_price = mk_authored(
        "gamma",
        tau,
        "bassel",
        "todo",
        tau,
        "body",
        Some(fpl::mk_flat(0.02).expect("in [0, 1]")),
        vec![],
        "",
        "",
        "",
        None,
        vec![],
        vec![],
        "",
    )
    .expect("valid");
    let gamma = TodoId::new("gamma").expect("valid");
    let before = flatten(std::slice::from_ref(&checklist_only), tau, &policy).expect("folds");
    let after = flatten(&[checklist_only, first_price], tau, &policy).expect("folds");
    assert_eq!(
        print_term(&before[&gamma]),
        "Conj(terms=(Flat(value=0.5), Flat(value=0.5)), p=-4.0)"
    );
    assert_eq!(
        print_term(&after[&gamma]),
        "Piecewise(head=OffsetBy(delta=Flat(value=0.02), \
         term=Conj(terms=(Flat(value=0.5), Flat(value=0.5)), p=-4.0)), \
         pieces=(Piece(at=datetime(2026, 9, 1, 0, 0, 1), term=Flat(value=0.02)),))"
    );
    assert_eq!(fpl::fulfillment(&before[&gamma], now, &env), 0.5);
    assert_eq!(fpl::fulfillment(&after[&gamma], now, &env), 0.51);

    // And the other way: a spec with no content record, given a checklist for
    // the first time at tau.
    let priced = mk_spec_revised(
        "beta",
        start,
        "bassel",
        fpl::mk_flat(0.5).expect("in [0, 1]"),
        "",
    )
    .expect("valid");
    let first_checklist = mk_authored(
        "beta",
        tau,
        "bassel",
        "todo",
        tau,
        "body",
        None,
        vec![],
        "",
        "",
        "",
        None,
        vec![
            mk_subtodo("item 0", true).expect("valid"),
            mk_subtodo("item 1", false).expect("valid"),
        ],
        vec![],
        "",
    )
    .expect("valid");
    let beta = TodoId::new("beta").expect("valid");
    let before = flatten(std::slice::from_ref(&priced), tau, &policy).expect("folds");
    let after = flatten(&[priced, first_checklist], tau, &policy).expect("folds");
    assert_eq!(print_term(&before[&beta]), "Flat(value=0.5)");
    assert_eq!(
        print_term(&after[&beta]),
        "OffsetBy(delta=Flat(value=0.5), \
         term=Conj(terms=(Flat(value=0.5), Flat(value=0.5)), p=-4.0))"
    );
    assert_eq!(fpl::fulfillment(&before[&beta], now, &env), 0.5);
    assert_eq!(fpl::fulfillment(&after[&beta], now, &env), 0.75);
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A scratch root that cleans itself up when the law is done with it.
struct Replicas(std::path::PathBuf);

impl Drop for Replicas {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Two replicas that diverged: a shared history, then concurrent writes on
/// each side, then an import. Built the only way a second head can appear.
fn diverged(shared: &[Event], mine: &[Event], theirs: &[Event]) -> (Replicas, Store) {
    let root = std::env::temp_dir().join(format!(
        "prodrome-laws-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    let guard = Replicas(root.clone());
    let here = Store::new(root.join("here"), roster());
    for event in shared {
        here.append(event.clone(), None).expect("appends");
    }
    let there = Store::new(root.join("there"), roster());
    if let Some(tip) = here.tip() {
        there.adopt(&here, &tip).expect("adopts");
    }
    for event in mine {
        here.append(event.clone(), None).expect("appends");
    }
    for event in theirs {
        there.append(event.clone(), None).expect("appends");
    }
    if let Some(tip) = there.tip() {
        here.adopt(&there, &tip).expect("adopts");
    }
    (guard, here)
}

fn nodes_from(store: &Store) -> Vec<Chain> {
    nodes_of(&store.read_dag_named().expect("the DAG reads"))
}

fn events_from(nodes: &[Chain]) -> Vec<Event> {
    nodes.iter().filter_map(|node| node.event.clone()).collect()
}

fn written(events: &[Event], policy: &Untrusted) -> BTreeSet<(Kind, TodoId)> {
    events
        .iter()
        .filter(|event| policy.standing(event).binds())
        .flat_map(|event| {
            prodrome::registers::writes_of(event)
                .iter()
                .map(|kind| (*kind, event.todo().clone()))
        })
        .collect()
}

fn conflicted(state: &State) -> BTreeSet<(Kind, TodoId)> {
    conflicts_of(state)
        .iter()
        .flat_map(|(todo, by_kind)| by_kind.keys().map(|kind| (*kind, todo.clone())))
        .collect()
}

proptest! {
    // Each case builds two stores on disk and imports one into the other, so
    // the budget buys graphs rather than repetitions.
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// §9.6, first clause — ON ANY DAG THE REGISTERS ARE THE FOLDS. Under
    /// conflict the projections pick the linearisation's last write, which is
    /// the write the event folds pick, so a conflicted todo never shows one
    /// write's content beside another write's price.
    #[test]
    fn the_registers_are_the_folds_on_any_dag(
        shared in a_schedule(1..8),
        mine in a_schedule(1..6),
        theirs in a_schedule(1..6),
    ) {
        let policy = roster();
        let mine: Vec<Event> = mine.iter().map(|(d, at)| d.at_with_note(moment(*at), "mine")).collect();
        let theirs: Vec<Event> =
            theirs.iter().map(|(d, at)| d.at_with_note(moment(*at), "theirs")).collect();
        let (_guard, store) = diverged(&realise(&shared, 0), &mine, &theirs);
        let nodes = nodes_from(&store);
        let events = events_from(&nodes);
        let state = fold(&nodes, None, &policy);

        prop_assert_eq!(env_of(&state), env_at(&events, far(), &policy));
        let printed = |specs: BTreeMap<TodoId, Term>| -> BTreeMap<String, String> {
            specs.iter().map(|(t, s)| (t.as_str().to_owned(), print_term(s))).collect()
        };
        prop_assert_eq!(printed(specs_of(&state)), printed(specs_at(&events, far(), &policy)));
        prop_assert_eq!(content_of(&state), authored_at(&events, far()));
    }

    /// §9.6, the rest — CONFLICTS ARE EXACTLY THE REGISTERS BOTH BRANCHES
    /// WROTE (the shared prefix's writes are ancestors of both and never
    /// conflict); A MERGE SETTLES NOTHING, because it carries no write; and a
    /// write that DESCENDS FROM BOTH settles every register it writes.
    #[test]
    fn conflicts_are_the_concurrent_writes_and_a_descending_write_settles(
        shared in a_schedule(1..8),
        mine in a_schedule(1..6),
        theirs in a_schedule(1..6),
    ) {
        let policy = roster();
        let mine: Vec<Event> = mine.iter().map(|(d, at)| d.at_with_note(moment(*at), "mine")).collect();
        let theirs: Vec<Event> =
            theirs.iter().map(|(d, at)| d.at_with_note(moment(*at), "theirs")).collect();
        let (_guard, store) = diverged(&realise(&shared, 0), &mine, &theirs);

        let expected: BTreeSet<(Kind, TodoId)> = written(&mine, &policy)
            .intersection(&written(&theirs, &policy))
            .cloned()
            .collect();
        prop_assert_eq!(conflicted(&fold(&nodes_from(&store), None, &policy)), expected.clone());

        if store.tips().len() > 1 {
            store.merge(None, None).expect("merges");
            prop_assert_eq!(
                conflicted(&fold(&nodes_from(&store), None, &policy)),
                expected.clone(),
                "a merge carries no write and settles nothing"
            );
            let settle = mk_completed("alpha", far(), "bassel", "settled").expect("valid");
            store.append(settle, None).expect("appends");
            let alpha = TodoId::new("alpha").expect("valid");
            let mut left = expected;
            left.remove(&(Kind::State, alpha));
            prop_assert_eq!(conflicted(&fold(&nodes_from(&store), None, &policy)), left);
        }
    }

    /// §9.9 — VIEW IS THE COMPOSITION IT NAMES (§6.7).
    ///
    /// Every field is recomputed here by the OTHER route. `view::entries`
    /// reads the confirmed outcome, the content and the conflicts off the
    /// REGISTERS, so those are checked against the event folds (`env_at`,
    /// `authored_at`); it reads the claim off `env_at`, so that one is checked
    /// against the REGISTERS under [`Everything`]. The two routes meet only
    /// through §9.6, so
    /// this is a second path to each answer and not the same code run twice.
    /// Under conflict the register projection picks the linearisation's last
    /// write, which is the write the event folds pick, so the routes agree on
    /// forked graphs too — which is why the DAG is a two-replica one.
    ///
    /// The moment is drawn INSIDE the generator's window, so entries are asked
    /// for while some events are still in the future: the row set is every
    /// todo the events mention, and `t` says only what is believed.
    #[test]
    fn the_entry_is_the_composition_of_the_folds_it_names(
        shared in a_schedule(1..8),
        mine in a_schedule(1..6),
        theirs in a_schedule(1..6),
        when in 0i64..WINDOW,
    ) {
        let policy = roster();
        let mine: Vec<Event> = mine.iter().map(|(d, at)| d.at_with_note(moment(*at), "mine")).collect();
        let theirs: Vec<Event> =
            theirs.iter().map(|(d, at)| d.at_with_note(moment(*at), "theirs")).collect();
        let (_guard, store) = diverged(&realise(&shared, 0), &mine, &theirs);
        let nodes = nodes_from(&store);
        let events = events_from(&nodes);
        let t = moment(when);

        let rows = view::entries(&nodes, t, &policy).expect("the DAG folds");

        // The ROWS: one per todo any event mentions, in id order — no filter
        // on `t`, none on standing.
        let mentioned: BTreeSet<&TodoId> = events.iter().map(TodoEvent::todo).collect();
        prop_assert_eq!(
            rows.iter().map(|row| &row.todo).collect::<Vec<_>>(),
            mentioned.into_iter().collect::<Vec<_>>()
        );

        let bound = env_at(&events, t, &policy);
        // The claim, by the route `entries` does NOT take: the registers under
        // the policy that binds everything.
        let claimed_side = env_of(&fold(&nodes, Some(t), &Everything));
        let specs = flatten(&events, t, &policy).expect("the DAG flattens");
        let records = authored_at(&events, t);
        let conflicts = conflicts_of(&fold(&nodes, Some(t), &policy));
        let env = prodrome::fold::evaluation_env(&bound);
        let now = fpl::instant_of(t);
        let carried: BTreeMap<&prodrome::event::Hash, &Event> = nodes
            .iter()
            .filter_map(|node| node.event.as_ref().map(|event| (&node.name, event)))
            .collect();

        for row in &rows {
            let todo = &row.todo;
            prop_assert_eq!(row.outcome, bound.get(todo).copied(), "outcome");

            // The CLAIM is the CLAIMED reading where the two name different
            // outcomes, and absent where they agree — the instants alone do
            // not disagree.
            let kinds = |b: Option<Binding>| b.map_or("open", Binding::kind);
            let disputed = kinds(claimed_side.get(todo).copied()) != kinds(bound.get(todo).copied());
            prop_assert_eq!(row.claim, if disputed { claimed_side.get(todo).copied() } else { None }, "claim");

            // The PRICE: §6.4's function, valued at `t` under the CONFIRMED
            // environment, and absent exactly where the function is.
            prop_assert_eq!(row.spec().map(print_term), specs.get(todo).map(print_term), "spec");
            prop_assert_eq!(
                row.value(),
                specs.get(todo).map(|spec| fpl::fulfillment(spec, now, &env)),
                "value"
            );

            // The CONTENT is a NAME, and the object it names carries the
            // record `authored_at` chose.
            let named = row.content.as_ref().map(|name| match carried[name] {
                TodoEvent::Authored(record) => (**record).clone(),
                other => panic!("the content register names {other:?}"),
            });
            prop_assert_eq!(named.as_ref(), records.get(todo), "content");

            let by_kind: BTreeMap<Kind, Vec<prodrome::event::Hash>> = conflicts
                .get(todo)
                .map(|held| {
                    held.iter()
                        .map(|(kind, frontier)| {
                            (*kind, frontier.writes().iter().map(|w| w.at.clone()).collect())
                        })
                        .collect()
                })
                .unwrap_or_default();
            prop_assert_eq!(&row.conflicts, &by_kind, "conflicts");

            // The CONFIDENCE is the two asymmetries and nothing else: a claim
            // refused, and a winning content record the policy does not
            // confirm.
            let provisional = disputed
                || row.content.as_ref().is_some_and(|name| !policy.confirms(carried[name]));
            prop_assert_eq!(row.confidence.is_provisional(), provisional, "confidence");

            // The STREAM is every object whose event names this todo, in the
            // DAG's own order.
            let stream: Vec<&prodrome::event::Hash> = nodes
                .iter()
                .filter(|node| node.event.as_ref().is_some_and(|e| e.todo() == todo))
                .map(|node| &node.name)
                .collect();
            prop_assert_eq!(row.stream.iter().collect::<Vec<_>>(), stream, "stream");
        }
    }
}
