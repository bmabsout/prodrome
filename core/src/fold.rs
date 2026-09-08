//! §6.1–6.5 — the causal folds: belief at a moment, as a query over the log.
//!
//! See ../../SPEC.md; implemented against ../../conformance/folds.json and
//! ported from the reference's `events.py`.
//!
//! ORDER IS CAUSAL, TIME IS DATA (§1). Every fold here reads
//! [`chronological`] — the events in the order the chain holds them, with the
//! ones dated after the moment dropped — and nothing else. No fold sorts by
//! `at`, because that would let a clock decide who wins; `at` is read as DATA
//! (a deadline, a completion's instant, the time machine's window) and the
//! writer stamped it.
//!
//! Each fold is a last-write-wins map keyed by todo. They differ in exactly
//! two ways, and those two are the whole of §5 and §6: WHICH kinds write, and
//! WHICH actors may. [`Untrusted`] is that second question as a type, so a
//! call site cannot inherit a trust policy it never considered.

use std::collections::{BTreeMap, BTreeSet};

use crate::event::{binds, Actor, Authored, TodoEvent, TodoId};
use crate::fpl::{self, FplError, Instant, Term};
use crate::literal::Datetime;

/// The actors whose lifecycle and repricing events are PROVISIONAL (§5):
/// stored, shown as claims, never folded. A newtype rather than a bare set
/// because it is a POLICY — the deployment's whole trust roster — and passing
/// one set of actor names where another was meant is the failure it prevents.
/// `Untrusted::none()` is a deployment that trusts every writer, said out loud.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Untrusted(BTreeSet<Actor>);

impl Untrusted {
    /// A deployment that trusts every writer.
    pub fn none() -> Untrusted {
        Untrusted(BTreeSet::new())
    }

    pub fn of(actors: impl IntoIterator<Item = Actor>) -> Untrusted {
        Untrusted(actors.into_iter().collect())
    }

    /// THE trust rule (§5), asked through the policy that holds it.
    pub fn binds(&self, event: &TodoEvent) -> bool {
        binds(event, &self.0)
    }

    pub fn actors(&self) -> &BTreeSet<Actor> {
        &self.0
    }
}

/// What history says about a todo: the two ways it can be over, which mean
/// OPPOSITE things downstream (a cancellation prices as moot, a completion
/// re-anchors a dependent's clock), so they are an ADT and never a boolean.
///
/// Keyed by [`TodoId`] here and by an event NAME in [`fpl::Env`] — §7's
/// `After` binds a free variable that today is always a todo id and need not
/// stay one, which is why the two are different types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    Completed(Instant),
    Cancelled(Instant),
}

impl Binding {
    pub fn at(self) -> Instant {
        match self {
            Binding::Completed(at) | Binding::Cancelled(at) => at,
        }
    }

    /// The constructor name the reference prints for this outcome.
    pub fn kind(self) -> &'static str {
        match self {
            Binding::Completed(_) => "Completed",
            Binding::Cancelled(_) => "Cancelled",
        }
    }

    fn outcome(self) -> fpl::Outcome {
        match self {
            Binding::Completed(at) => fpl::Outcome::Completed(at),
            Binding::Cancelled(at) => fpl::Outcome::Cancelled(at),
        }
    }
}

/// §6.1's answer: which todos are resolved, and when.
pub type Env = BTreeMap<TodoId, Binding>;

/// The environment as the EVALUATOR reads it (§7). One function rather than a
/// conversion written out at each consumer, because forgetting it would be a
/// type error in the evaluator rather than a wrong answer.
pub fn evaluation_env(env: &Env) -> fpl::Env {
    env.iter()
        .map(|(todo, binding)| (todo.as_str().to_owned(), binding.outcome()))
        .collect()
}

/// The events known by `t`, in CAUSAL order — THE ordering every fold below
/// shares, named once. `events` is the chain (or a prefix of it, or a DAG's
/// linearisation); this keeps that order and drops what is dated after `t`.
pub fn chronological(events: &[TodoEvent], t: Datetime) -> impl Iterator<Item = &TodoEvent> {
    events.iter().filter(move |event| event.at() <= t)
}

/// §6.1 — fold history into the environment as of `t`: last binding write
/// wins, `Reopened` clears, and only events that [`Untrusted::binds`] write at
/// all. `Created`, `SpecRevised` and `Authored` have no env effect.
///
/// This is what makes `fulfillment(term, t, env_at(…, t))` a time machine: the
/// belief at any past moment is a query over the log, never a stored snapshot.
pub fn env_at(events: &[TodoEvent], t: Datetime, untrusted: &Untrusted) -> Env {
    let mut env = Env::new();
    for event in chronological(events, t) {
        if !untrusted.binds(event) {
            continue;
        }
        match event {
            TodoEvent::Completed(e) => {
                env.insert(e.todo.clone(), Binding::Completed(fpl::instant_of(e.at)));
            }
            TodoEvent::Cancelled(e) => {
                env.insert(e.todo.clone(), Binding::Cancelled(fpl::instant_of(e.at)));
            }
            TodoEvent::Reopened(e) => {
                env.remove(&e.todo);
            }
            TodoEvent::Created(_) | TodoEvent::SpecRevised(_) | TodoEvent::Authored(_) => {}
        }
    }
    env
}

/// §6.2 — the chain's PRICING opinion as of `t`: the latest spec per todo.
///
/// An `Authored` record's spec counts from ANY actor, and that is a decision:
/// writing content is what the agent is for, and a todo authored without its
/// price is not a todo. `SpecRevised` stays trusted-only — it is the narrow
/// "reprice without rewriting" override, and an injected repricing is exactly
/// the threat §5 exists for.
///
/// ABSENCE IS NOT ZERO: a todo missing here means the chain has no opinion
/// about its price, not that it is worth nothing.
pub fn specs_at(
    events: &[TodoEvent],
    t: Datetime,
    untrusted: &Untrusted,
) -> BTreeMap<TodoId, Term> {
    let mut specs = BTreeMap::new();
    for event in chronological(events, t) {
        match event {
            TodoEvent::SpecRevised(e) if untrusted.binds(event) => {
                specs.insert(e.todo.clone(), e.spec.clone());
            }
            TodoEvent::Authored(e) => {
                if let Some(spec) = &e.spec {
                    specs.insert(e.todo.clone(), spec.clone());
                }
            }
            TodoEvent::Created(_)
            | TodoEvent::Completed(_)
            | TodoEvent::Cancelled(_)
            | TodoEvent::Reopened(_)
            | TodoEvent::SpecRevised(_) => {}
        }
    }
    specs
}

/// §6.3 — the chain's CONTENT as of `t`: the latest `Authored` per todo.
///
/// No trust parameter, deliberately, where the other two folds require one:
/// content from every actor renders. A fold that hid the agent's writes would
/// not be containment, it would be an outage that reports success; the reader
/// marks a provisional record instead.
pub fn authored_at(events: &[TodoEvent], t: Datetime) -> BTreeMap<TodoId, Authored> {
    let mut out = BTreeMap::new();
    for event in chronological(events, t) {
        if let TodoEvent::Authored(e) = event {
            out.insert(e.todo.clone(), (**e).clone());
        }
    }
    out
}

/// A chronological timeline's value at `m`: its last entry dated at or before
/// `m`, else `default`. The timeline is in CHAIN order and is never sorted —
/// that is the whole point of §6's ordering rule.
fn latest<T: Clone>(timeline: &[(Instant, T)], m: Instant, default: T) -> T {
    let mut value = default;
    for (at, entry) in timeline {
        if *at <= m {
            value = entry.clone();
        }
    }
    value
}

/// [`latest`] where the entries and the default have DIFFERENT types: every
/// spec write carries a spec, but the default — the first spec ever recorded —
/// may not exist at all, and a todo priced by its checklist alone is exactly
/// that case.
fn latest_spec(timeline: &[(Instant, Term)], m: Instant, default: Option<Term>) -> Option<Term> {
    let mut value = default;
    for (at, spec) in timeline {
        if *at <= m {
            value = Some(spec.clone());
        }
    }
    value
}

/// §6.5 — the environment as a FUNCTION OF TIME. `at(t)` equals
/// `env_at(events, t)` for every `t`, from ONE pass over the events.
///
/// `env_at` is the definition; this is what a consumer asking at many instants
/// reads — a curve — so that a dependency bound at τ is unbound at every
/// moment before τ and bound again exactly from it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct History {
    bindings: BTreeMap<TodoId, Vec<(Instant, Option<Binding>)>>,
}

impl History {
    pub fn at(&self, t: Datetime) -> Env {
        let m = fpl::instant_of(t);
        let mut env = Env::new();
        for (todo, timeline) in &self.bindings {
            if let Some(binding) = latest(timeline, m, None) {
                env.insert(todo.clone(), binding);
            }
        }
        env
    }

    /// Per todo, the sequence of (instant, binding-or-cleared) the chain
    /// records — what `at` reads, exposed because a consumer drawing a curve
    /// wants the transition instants and not one snapshot per pixel.
    pub fn bindings(&self) -> &BTreeMap<TodoId, Vec<(Instant, Option<Binding>)>> {
        &self.bindings
    }
}

/// Fold the whole log into a [`History`]. Same writers and the same trust rule
/// as [`env_at`]: `Completed`/`Cancelled` bind, `Reopened` clears, the rest
/// and every untrusted actor's lifecycle event do nothing.
pub fn history(events: &[TodoEvent], untrusted: &Untrusted) -> History {
    let mut bindings: BTreeMap<TodoId, Vec<(Instant, Option<Binding>)>> = BTreeMap::new();
    for event in events {
        if !untrusted.binds(event) {
            continue;
        }
        let (todo, at, binding) = match event {
            TodoEvent::Completed(e) => {
                let at = fpl::instant_of(e.at);
                (&e.todo, at, Some(Binding::Completed(at)))
            }
            TodoEvent::Cancelled(e) => {
                let at = fpl::instant_of(e.at);
                (&e.todo, at, Some(Binding::Cancelled(at)))
            }
            TodoEvent::Reopened(e) => (&e.todo, fpl::instant_of(e.at), None),
            TodoEvent::Created(_) | TodoEvent::SpecRevised(_) | TodoEvent::Authored(_) => continue,
        };
        bindings
            .entry(todo.clone())
            .or_default()
            .push((at, binding));
    }
    History { bindings }
}

/// §6.4 — fold each todo's history as of `t` into ONE fulfillment function.
///
/// The fold the consumers that show a NUMBER read: `env_at` says what
/// happened, `specs_at` what the todo is priced by today, `authored_at` what
/// it says — this says what it demanded at every instant. Every transition on
/// the chain is a piece:
///
/// - an `Authored` spec (any actor) or a trusted `SpecRevised` puts that spec
///   in force from its instant, and the curve before it is untouched;
/// - a trusted `Completed` or `Cancelled` puts a flat 1.0 in force, because
///   nothing is demanded of a resolved todo;
/// - a trusted `Reopened` puts the spec in force again.
///
/// The HEAD — the function before the first transition — is the first spec
/// ever recorded and the first checklist ever recorded, even when a resolution
/// precedes them in chain time (the backfill: completions dated 2025, records
/// dated the 2026 inversion). That is the only reading that does not invent a
/// demand. A todo with no spec and no checklist has no function and is absent,
/// resolved or not.
///
/// One consequence, stated because it is the only way a later event reaches
/// back: a todo can have a function from ONE half of the head — a checklist
/// with no spec, or a spec with no content record — and the event that records
/// the other half for the first time RE-HEADS its whole curve. It is the same
/// reading ("what the todo asked while open"), applied to a head that was
/// half-known; `tests/fold_laws.rs` pins it both ways, against the reference.
///
/// Returns a `Result` rather than panicking: every `mk_*` below is called with
/// arguments this function proves valid, but a fold over stored data answers
/// with a value, never with an abort.
pub fn flatten(
    events: &[TodoEvent],
    t: Datetime,
    untrusted: &Untrusted,
) -> Result<BTreeMap<TodoId, Term>, FplError> {
    let mut specs: BTreeMap<TodoId, Vec<(Instant, Term)>> = BTreeMap::new();
    // (at, checklist length) — CONTENT, so from any actor, like `authored_at`.
    let mut items: BTreeMap<TodoId, Vec<(Instant, usize)>> = BTreeMap::new();
    // (at, closed)
    let mut states: BTreeMap<TodoId, Vec<(Instant, bool)>> = BTreeMap::new();
    for event in chronological(events, t) {
        match event {
            TodoEvent::Authored(e) => {
                let at = fpl::instant_of(e.at);
                items
                    .entry(e.todo.clone())
                    .or_default()
                    .push((at, e.subtodos.len()));
                if let Some(spec) = &e.spec {
                    specs
                        .entry(e.todo.clone())
                        .or_default()
                        .push((at, spec.clone()));
                }
            }
            TodoEvent::SpecRevised(e) if untrusted.binds(event) => {
                specs
                    .entry(e.todo.clone())
                    .or_default()
                    .push((fpl::instant_of(e.at), e.spec.clone()));
            }
            TodoEvent::Completed(e) | TodoEvent::Cancelled(e) if untrusted.binds(event) => {
                states
                    .entry(e.todo.clone())
                    .or_default()
                    .push((fpl::instant_of(e.at), true));
            }
            TodoEvent::Reopened(e) if untrusted.binds(event) => {
                states
                    .entry(e.todo.clone())
                    .or_default()
                    .push((fpl::instant_of(e.at), false));
            }
            TodoEvent::Created(_)
            | TodoEvent::Completed(_)
            | TodoEvent::Cancelled(_)
            | TodoEvent::Reopened(_)
            | TodoEvent::SpecRevised(_) => {}
        }
    }
    let todos: BTreeSet<&TodoId> = specs.keys().chain(items.keys()).collect();
    let mut out = BTreeMap::new();
    for todo in todos {
        let empty_specs = Vec::new();
        let empty_items = Vec::new();
        let empty_states = Vec::new();
        let function = flatten_one(
            specs.get(todo).unwrap_or(&empty_specs),
            items.get(todo).unwrap_or(&empty_items),
            states.get(todo).unwrap_or(&empty_states),
        )?;
        if let Some(function) = function {
            out.insert(todo.clone(), function);
        }
    }
    Ok(out)
}

/// One todo's function: at every moment its spec, its checklist or its state
/// changed, the term in force is `fpl::checklist` of the spec in force and the
/// items in force — a flat 1.0 while resolved.
fn flatten_one(
    specs: &[(Instant, Term)],
    items: &[(Instant, usize)],
    states: &[(Instant, bool)],
) -> Result<Option<Term>, FplError> {
    let first_spec = specs.first().map(|(_, spec)| spec.clone());
    let first_items = items.first().map_or(0, |(_, n)| *n);
    let Some(head) = fpl::checklist(first_spec.clone(), first_items)? else {
        return Ok(None);
    };
    // A set, so the partition is the instants THEMSELVES and each appears
    // once; ordered, so `mk_piecewise`'s strictly-increasing precondition is
    // established by the type rather than checked afterwards.
    let moments: BTreeSet<Instant> = specs
        .iter()
        .map(|(at, _)| *at)
        .chain(items.iter().map(|(at, _)| *at))
        .chain(states.iter().map(|(at, _)| *at))
        .collect();
    let mut pieces = Vec::with_capacity(moments.len());
    for m in moments {
        if latest(states, m, false) {
            pieces.push((m, fpl::mk_flat(1.0)?));
            continue;
        }
        let term = fpl::checklist(
            latest_spec(specs, m, first_spec.clone()),
            latest(items, m, first_items),
        )?;
        pieces.push((m, term.unwrap_or_else(|| head.clone())));
    }
    // The constructor IS the normal form: unit, join, no adjacent repeats.
    fpl::mk_piecewise(head, pieces).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{mk_authored, mk_completed, mk_reopened, mk_subtodo};
    use crate::fpl::{fulfillment, mk_flat, print_term};

    fn at(day: u32) -> Datetime {
        Datetime::new(2026, 9, day, 12, 0, 0, 0).expect("a real instant")
    }

    fn authored(todo: &str, day: u32, actor: &str, spec: Option<Term>, items: usize) -> TodoEvent {
        let subtodos = (0..items)
            .map(|i| mk_subtodo(&format!("item {i}"), false).expect("valid"))
            .collect();
        mk_authored(
            todo,
            at(day),
            actor,
            "todo",
            at(day),
            "body",
            spec,
            vec![],
            "",
            "",
            "",
            None,
            subtodos,
            vec![],
            "",
        )
        .expect("valid")
    }

    fn trusted() -> Untrusted {
        Untrusted::of([Actor::new("triage").expect("valid")])
    }

    #[test]
    fn a_completion_binds_and_a_reopening_clears() {
        let log = vec![
            mk_completed("alpha", at(2), "bassel", "").expect("valid"),
            mk_reopened("alpha", at(4), "bassel", "").expect("valid"),
        ];
        let policy = trusted();
        assert_eq!(env_at(&log, at(3), &policy).len(), 1);
        assert!(env_at(&log, at(5), &policy).is_empty());
        // And the history reads the same at both moments, which is §9.4.
        let past = history(&log, &policy);
        assert_eq!(past.at(at(3)), env_at(&log, at(3), &policy));
        assert_eq!(past.at(at(5)), env_at(&log, at(5), &policy));
    }

    #[test]
    fn an_untrusted_completion_is_a_claim_and_not_a_binding() {
        let log = vec![mk_completed("alpha", at(2), "triage", "").expect("valid")];
        assert!(env_at(&log, at(3), &trusted()).is_empty());
        assert_eq!(env_at(&log, at(3), &Untrusted::none()).len(), 1);
    }

    #[test]
    fn a_resolved_todo_reads_one_from_the_instant_it_was_resolved() {
        let log = vec![
            authored("alpha", 1, "bassel", Some(mk_flat(0.2).expect("valid")), 0),
            mk_completed("alpha", at(3), "bassel", "").expect("valid"),
        ];
        let policy = trusted();
        let flat = flatten(&log, at(9), &policy).expect("folds");
        let term = &flat[&TodoId::new("alpha").expect("valid")];
        let env = evaluation_env(&env_at(&log, at(9), &policy));
        assert_eq!(fulfillment(term, fpl::instant_of(at(2)), &env), 0.2);
        assert_eq!(fulfillment(term, fpl::instant_of(at(4)), &env), 1.0);
    }

    #[test]
    fn a_checklist_is_the_parents_offset_on_its_items() {
        let log = vec![authored(
            "alpha",
            1,
            "bassel",
            Some(mk_flat(0.5).expect("valid")),
            2,
        )];
        let flat = flatten(&log, at(9), &trusted()).expect("folds");
        let term = &flat[&TodoId::new("alpha").expect("valid")];
        assert_eq!(
            print_term(term),
            "OffsetBy(delta=Flat(value=0.5), term=Conj(terms=(Flat(value=0.5), Flat(value=0.5)), p=-4.0))"
        );
    }

    #[test]
    fn a_todo_with_no_spec_and_no_checklist_has_no_function() {
        let log = vec![
            authored("alpha", 1, "bassel", None, 0),
            mk_completed("alpha", at(3), "bassel", "").expect("valid"),
        ];
        assert!(flatten(&log, at(9), &trusted()).expect("folds").is_empty());
    }
}
