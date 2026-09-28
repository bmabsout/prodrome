//! §6.7 — the entry: one todo as the folds see it at a moment.
//!
//! See ../../SPEC.md and Draft A. Not a fifth fold: every field is a
//! projection of the todo's [`Registers`] or §7's evaluator over them, composed
//! once here so every consumer performs it once.
//!
//! TWO READINGS. The CONFIRMED one, under the host's [`Policy`], prices and is
//! what a reader is told; the CLAIMED one, under [`Everything`], exists only to
//! name what a writer claimed and the policy did not bind ([`Entry::claim`]).
//!
//! A CONFLICT IS ITS CANDIDATES. Nothing picks a winner: the outcome and the
//! content are candidate sets, and the function is `Least` over the worlds, so
//! a conflict prices as its most urgent candidate.
//!
//! EVERY TODO EVER MENTIONED. A row per todo any node names, whatever `t` is:
//! `t` asks what is believed, not what exists.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::event::{Hash, TodoId};
use crate::fold::{self, Kind, Registers};
use crate::fpl::{self, Candidates, Env, LinkError, Outcome};
use crate::literal::{Datetime, ProdromeError};
use crate::payload::Payload;
use crate::policy::{Everything, Policy};
use crate::registers::{self, Genesis, Node};
use crate::term::Term;

/// A todo's price at a moment: §6.4's function and that function's value
/// there. Every row has one: a todo with no function is priced by `Absent`
/// (§7), which reads `∅`.
#[derive(Debug, Clone, PartialEq)]
pub struct Price {
    /// The todo's fulfillment FUNCTION over all of time — its revisions and
    /// its lifecycle as pieces of one term. Not the authored spec of the
    /// moment; `fold::specs` is that. Open: its `Ref`s are other todos'. `Absent`
    /// where the todo has no spec and no checklist.
    pub spec: Term,
    /// `spec`, linked against every todo's function, at the moment the entry
    /// was taken: a number in [0, 1], `∅` (`None`) where it has none, or why
    /// it does not link. A function whose reference is unknown or loops has
    /// no value, and says so.
    pub value: Result<Option<f64>, LinkError>,
    /// `spec`, linked: what a consumer explains, samples or compiles, so no
    /// reader links a second time with a different set of specs.
    pub linked: Result<fpl::Closed, LinkError>,
}

/// WHY a reader is being shown something the confirmed reading did not have.
///
/// Never "no reason": [`Confidence::Confirmed`] is that case, so a `Provisional`
/// that means nothing is not a value that exists. The two reasons are
/// independent — a claimed state and a content record the policy does not
/// confirm are different asymmetries with different remedies — so the sum
/// names all three
/// inhabitants rather than carrying two booleans that can both be false.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provisional {
    /// A CLAIMING lifecycle event names a state the confirmed reading refuses.
    /// [`Entry::claim`] is that state.
    Claimed,
    /// The winning content record is one the policy does not
    /// [`Policy::confirms`] — it BINDS, and it is still its writer's — so it is
    /// shown as source and never rendered ("rendered means confirmed").
    Content,
    /// Both at once.
    ClaimedAndContent,
}

/// Whether the chain's answer for this todo is the confirmed reading, whole.
///
/// A SUM and not a bool beside a string: a consumer that
/// wants one flag asks [`Confidence::is_provisional`], and a consumer that wants
/// to say WHICH asymmetry it is looking at can, without a second field to keep
/// in step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// The two readings agree and the policy confirms the winning content
    /// record.
    Confirmed,
    Provisional(Provisional),
}

impl Confidence {
    /// The confidence implied by the two asymmetries. Total: neither of them is
    /// [`Confidence::Confirmed`], which is why `Provisional` has no fourth arm.
    pub fn of(claimed: bool, content: bool) -> Confidence {
        match (claimed, content) {
            (false, false) => Confidence::Confirmed,
            (true, false) => Confidence::Provisional(Provisional::Claimed),
            (false, true) => Confidence::Provisional(Provisional::Content),
            (true, true) => Confidence::Provisional(Provisional::ClaimedAndContent),
        }
    }

    /// The one flag a wire carries — `view.Entry.unconfirmed`.
    pub fn is_provisional(self) -> bool {
        matches!(self, Confidence::Provisional(_))
    }
}

/// One todo, as the folds see it at a moment.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub genesis: Genesis,
    pub todo: TodoId,
    /// The CONFIRMED candidate bindings, `None` for open: one member unless
    /// the state is in conflict.
    pub outcome: Candidates,
    /// The CLAIMED candidates, where their kinds differ from the confirmed
    /// ones. DISPLAYED, never applied.
    pub claim: Option<Candidates>,
    pub confidence: Confidence,
    /// §6.4's function and its value here (see [`Price`]).
    pub price: Price,
    /// The NAMES of the candidate content records, sorted.
    pub content: Vec<Hash>,
    /// The registers of this todo with more than one live write, each named by
    /// the writes a human settles with the next one.
    pub conflicts: BTreeMap<Kind, Vec<Hash>>,
    /// Every object whose event names this todo, in causal order.
    pub stream: Vec<Hash>,
}

impl Entry {
    pub fn spec(&self) -> &Term {
        &self.price.spec
    }

    /// A number, `∅` (`Ok(None)`), or why the function does not link.
    pub fn value(&self) -> Result<Option<f64>, &LinkError> {
        self.price.value.as_ref().copied()
    }

    /// The function, linked against every todo's, when it links: what to
    /// explain, sample or compile.
    pub fn linked(&self) -> Option<&fpl::Closed> {
        self.price.linked.as_ref().ok()
    }

    /// Why the function has no value, where it does not link.
    pub fn unlinked(&self) -> Option<&LinkError> {
        self.price.value.as_ref().err()
    }

    pub fn is_open(&self) -> bool {
        self.outcome == Candidates::from([None])
    }

    /// The lifecycle as the WIRE spells it: each candidate's constructor name
    /// lowercased, `"open"` for none, joined by `|` under a conflict. A
    /// rendering; [`Entry::outcome`] is the value.
    pub fn state(&self) -> String {
        lowered(&self.outcome)
    }

    /// The bound candidates' instants as CPython's `isoformat(" ")`, `""` for
    /// an open todo.
    pub fn at(&self) -> String {
        let instants: Vec<String> = self
            .outcome
            .iter()
            .flatten()
            .map(|binding| fpl::iso(binding.at()).replace('T', " "))
            .collect();
        instants.join("|")
    }

    /// The CLAIMED state, `""` where the two readings agree and, as the wire
    /// always spelt it, where the claim is only that the todo is open.
    pub fn claimed(&self) -> String {
        match &self.claim {
            Some(claim) if *claim != Candidates::from([None]) => lowered(claim),
            _ => String::new(),
        }
    }
}

/// §6.7's LIST ORDER, stated once so no host decides it: most urgent first —
/// ascending in value, ties by (genesis, id) — then every row with no number,
/// `absent` or not linking, by (genesis, id).
pub fn list_order(a: &Entry, b: &Entry) -> Ordering {
    let number = |entry: &Entry| entry.value().ok().flatten();
    match (number(a), number(b)) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
    .then_with(|| (&a.genesis, &a.todo).cmp(&(&b.genesis, &b.todo)))
}

/// `state`'s and `claimed`'s shared spelling.
fn lowered(candidates: &Candidates) -> String {
    let names: Vec<&str> = candidates
        .iter()
        .map(|binding| match binding {
            None => "open",
            Some(Outcome::Completed(_)) => "completed",
            Some(Outcome::Cancelled(_)) => "cancelled",
        })
        .collect();
    names.join("|")
}

/// §6.7 — every todo the DAG has ever mentioned, as the folds see it at `t`,
/// per genesis and by todo id.
///
/// Per todo: `outcome` is the confirmed state's candidates; `claim` the
/// claimed ones where their kinds differ; `spec` is `flatten`'s function,
/// `Absent` where none, and `value` its fulfillment at `t`, linked against its
/// prodrome's functions under the CONFIRMED environment; `content` the
/// candidate records' names; `conflicts` every register with two writes; and
/// `confidence` whether a claim was refused or a candidate record is one the
/// policy does not confirm.
pub fn entries<P: Payload>(
    nodes: &[Node<P>],
    t: Datetime,
    policy: &impl Policy<P>,
) -> Result<Vec<Entry>, ProdromeError> {
    let state = registers::fold(nodes);
    let now = fpl::instant_of(t);
    let mut out = Vec::new();
    for (genesis, prodrome) in state.prodromes() {
        let confirmed: Vec<(&TodoId, Registers<P>)> = prodrome
            .iter()
            .map(|(todo, stream)| (todo, Registers::read(stream, Some(now), policy)))
            .collect();
        let mut env = Env::new();
        for (todo, registers) in &confirmed {
            registers.bind(todo, &mut env);
        }
        let functions = fold::flatten(prodrome, now, policy)?;
        let linkable = fold::link_specs(&functions, prodrome.keys());
        for (todo, registers) in confirmed {
            let stream = &prodrome[todo];
            let outcome = registers.outcomes();
            let claimed = Registers::read(stream, Some(now), &Everything).outcomes();
            let disputed = kinds(&claimed) != kinds(&outcome);
            let provisional_content = registers
                .content
                .candidates()
                .iter()
                .any(|stamp| !policy.confirms(&stamp.event));
            let spec = functions.get(todo).cloned().unwrap_or_else(fpl::mk_absent);
            let linked = fpl::link(&spec, &linkable);
            out.push(Entry {
                genesis: genesis.clone(),
                todo: todo.clone(),
                claim: disputed.then_some(claimed),
                confidence: Confidence::of(disputed, provisional_content),
                outcome,
                price: Price {
                    value: linked
                        .as_ref()
                        .map(|closed| fpl::fulfillment(closed, now, &env))
                        .map_err(Clone::clone),
                    linked,
                    spec,
                },
                content: registers
                    .content
                    .candidates()
                    .iter()
                    .map(|stamp| stamp.name.clone())
                    .collect(),
                conflicts: registers.conflicts(),
                stream: stream.iter().map(|stamp| stamp.name.clone()).collect(),
            });
        }
    }
    Ok(out)
}

/// The candidates' constructor names, `"open"` for none: the instants alone
/// do not dispute a binding.
fn kinds(candidates: &Candidates) -> BTreeSet<&'static str> {
    candidates
        .iter()
        .map(|binding| binding.as_ref().map_or("open", Outcome::kind))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{mk_completed, mk_sealed, seal_hash, Actor, TodoEvent};
    use crate::fpl::mk_flat;
    use crate::policy::Untrusted;
    use crate::reference::{mk_authored, Todo};

    type Event = TodoEvent<Todo>;

    fn at(day: u32) -> Datetime {
        Datetime::new(2026, 9, day, 12, 0, 0, 0).expect("a real instant")
    }

    fn chain(events: Vec<Event>) -> Vec<Node<Todo>> {
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

    fn authored(todo: &str, day: u32, actor: &str, spec: Option<Term>) -> Event {
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

    fn roster() -> Untrusted {
        Untrusted::of([Actor::new("triage").expect("valid")])
    }

    #[test]
    fn a_claiming_completion_is_a_claim_and_the_entry_is_provisional() {
        let nodes = chain(vec![
            authored("alpha", 1, "bassel", Some(mk_flat(0.25).expect("valid"))),
            mk_completed("alpha", at(3), "triage", "").expect("valid"),
        ]);
        let entries = entries(&nodes, at(9), &roster()).expect("folds");
        let [entry] = &entries[..] else {
            panic!("one todo")
        };
        assert_eq!(
            entry.outcome,
            Candidates::from([None]),
            "the confirmed reading refuses the claim"
        );
        assert_eq!(entry.claimed(), "completed");
        assert_eq!(
            entry.confidence,
            Confidence::Provisional(Provisional::Claimed),
            "the claim alone, since bassel wrote the content"
        );
        assert_eq!(
            entry.value(),
            Ok(Some(0.25)),
            "an open todo prices by its spec"
        );
        assert_eq!(entry.stream.len(), 2);
    }

    #[test]
    fn an_unconfirmed_record_makes_the_entry_provisional_without_a_claim() {
        let nodes = chain(vec![authored("alpha", 1, "triage", None)]);
        let entries = entries(&nodes, at(9), &roster()).expect("folds");
        let [entry] = &entries[..] else {
            panic!("one todo")
        };
        assert_eq!(entry.claim, None);
        assert_eq!(
            entry.confidence,
            Confidence::Provisional(Provisional::Content)
        );
        assert_eq!(
            entry.content,
            [nodes[0].name.clone()],
            "the name of the winning record, not the record"
        );
        assert_eq!(
            fpl::print_term(entry.spec()),
            "Absent()",
            "no spec and no checklist is no function: Absent"
        );
        assert_eq!(entry.value(), Ok(None), "and Absent reads ∅, not a number");
    }

    #[test]
    fn a_todo_whose_only_event_is_dated_after_the_moment_is_still_a_row() {
        let nodes = chain(vec![authored(
            "alpha",
            9,
            "bassel",
            Some(mk_flat(0.5).expect("valid")),
        )]);
        let entries = entries(&nodes, at(1), &roster()).expect("folds");
        let [entry] = &entries[..] else {
            panic!("one todo")
        };
        assert_eq!(entry.confidence, Confidence::Confirmed);
        assert!(entry.is_open());
        assert_eq!(entry.value(), Ok(None), "the chain knows no price yet");
        assert!(entry.content.is_empty(), "and no record yet");
        assert_eq!(entry.stream.len(), 1, "the object is still its stream");
    }

    #[test]
    fn a_resolved_todo_reads_one_and_carries_its_instant() {
        let nodes = chain(vec![
            authored("alpha", 1, "bassel", Some(mk_flat(0.2).expect("valid"))),
            mk_completed("alpha", at(3), "bassel", "").expect("valid"),
        ]);
        let entries = entries(&nodes, at(9), &roster()).expect("folds");
        let [entry] = &entries[..] else {
            panic!("one todo")
        };
        assert_eq!(
            entry.outcome,
            Candidates::from([Some(Outcome::Completed(fpl::instant_of(at(3))))])
        );
        assert_eq!(entry.claim, None, "the two folds agree");
        assert_eq!(entry.confidence, Confidence::Confirmed);
        assert_eq!(entry.value(), Ok(Some(1.0)));
    }

    #[test]
    fn a_reference_prices_by_the_todo_it_names_and_its_lifecycle() {
        let group = fpl::mk_conj(
            vec![
                fpl::mk_ref("alpha".to_owned()).expect("valid"),
                fpl::mk_ref("beta".to_owned()).expect("valid"),
            ],
            -1.0,
        )
        .expect("valid");
        let nodes = chain(vec![
            authored("alpha", 1, "bassel", Some(mk_flat(0.25).expect("valid"))),
            authored("beta", 1, "bassel", Some(mk_flat(0.5).expect("valid"))),
            authored("group", 1, "bassel", Some(group)),
            mk_completed("beta", at(3), "bassel", "").expect("valid"),
        ]);
        let rows = entries(&nodes, at(2), &roster()).expect("folds");
        // The harmonic mean of 0.25 and 0.5 while both are open ...
        assert_eq!(rows[2].todo.as_str(), "group");
        let value = |row: &Entry| row.value().expect("linked").expect("a value");
        assert!((value(&rows[2]) - 1.0 / 3.0).abs() < 1e-12);
        // ... and of 0.25 and 1.0 once beta is done.
        let rows = entries(&nodes, at(9), &roster()).expect("folds");
        assert!((value(&rows[2]) - 0.4).abs() < 1e-12);
        assert_eq!(rows[2].unlinked(), None);
    }

    #[test]
    fn a_reference_that_does_not_link_has_a_function_and_no_value() {
        let nodes = chain(vec![authored(
            "alpha",
            1,
            "bassel",
            Some(fpl::mk_ref("ghost".to_owned()).expect("valid")),
        )]);
        let rows = entries(&nodes, at(9), &roster()).expect("folds");
        assert_eq!(fpl::print_term(rows[0].spec()), "Ref(todo='ghost')");
        assert_eq!(
            rows[0].value(),
            Err(&LinkError::Unknown("ghost".to_owned()))
        );
        assert_eq!(
            rows[0].unlinked(),
            Some(&LinkError::Unknown("ghost".to_owned()))
        );
    }

    #[test]
    fn a_reference_to_a_known_todo_with_no_spec_links_to_absent() {
        let group = fpl::mk_conj(
            vec![
                fpl::mk_ref("alpha".to_owned()).expect("valid"),
                fpl::mk_ref("beta".to_owned()).expect("valid"),
                fpl::mk_ref("gamma".to_owned()).expect("valid"),
            ],
            -1.0,
        )
        .expect("valid");
        let nodes = chain(vec![
            authored("alpha", 1, "bassel", Some(mk_flat(0.25).expect("valid"))),
            // A record with no spec, and a todo only ever created.
            authored("beta", 1, "bassel", None),
            crate::event::mk_created("gamma", at(1), "bassel", "a note", "").expect("valid"),
            authored("group", 1, "bassel", Some(group)),
        ]);
        let rows = entries(&nodes, at(9), &roster()).expect("folds");
        let [alpha, beta, gamma, group] = &rows[..] else {
            panic!("four todos")
        };
        assert_eq!(alpha.value(), Ok(Some(0.25)));
        assert_eq!((beta.value(), gamma.value()), (Ok(None), Ok(None)));
        // The two absent members neither raise nor lower the mean.
        assert_eq!(group.value(), Ok(Some(0.25)));
        assert_eq!(
            group.linked().map(|closed| fpl::print_term(closed.term())),
            Some("Conj(terms=(Flat(value=0.25), Absent(), Absent()), p=-1.0)".to_owned())
        );
    }

    #[test]
    fn a_list_puts_every_row_with_no_number_after_the_priced_ones_by_id() {
        let nodes = chain(vec![
            authored("delta", 1, "bassel", Some(mk_flat(0.75).expect("valid"))),
            authored("gamma", 1, "bassel", None),
            authored("beta", 1, "bassel", Some(mk_flat(0.25).expect("valid"))),
            authored(
                "alpha",
                1,
                "bassel",
                Some(fpl::mk_ref("ghost".to_owned()).expect("valid")),
            ),
            authored("epsilon", 1, "bassel", Some(mk_flat(0.25).expect("valid"))),
        ]);
        let mut rows = entries(&nodes, at(9), &roster()).expect("folds");
        rows.sort_by(list_order);
        let order: Vec<&str> = rows.iter().map(|row| row.todo.as_str()).collect();
        // Ascending in value, ties by id; then `absent` and the unlinked, by id.
        assert_eq!(order, ["beta", "epsilon", "delta", "alpha", "gamma"]);
    }

    #[test]
    fn the_confidence_of_no_asymmetry_is_confirmed() {
        assert_eq!(Confidence::of(false, false), Confidence::Confirmed);
        assert!(!Confidence::of(false, false).is_provisional());
        assert!(Confidence::of(true, true).is_provisional());
        assert_eq!(
            Confidence::of(true, true),
            Confidence::Provisional(Provisional::ClaimedAndContent)
        );
    }
}
