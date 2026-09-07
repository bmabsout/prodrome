//! §6.7 — the entry: one todo as the folds see it at a moment.
//!
//! See ../../SPEC.md; the composition this module names is the one
//! `suzatary/view.py::entries_of` performed while the application was Python,
//! and `../../conformance/view.json` is that reference's answers.
//!
//! THIS IS NOT A FIFTH FOLD. Every number and every name below comes out of
//! §6's folds and §7's evaluator, called in one order and written down:
//!
//! | field       | where it comes from                                        |
//! | ----------- | ---------------------------------------------------------- |
//! | `outcome`   | `env_of(registers::fold(nodes, t, untrusted))`              |
//! | `claim`     | `fold::env_at(events, t, ∅)`, where it DISAGREES            |
//! | `spec`      | `fold::flatten(events, t, untrusted)`                       |
//! | `value`     | `fpl::fulfillment(spec, t, evaluation_env(outcomes))`       |
//! | `content`   | `registers::chosen_of(confirmed, Kind::Content)`            |
//! | `conflicts` | `registers::conflicts_of(confirmed)`                        |
//! | `stream`    | the nodes whose event names this todo, in causal order      |
//!
//! It lives in the Prodrome because every word of it is the Prodrome's own —
//! there is no notion here of a page, a briefing or a phone — and because the
//! server and the browser's offline fold must be the SAME reading of it
//! (`docs/ARCHITECTURE.md` §4). A composition written twice is two
//! compositions, and two compositions drift; the law in §9 says this one
//! equals the folds it names, on random DAGs.
//!
//! WHY TWO ENVIRONMENTS. The CONFIRMED one is what prices and what a reader is
//! told; the LOOSE one trusts every writer and exists only to name what an
//! untrusted actor has claimed and has not bound. The gap between them is §5's
//! containment asymmetry, made into a value ([`Entry::claim`]) rather than
//! left implicit — displayed, never applied.
//!
//! ABSENCE. Three of them, and each is a type rather than a sentinel:
//!
//! - no spec and no checklist means NO FUNCTION (§6.4), so no value either —
//!   which is why [`Priced`] is one field and not two `Option`s that could
//!   disagree. Absence is not zero: a todo the chain has no price for is not
//!   a todo worth nothing.
//! - no binding means OPEN, which is an absent `outcome` and not a third
//!   constructor beside `Completed` and `Cancelled`.
//! - no content record means the chain holds events about this todo and no
//!   `Authored` among them, which happens and is not an error.
//!
//! AN ENTRY CARRIES A NAME, NOT A RECORD. `content` is the [`Hash`] of the
//! object whose write the content register shows; a consumer holding the
//! objects looks it up (ARCHITECTURE §4). The Prodrome does not know what a
//! body is for, and a whole todo body reprinted per entry per request was
//! measured at 1.7 ms of a 12 ms fold (`registers::chosen_of`).
//!
//! EVERY TODO EVER MENTIONED, INCLUDING THE FUTURE'S. The rows are one per
//! todo any event in `nodes` names — not one per todo known by `t`. An event
//! dated after `t` still puts its todo on the list, open and unpriced, and its
//! object still appears in that todo's `stream`. That is the reference's
//! reading and it is the honest one: the stream is the chain's record of a
//! todo, and `t` is the moment BELIEF is asked about, not a filter on what
//! exists.

use std::collections::BTreeMap;

use crate::event::{Hash, TodoEvent, TodoId};
use crate::fold::{self, Binding, Untrusted};
use crate::fpl::{self, Term};
use crate::literal::{Datetime, ProdromeError};
use crate::registers::{self, Kind, Node};

/// A todo's price at a moment: §6.4's function and that function's value
/// there.
///
/// ONE value and not two fields, because there is no spec without a value and
/// no value without a spec — §6.4's absence rule ("a todo with no spec and no
/// checklist has no function") stated as a type instead of as two `Option`s a
/// reader would have to be told agree.
#[derive(Debug, Clone, PartialEq)]
pub struct Priced {
    /// The todo's fulfillment FUNCTION over all of time — its revisions and
    /// its lifecycle as pieces of one term. Not the authored spec of the
    /// moment; `specs_at` is that.
    pub spec: Term,
    /// `spec` at the moment the entry was taken, in [0, 1].
    pub value: f64,
}

/// WHY a reader is being shown something the trusted fold did not bind.
///
/// Never "no reason": [`Standing::Confirmed`] is that case, so a `Provisional`
/// that means nothing is not a value that exists. The two reasons are
/// independent — a claimed state and an untrusted content record are different
/// asymmetries with different remedies — so the sum names all three
/// inhabitants rather than carrying two booleans that can both be false.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provisional {
    /// An untrusted actor's lifecycle event binds a state the trusted fold
    /// refuses. [`Entry::claim`] is that state.
    Claimed,
    /// The winning content record was written by an untrusted actor, so it is
    /// shown as source and never rendered ("rendered means confirmed").
    Content,
    /// Both at once.
    ClaimedAndContent,
}

/// Whether the chain's answer for this todo is the trusted fold's, whole.
///
/// A SUM and not a bool beside a string (ARCHITECTURE §3): a consumer that
/// wants one flag asks [`Standing::is_provisional`], and a consumer that wants
/// to say WHICH asymmetry it is looking at can, without a second field to keep
/// in step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// The two folds agree and the winning content record is a trusted
    /// actor's.
    Confirmed,
    Provisional(Provisional),
}

impl Standing {
    /// The standing implied by the two asymmetries. Total: neither of them is
    /// [`Standing::Confirmed`], which is why `Provisional` has no fourth arm.
    pub fn of(claimed: bool, content: bool) -> Standing {
        match (claimed, content) {
            (false, false) => Standing::Confirmed,
            (true, false) => Standing::Provisional(Provisional::Claimed),
            (false, true) => Standing::Provisional(Provisional::Content),
            (true, true) => Standing::Provisional(Provisional::ClaimedAndContent),
        }
    }

    /// The one flag a wire carries — `view.Entry.unconfirmed`.
    pub fn is_provisional(self) -> bool {
        matches!(self, Standing::Provisional(_))
    }
}

/// One todo, as the folds see it at a moment.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub todo: TodoId,
    /// The CONFIRMED binding; `None` is open.
    pub outcome: Option<Binding>,
    /// What every writer's events would bind and the trusted fold refused —
    /// `None` when the two agree. It is DISPLAYED, never applied.
    ///
    /// Two bindings of the same KIND at different instants do not disagree:
    /// what an untrusted actor can claim is that a todo is over, and an
    /// instant on a binding the trusted fold already made is not a claim about
    /// anything a reader acts on. That is the reference's rule
    /// (`view._entry`'s `disputed`, which compares the two outcomes' kinds).
    pub claim: Option<Binding>,
    pub standing: Standing,
    /// §6.4's function and its value here, absent together (see [`Priced`]).
    pub priced: Option<Priced>,
    /// The NAME of the object whose write the content register shows.
    pub content: Option<Hash>,
    /// The registers of this todo with more than one live write — a DAG's
    /// concurrent writes that no later write has settled — each named by the
    /// object that made it. Empty on a chain. The projections chose one of
    /// them (the linearisation's last) to price and to show; this is that
    /// choice made visible, so a human settles it with the next write.
    pub conflicts: BTreeMap<Kind, Vec<Hash>>,
    /// Every object whose event names this todo, in causal order. Merges carry
    /// no event and are not on any todo's timeline.
    pub stream: Vec<Hash>,
}

impl Entry {
    pub fn spec(&self) -> Option<&Term> {
        self.priced.as_ref().map(|priced| &priced.spec)
    }

    pub fn value(&self) -> Option<f64> {
        self.priced.as_ref().map(|priced| priced.value)
    }

    /// The lifecycle as the WIRE spells it: the outcome's constructor name
    /// lowercased, `"open"` for no binding.
    ///
    /// A rendering, in the same category as [`fpl::iso`] and `print_term` and
    /// for the same reason: the alternative is each binding rendering it, and
    /// two spellings of one string is exactly the drift this module exists to
    /// remove. It decides nothing — [`Entry::outcome`] is the value.
    pub fn state(&self) -> &'static str {
        lowered(self.outcome)
    }

    /// The confirmed binding's instant as CPython's `isoformat(" ")` — the
    /// spelling `view.json_entry` shipped — and `""` for an open todo, which
    /// is absence and not an instant.
    pub fn at(&self) -> String {
        self.outcome.map_or_else(String::new, |binding| {
            fpl::iso(binding.at()).replace('T', " ")
        })
    }

    /// The CLAIMED state, `""` where the two folds agree.
    pub fn claimed(&self) -> &'static str {
        match self.claim {
            None => "",
            claimed => lowered(claimed),
        }
    }
}

/// `state`'s and `claimed`'s shared spelling: `"open"`, `"completed"`,
/// `"cancelled"`.
fn lowered(binding: Option<Binding>) -> &'static str {
    match binding {
        None => "open",
        Some(Binding::Completed(_)) => "completed",
        Some(Binding::Cancelled(_)) => "cancelled",
    }
}

/// §6.7 — every todo the DAG has ever mentioned, as the folds see it at `t`.
///
/// `nodes` is the DAG in the linearisation's order (parents first), which is
/// what [`registers::nodes_of`] hands over and what every fold here reads;
/// `untrusted` is §5's whole policy. The rows are sorted by todo id.
///
/// The composition, spelled once so the law can say "this equals that":
///
/// 1. `confirmed = registers::fold(nodes, Some(t), untrusted)`;
/// 2. `outcome = env_of(confirmed)[todo]`;
///    `claim = env_at(events, t, ∅)[todo]` where the two outcomes name
///    different kinds;
/// 3. `spec = flatten(events of nodes, t, untrusted)[todo]`, and `value` is
///    `fulfillment(spec, t, evaluation_env(env_of(confirmed)))` — the
///    CONFIRMED environment, because a claim does not price;
/// 4. `content = chosen_of(confirmed, Kind::Content)[todo]`,
///    `conflicts = conflicts_of(confirmed)[todo]`;
/// 5. `standing` is `Standing::of(claim.is_some(), the content record's actor
///    is untrusted)`.
///
/// ONE REGISTER FOLD, NOT TWO. The loose side is asked exactly one question —
/// what would every writer's events bind — and that question is §6.1's, which
/// [`fold::env_at`] answers from the linearisation. §9.6 says the two agree on
/// any DAG (`env_of(fold(nodes, …)) == env_at(events, …)`), so this is the
/// same value; what it is not is the ancestry bitsets and the frontier
/// arithmetic, which exist to make CONFLICTS visible, and a claim's conflicts
/// are not shown. The confirmed side keeps the registers because it IS asked
/// for conflicts, and for the object each content register chose. Measured on
/// the live chain (565 objects, 2026-09-07): 1.5 ms against 0.3 ms.
///
/// Returns a `Result` because [`fold::flatten`] does: a fold over stored data
/// answers with a value, never with an abort.
pub fn entries(
    nodes: &[Node],
    t: Datetime,
    untrusted: &Untrusted,
) -> Result<Vec<Entry>, ProdromeError> {
    let confirmed = registers::fold(nodes, Some(t), untrusted);
    let outcomes = registers::env_of(&confirmed);
    let content = registers::chosen_of(&confirmed, Kind::Content);
    let mut conflicts = registers::conflicts_of(&confirmed);

    let events: Vec<TodoEvent> = nodes.iter().filter_map(|node| node.event.clone()).collect();
    let claims = fold::env_at(&events, t, &Untrusted::none());
    let specs = fold::flatten(&events, t, untrusted)?;
    // ONE conversion of the environment for the whole list: §7's `Env` is
    // keyed by event NAME and the fold's by `TodoId`, and converting per todo
    // would rebuild it once per row.
    let env = fold::evaluation_env(&outcomes);
    let now = fpl::instant_of(t);

    // Which objects wrote about which todo, in the order the DAG handed them
    // over — and, with it, WHICH TODOS THERE ARE. No filter on `t` and none on
    // trust: a todo exists on this list because the chain mentions it.
    let mut streams: BTreeMap<TodoId, Vec<Hash>> = BTreeMap::new();
    for node in nodes {
        if let Some(event) = &node.event {
            streams
                .entry(event.todo().clone())
                .or_default()
                .push(node.name.clone());
        }
    }

    // The untrusted actors' content records, by todo — the second half of
    // `standing`. Read off the events the nodes carry rather than off a
    // re-lookup, because `content` names an object and this is what that
    // object said.
    let mut untrusted_content: BTreeMap<&Hash, bool> = BTreeMap::new();
    for node in nodes {
        if let Some(TodoEvent::Authored(record)) = &node.event {
            untrusted_content.insert(&node.name, untrusted.actors().contains(&record.actor));
        }
    }

    let mut out = Vec::with_capacity(streams.len());
    for (todo, stream) in streams {
        let outcome = outcomes.get(&todo).copied();
        let claimed = claims.get(&todo).copied();
        let disputed = kind_of(claimed) != kind_of(outcome);
        let written = content.get(&todo).cloned();
        let provisional_content = written
            .as_ref()
            .and_then(|name| untrusted_content.get(name))
            .copied()
            .unwrap_or(false);
        let priced = specs.get(&todo).map(|spec| Priced {
            value: fpl::fulfillment(spec, now, &env),
            spec: spec.clone(),
        });
        out.push(Entry {
            claim: if disputed { claimed } else { None },
            standing: Standing::of(disputed, provisional_content),
            outcome,
            priced,
            content: written,
            conflicts: conflicts
                .remove(&todo)
                .map(|by_kind| {
                    by_kind
                        .into_iter()
                        .map(|(kind, frontier)| {
                            (
                                kind,
                                frontier
                                    .writes()
                                    .iter()
                                    .map(|write| write.at.clone())
                                    .collect(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            stream,
            todo,
        });
    }
    Ok(out)
}

/// The outcome's constructor name, or `"open"` for no binding — the three
/// values `disputed` compares. `None` is a value here and not a missing one:
/// an untrusted `Reopened` that the trusted fold refuses is a claim that the
/// todo is OPEN, and comparing `Option`s would have made that the same as
/// having nothing to say.
fn kind_of(binding: Option<Binding>) -> &'static str {
    binding.map_or("open", Binding::kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{mk_authored, mk_completed, mk_sealed, seal_hash, Actor};
    use crate::fpl::mk_flat;

    fn at(day: u32) -> Datetime {
        Datetime::new(2026, 9, day, 12, 0, 0, 0).expect("a real instant")
    }

    fn chain(events: Vec<TodoEvent>) -> Vec<Node> {
        let mut prev = None;
        let mut nodes = Vec::new();
        for event in events {
            let envelope = mk_sealed(prev, event);
            let name = seal_hash(&envelope);
            prev = Some(name.clone());
            nodes.push(Node::of(name, &envelope));
        }
        nodes
    }

    fn authored(todo: &str, day: u32, actor: &str, spec: Option<Term>) -> TodoEvent {
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
            vec![],
            vec![],
            "",
        )
        .expect("valid")
    }

    fn trusted() -> Untrusted {
        Untrusted::of([Actor::new("triage").expect("valid")])
    }

    #[test]
    fn an_untrusted_completion_is_a_claim_and_the_entry_is_provisional() {
        let nodes = chain(vec![
            authored("alpha", 1, "bassel", Some(mk_flat(0.25).expect("valid"))),
            mk_completed("alpha", at(3), "triage", "").expect("valid"),
        ]);
        let entries = entries(&nodes, at(9), &trusted()).expect("folds");
        let [entry] = &entries[..] else {
            panic!("one todo")
        };
        assert_eq!(entry.outcome, None, "the trusted fold refuses the claim");
        assert!(matches!(entry.claim, Some(Binding::Completed(_))));
        assert_eq!(
            entry.standing,
            Standing::Provisional(Provisional::Claimed),
            "the claim alone, since bassel wrote the content"
        );
        assert_eq!(entry.value(), Some(0.25), "an open todo prices by its spec");
        assert_eq!(entry.stream.len(), 2);
    }

    #[test]
    fn an_untrusted_record_makes_the_entry_provisional_without_a_claim() {
        let nodes = chain(vec![authored("alpha", 1, "triage", None)]);
        let entries = entries(&nodes, at(9), &trusted()).expect("folds");
        let [entry] = &entries[..] else {
            panic!("one todo")
        };
        assert_eq!(entry.claim, None);
        assert_eq!(entry.standing, Standing::Provisional(Provisional::Content));
        assert_eq!(
            entry.content.as_ref(),
            Some(&nodes[0].name),
            "the name of the winning record, not the record"
        );
        assert_eq!(
            entry.priced, None,
            "no spec and no checklist is no function, so no value"
        );
    }

    #[test]
    fn a_todo_whose_only_event_is_dated_after_the_moment_is_still_a_row() {
        let nodes = chain(vec![authored(
            "alpha",
            9,
            "bassel",
            Some(mk_flat(0.5).expect("valid")),
        )]);
        let entries = entries(&nodes, at(1), &trusted()).expect("folds");
        let [entry] = &entries[..] else {
            panic!("one todo")
        };
        assert_eq!(entry.standing, Standing::Confirmed);
        assert_eq!(entry.outcome, None);
        assert_eq!(entry.priced, None, "the chain knows no price yet");
        assert_eq!(entry.content, None, "and no record yet");
        assert_eq!(entry.stream.len(), 1, "the object is still its stream");
    }

    #[test]
    fn a_resolved_todo_reads_one_and_carries_its_instant() {
        let nodes = chain(vec![
            authored("alpha", 1, "bassel", Some(mk_flat(0.2).expect("valid"))),
            mk_completed("alpha", at(3), "bassel", "").expect("valid"),
        ]);
        let entries = entries(&nodes, at(9), &trusted()).expect("folds");
        let [entry] = &entries[..] else {
            panic!("one todo")
        };
        assert_eq!(
            entry.outcome,
            Some(Binding::Completed(fpl::instant_of(at(3))))
        );
        assert_eq!(entry.claim, None, "the two folds agree");
        assert_eq!(entry.standing, Standing::Confirmed);
        assert_eq!(entry.value(), Some(1.0));
    }

    #[test]
    fn the_standing_of_no_asymmetry_is_confirmed() {
        assert_eq!(Standing::of(false, false), Standing::Confirmed);
        assert!(!Standing::of(false, false).is_provisional());
        assert!(Standing::of(true, true).is_provisional());
        assert_eq!(
            Standing::of(true, true),
            Standing::Provisional(Provisional::ClaimedAndContent)
        );
    }
}
