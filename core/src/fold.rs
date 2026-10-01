//! §6.1–6.5 — belief at a moment: each todo's stream folded into its
//! registers, and every reading a projection of them.
//!
//! See ../../SPEC.md §6. ORDER IS CAUSAL, TIME IS DATA (§1): a write
//! joins its register when it is dated by the moment asked and the policy
//! admits it, and what it supersedes is ancestry alone. No clock and no
//! linearisation picks a winner; a conflict is its candidates.

mod frontier;
mod order;
mod register;
mod write;

use std::collections::{BTreeMap, BTreeSet};

pub use frontier::Frontier;
pub use order::{grows, maximal, Discrete, Inflationary, Order, Total};
pub use register::{GrowSet, Register};
pub use write::{Kind, Write};

use crate::event::{Authored, Hash, TodoEvent, TodoId};
use crate::fpl::{self, Candidates, Env, FplError, Instant};
use crate::payload::Payload;
use crate::policy::{Everything, Policy};
use crate::registers::{Prodrome, Stamp};
use crate::term::Term;

/// One todo's registers at a moment: the one fold every reading projects.
/// The state, spec and content are discrete; the tendings, a grow-only set,
/// are ordered by inclusion.
#[derive(Debug, Clone, PartialEq)]
pub struct Registers<'a, P> {
    pub state: Frontier<'a, P>,
    pub spec: Frontier<'a, P>,
    pub content: Frontier<'a, P>,
    pub tended: GrowSet<Instant>,
}

impl<P> Default for Registers<'_, P> {
    fn default() -> Self {
        Registers {
            state: Frontier::default(),
            spec: Frontier::default(),
            content: Frontier::default(),
            tended: GrowSet::new(),
        }
    }
}

/// Does `stamp` join its registers at `at` (`None`: ever) under `policy`? A
/// record joins whoever wrote it (§6.2, §6.3); every other write only where
/// the policy binds it.
fn admits<P: Payload>(stamp: &Stamp<P>, at: Option<Instant>, policy: &impl Policy<P>) -> bool {
    at.is_none_or(|at| fpl::instant_of(stamp.event.at()) <= at)
        && (matches!(*stamp.event, TodoEvent::Authored(_)) || policy.standing(&stamp.event).binds())
}

impl<'a, P: Payload> Registers<'a, P> {
    pub fn read(stream: &'a [Stamp<P>], at: Option<Instant>, policy: &impl Policy<P>) -> Self {
        let mut registers = Registers::default();
        for stamp in stream.iter().filter(|stamp| admits(stamp, at, policy)) {
            for write in Write::of(&stamp.event) {
                registers.join(stamp, write);
            }
        }
        registers
    }

    /// The writes of `written` that no other write to their register precedes:
    /// what each register held first. The state has nothing to fall back to.
    fn earliest(written: &[&'a Stamp<P>]) -> Self {
        let mut first = Registers::default();
        for stamp in written {
            for write in Write::of(&stamp.event) {
                let preceded = written.iter().any(|other| {
                    stamp.descends(other)
                        && Write::of(&other.event).any(|w| w.kind() == write.kind())
                });
                if write.kind().is_some_and(|kind| kind != Kind::State) && !preceded {
                    first.join(stamp, write);
                }
            }
        }
        first
    }

    fn join(&mut self, stamp: &'a Stamp<P>, write: Write<'a, P>) {
        match write {
            Write::State(_) => self.state.join(stamp),
            Write::Spec(_) => self.spec.join(stamp),
            Write::Content(_) => self.content.join(stamp),
            Write::Tend(at) => self.tended.join(at),
        }
    }

    pub fn frontier(&self, kind: Kind) -> &Frontier<'a, P> {
        match kind {
            Kind::State => &self.state,
            Kind::Spec => &self.spec,
            Kind::Content => &self.content,
        }
    }

    /// The candidate bindings, `{None}` for an open todo.
    pub fn outcomes(&self) -> Candidates {
        let mut out: Candidates = self
            .state
            .candidates()
            .into_iter()
            .flat_map(|stamp| Write::of(&stamp.event))
            .filter_map(|write| match write {
                Write::State(binding) => Some(binding),
                _ => None,
            })
            .collect();
        if out.is_empty() {
            out.insert(None);
        }
        out
    }

    pub fn specs(&self) -> Vec<&'a Term> {
        self.spec
            .candidates()
            .into_iter()
            .flat_map(|stamp| Write::of(&stamp.event))
            .filter_map(|write| match write {
                Write::Spec(spec) => Some(spec),
                _ => None,
            })
            .collect()
    }

    pub fn records(&self) -> Vec<&'a Authored<P>> {
        self.content
            .candidates()
            .into_iter()
            .flat_map(|stamp| Write::of(&stamp.event))
            .filter_map(|write| match write {
                Write::Content(record) => Some(record),
                _ => None,
            })
            .collect()
    }

    /// Every register with more than one write, each named by its writes.
    pub fn conflicts(&self) -> BTreeMap<Kind, Vec<Hash>> {
        [Kind::State, Kind::Spec, Kind::Content]
            .into_iter()
            .filter(|kind| self.frontier(*kind).is_conflict())
            .map(|kind| (kind, self.frontier(kind).names()))
            .collect()
    }

    /// Record this todo in `env`: its bindings unless simply open, and its
    /// tendings.
    pub fn bind(&self, todo: &TodoId, env: &mut Env) {
        let outcomes = self.outcomes();
        if outcomes != Candidates::from([None]) {
            env.outcomes.insert(todo.as_str().to_owned(), outcomes);
        }
        if !self.tended.is_empty() {
            env.tended
                .insert(todo.as_str().to_owned(), self.tended.clone());
        }
    }

    /// `Least` over the worlds in force, one candidate from each register,
    /// falling back to `first` where a register is unwritten yet: a flat 1.0
    /// while resolved, else `checklist(spec, items)`, else `head`.
    fn worlds(&self, first: &Self, head: Option<&Term>) -> Result<Option<Term>, FplError> {
        let mut specs: Vec<Option<&Term>> = or_first(self.specs(), first.specs())
            .into_iter()
            .map(Some)
            .collect();
        if specs.is_empty() {
            specs.push(None);
        }
        let mut items: Vec<usize> = or_first(self.records(), first.records())
            .into_iter()
            .map(|record| record.payload.checklist_len())
            .collect();
        if items.is_empty() {
            items.push(0);
        }
        let closed: BTreeSet<bool> = self.outcomes().iter().map(Option::is_some).collect();
        let mut terms = Vec::new();
        for closed in closed {
            for spec in &specs {
                for items in &items {
                    let term = if closed {
                        Some(fpl::mk_flat(1.0)?)
                    } else {
                        fpl::checklist(spec.cloned(), *items)?.or_else(|| head.cloned())
                    };
                    terms.extend(term);
                }
            }
        }
        if terms.is_empty() {
            Ok(None)
        } else {
            fpl::least_of(terms).map(Some)
        }
    }
}

/// `mine`, or `theirs` where `mine` is empty.
fn or_first<T>(mine: Vec<T>, theirs: Vec<T>) -> Vec<T> {
    if mine.is_empty() {
        theirs
    } else {
        mine
    }
}

/// §6.1 — the environment at `at`: each todo's candidate bindings and its
/// tendings.
pub fn env<P: Payload>(prodrome: &Prodrome<P>, at: Instant, policy: &impl Policy<P>) -> Env {
    let mut env = Env::new();
    for (todo, stream) in prodrome {
        Registers::read(stream, Some(at), policy).bind(todo, &mut env);
    }
    env
}

/// §6.2 — each todo's candidate specs at `at`. Absence is not zero: a todo
/// missing here has no price, not a price of nothing.
pub fn specs<P: Payload>(
    prodrome: &Prodrome<P>,
    at: Instant,
    policy: &impl Policy<P>,
) -> BTreeMap<TodoId, Vec<Term>> {
    prodrome
        .iter()
        .map(|(todo, stream)| {
            let specs = Registers::read(stream, Some(at), policy).specs();
            (todo.clone(), specs.into_iter().cloned().collect::<Vec<_>>())
        })
        .filter(|(_, specs)| !specs.is_empty())
        .collect()
}

/// §6.3 — each todo's candidate records at `at`, whoever wrote them: the
/// reader marks an unconfirmed one ([`crate::view::Provisional::Content`]).
pub fn content<P: Payload>(
    prodrome: &Prodrome<P>,
    at: Instant,
) -> BTreeMap<TodoId, Vec<Authored<P>>> {
    prodrome
        .iter()
        .map(|(todo, stream)| {
            let records = Registers::read(stream, Some(at), &Everything).records();
            (
                todo.clone(),
                records.into_iter().cloned().collect::<Vec<_>>(),
            )
        })
        .filter(|(_, records)| !records.is_empty())
        .collect()
}

/// §6.4 — each todo's history as of `at`, as ONE fulfillment function: a
/// piece at every instant one of its registers was written, the term in each
/// `Least` over the worlds in force there. The head, extending to −∞, is the
/// same over each register's earliest writes; a todo with no spec and no
/// checklist has no function.
pub fn flatten<P: Payload>(
    prodrome: &Prodrome<P>,
    at: Instant,
    policy: &impl Policy<P>,
) -> Result<BTreeMap<TodoId, Term>, FplError> {
    let mut out = BTreeMap::new();
    for (todo, stream) in prodrome {
        if let Some(function) = function(stream, at, policy)? {
            out.insert(todo.clone(), function);
        }
    }
    Ok(out)
}

fn function<P: Payload>(
    stream: &[Stamp<P>],
    at: Instant,
    policy: &impl Policy<P>,
) -> Result<Option<Term>, FplError> {
    let written: Vec<&Stamp<P>> = stream
        .iter()
        .filter(|stamp| admits(stamp, Some(at), policy))
        .filter(|stamp| Write::of(&stamp.event).any(|write| write.kind().is_some()))
        .collect();
    let first = Registers::earliest(&written);
    let Some(head) = first.worlds(&first, None)? else {
        return Ok(None);
    };
    let moments: BTreeSet<Instant> = written
        .iter()
        .map(|stamp| fpl::instant_of(stamp.event.at()))
        .collect();
    let mut pieces = Vec::with_capacity(moments.len());
    for m in moments {
        let registers = Registers::read(stream, Some(m), policy);
        let term = registers.worlds(&first, Some(&head))?;
        pieces.push((m, term.unwrap_or_else(|| head.clone())));
    }
    fpl::mk_piecewise(head, pieces).map(Some)
}

/// [`flatten`]'s functions as [`fpl::link`] reads them, over every todo in
/// `known`: `Absent` where a known todo has none (§7.2).
pub fn link_specs<'a>(
    functions: &BTreeMap<TodoId, Term>,
    known: impl IntoIterator<Item = &'a TodoId>,
) -> BTreeMap<String, Term> {
    let mut specs: BTreeMap<String, Term> = known
        .into_iter()
        .map(|todo| (todo.as_str().to_owned(), fpl::mk_absent()))
        .collect();
    specs.extend(
        functions
            .iter()
            .map(|(todo, term)| (todo.as_str().to_owned(), term.clone())),
    );
    specs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{mk_completed, mk_reopened, mk_sealed, seal_hash, Actor};
    use crate::fpl::{fulfillment, mk_flat, print_term};
    use crate::literal::Datetime;
    use crate::policy::Untrusted;
    use crate::reference::{mk_authored, mk_subtodo, Todo};
    use crate::registers::{fold, Node};

    type Event = TodoEvent<Todo>;

    /// A log as its writer's chain, folded to its one prodrome.
    fn prodrome(log: &[Event]) -> Prodrome<Todo> {
        let mut prev = None;
        let mut nodes = Vec::new();
        for event in log {
            let envelope = mk_sealed(prev, event.clone());
            let name = seal_hash(&envelope);
            nodes.push(Node::of(name.clone(), &envelope));
            prev = Some(name);
        }
        fold(&nodes).prodromes()[&None].clone()
    }

    fn env_at(log: &[Event], t: Datetime, policy: &impl Policy<Todo>) -> Env {
        env(&prodrome(log), fpl::instant_of(t), policy)
    }

    fn flat(log: &[Event], t: Datetime) -> BTreeMap<TodoId, Term> {
        flatten(&prodrome(log), fpl::instant_of(t), &roster()).expect("folds")
    }

    fn at(day: u32) -> Datetime {
        Datetime::new(2026, 9, day, 12, 0, 0, 0).expect("a real instant")
    }

    fn authored(todo: &str, day: u32, actor: &str, spec: Option<Term>, items: usize) -> Event {
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

    fn roster() -> Untrusted {
        Untrusted::of([Actor::new("triage").expect("valid")])
    }

    #[test]
    fn a_completion_binds_and_a_reopening_clears() {
        let log: Vec<Event> = vec![
            mk_completed("alpha", at(2), "bassel", "").expect("valid"),
            mk_reopened("alpha", at(4), "bassel", "").expect("valid"),
        ];
        let policy = roster();
        assert_eq!(env_at(&log, at(3), &policy).outcomes.len(), 1);
        assert!(env_at(&log, at(5), &policy).outcomes.is_empty());
    }

    #[test]
    fn a_claiming_completion_is_a_claim_and_not_a_binding() {
        let log: Vec<Event> = vec![mk_completed("alpha", at(2), "triage", "").expect("valid")];
        assert!(env_at(&log, at(3), &roster()).outcomes.is_empty());
        assert_eq!(env_at(&log, at(3), &Untrusted::none()).outcomes.len(), 1);
    }

    #[test]
    fn a_resolved_todo_reads_one_from_the_instant_it_was_resolved() {
        let log = vec![
            authored("alpha", 1, "bassel", Some(mk_flat(0.2).expect("valid")), 0),
            mk_completed("alpha", at(3), "bassel", "").expect("valid"),
        ];
        let flat = flat(&log, at(9));
        let term =
            &fpl::Closed::of(flat[&TodoId::new("alpha").expect("valid")].clone()).expect("no Ref");
        let env = env_at(&log, at(9), &roster());
        assert_eq!(fulfillment(term, fpl::instant_of(at(2)), &env), Some(0.2));
        assert_eq!(fulfillment(term, fpl::instant_of(at(4)), &env), Some(1.0));
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
        let flat = flat(&log, at(9));
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
        assert!(flat(&log, at(9)).is_empty());
    }
}
