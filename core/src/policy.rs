//! §5 — STANDING: does this event bind, or does it only claim?
//!
//! See ../../SPEC.md. Every fold in [`crate::fold`], the register action in
//! [`crate::registers`], the composition in [`crate::view`] and §3's `verify`
//! ask exactly this one question about an event, and a [`Policy`] — the HOST's
//! — is what answers it.
//!
//! THE DATABASE DOES NOT DECIDE WHOM TO BELIEVE. Which writers a deployment
//! stands behind is not a fact about a DAG of events, any more than what a
//! record MEANS is (§4, and the same move: 0.2 made the record the host's,
//! 0.3 makes the standing the host's). The core carried one deployment's
//! answer — a set of untrusted actor names, plus a kind-dependent rule — and
//! that answer is now [`Untrusted`], the REFERENCE policy: still here, still
//! what `conformance/` was taken under, and no longer something the folds
//! know.
//!
//! WHAT STAYS IN THE CORE is the two-reading fold: the CONFIRMED environment
//! and the CLAIMED one, side by side in [`crate::view::entries`]. A store that
//! held only the filtered reading could not show a claim at all, and showing
//! one is the whole point of storing it. [`Everything`] is the policy the
//! claimed reading is taken under.

use std::collections::BTreeSet;

use crate::event::{Actor, TodoEvent};
use crate::payload::Payload;

/// What a [`Policy`] says about one event: the two readings, as a sum.
///
/// A `bool` would have been the same two inhabitants with none of the meaning:
/// every call site here reads "does this write" and "is this a claim" out of
/// the same value, and the two are not negations of each other by luck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Standing {
    /// The folds take it: it writes its registers, and what it says is what
    /// the store believes.
    Binds,
    /// The folds refuse it. It is stored and it is SHOWN — as a claim, beside
    /// the answer that stands — and it changes no confirmed reading (§9.11).
    Claims,
}

impl Standing {
    pub fn binds(self) -> bool {
        matches!(self, Standing::Binds)
    }

    pub fn claims(self) -> bool {
        matches!(self, Standing::Claims)
    }
}

/// A deployment's answer to §5, as a type.
///
/// One required method: the standing of an event, which is a function of THAT
/// EVENT and nothing else — not of the log, not of the order, not of the
/// moment. §9.12 is that fact as a law: because standing selects events rather
/// than positions, folding under a policy is folding the sub-log of its
/// binding events, whatever order the log arrives in.
///
/// NOT SEALED. A host implements this; it is the seam the decision leaves the
/// core through. [`Untrusted`] is the reference implementation and
/// [`Everything`] the trivial one.
///
/// Generic over the payload because an event carries one, so a host whose
/// records say something about their own standing can read it. A policy that
/// does not care — the two here do not — implements it for every `P` in one
/// blanket impl.
pub trait Policy<P: Payload> {
    /// Does this event write, or does it only claim?
    fn standing(&self, event: &TodoEvent<P>) -> Standing;

    /// Is this event the host's OWN word?
    ///
    /// Two readers ask, and neither is a fold: [`crate::view::entries`] marks
    /// a row whose winning content record is not the host's own
    /// ([`crate::view::Provisional::Content`]), and §3's `verify` holds such
    /// an event's `at` to the DAG's order — a writer whose stamp the host
    /// forces cannot legitimately be dated behind what it was written on top
    /// of, where a human backfilling 2025 completions today legitimately can.
    ///
    /// THE DEFAULT IS §5 EXACTLY: a claiming event is not the host's word, a
    /// binding one is. A policy that folds a writer's events while still
    /// marking them as that writer's overrides it — the reference policy does,
    /// because a content record from an untrusted actor binds (writing content
    /// is what such a writer is for) and is still shown as its author's.
    ///
    /// `standing(e) == Claims` implies `!confirms(e)`; every implementation
    /// here keeps that, and §9's laws are stated over policies that do.
    fn confirms(&self, event: &TodoEvent<P>) -> bool {
        self.standing(event).binds()
    }
}

/// The policy under which every event binds.
///
/// §6.7's CLAIMED reading is taken under it — "what would every writer's
/// events bind" — and §9.10 is the law that says the two readings are then the
/// same reading. `Untrusted::none()` is equal to it and says the same thing
/// about a deployment with an empty roster.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Everything;

impl<P: Payload> Policy<P> for Everything {
    fn standing(&self, _event: &TodoEvent<P>) -> Standing {
        Standing::Binds
    }
}

/// THE REFERENCE POLICY: a set of actor names, and the rule the conformance
/// vectors were taken under.
///
/// A named actor's LIFECYCLE and `SpecRevised` events CLAIM — stored, shown,
/// never folded — because such a writer reads attacker-controlled input and an
/// injected completion or repricing is the threat the roster exists for. Its
/// CONTENT records BIND: writing content is what the writer is for, and a fold
/// that hid its writes would not be containment, it would be an outage that
/// reports success. They are still not the host's own word, so
/// [`Policy::confirms`] refuses them and the reader marks the row.
///
/// It lives in the core, not behind the `reference` feature that gates the
/// reference PAYLOAD: it is a policy over the core's own event kinds, it needs
/// nothing from any payload, and the core's conformance suites — which read
/// `conformance/`'s `untrusted` fields through it — would have to be
/// feature-gated with it for no gain. The `reference` feature answers "which
/// record shape"; this answers "whose word", and one store can want the second
/// without the first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Untrusted(BTreeSet<Actor>);

impl Untrusted {
    /// A deployment that stands behind every writer, said out loud.
    pub fn none() -> Untrusted {
        Untrusted(BTreeSet::new())
    }

    pub fn of(actors: impl IntoIterator<Item = Actor>) -> Untrusted {
        Untrusted(actors.into_iter().collect())
    }

    /// The roster. A reader that wants to SHOW the policy asks; nothing in the
    /// core reads an actor name to decide anything.
    pub fn actors(&self) -> &BTreeSet<Actor> {
        &self.0
    }
}

impl<P: Payload> Policy<P> for Untrusted {
    fn standing(&self, event: &TodoEvent<P>) -> Standing {
        if matches!(event, TodoEvent::Authored(_)) || !self.0.contains(event.actor()) {
            Standing::Binds
        } else {
            Standing::Claims
        }
    }

    /// The roster, whatever the kind — the override the doc on
    /// [`Policy::confirms`] describes. An untrusted actor's content record
    /// binds and is still that actor's.
    fn confirms(&self, event: &TodoEvent<P>) -> bool {
        !self.0.contains(event.actor())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{mk_completed, mk_created};
    use crate::literal::Datetime;
    use crate::reference::{mk_authored, Todo};

    type Event = TodoEvent<Todo>;

    fn at() -> Datetime {
        Datetime::new(2026, 9, 6, 7, 3, 0, 0).expect("a real instant")
    }

    fn record(actor: &str) -> Event {
        mk_authored(
            "alpha",
            at(),
            actor,
            "todo",
            at(),
            "body",
            None,
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
    fn a_named_actors_lifecycle_event_claims_and_its_record_binds() {
        let policy = roster();
        let claimed: Event = mk_completed("alpha", at(), "triage", "").expect("valid");
        assert_eq!(policy.standing(&claimed), Standing::Claims);
        assert_eq!(policy.standing(&record("triage")), Standing::Binds);
        assert_eq!(
            policy.standing(&mk_completed::<Todo>("alpha", at(), "bassel", "").expect("valid")),
            Standing::Binds
        );
    }

    #[test]
    fn a_record_that_binds_can_still_not_be_the_hosts_own_word() {
        let policy = roster();
        assert!(policy.standing(&record("triage")).binds());
        assert!(
            !policy.confirms(&record("triage")),
            "it binds, and it is still the agent's"
        );
        assert!(policy.confirms(&record("bassel")));
    }

    #[test]
    fn an_empty_roster_is_the_policy_that_binds_everything() {
        let none = Untrusted::none();
        for event in [
            record("triage"),
            mk_completed("alpha", at(), "triage", "").expect("valid"),
            mk_created("alpha", at(), "triage", "", "").expect("valid"),
        ] {
            assert_eq!(none.standing(&event), Everything.standing(&event));
            assert_eq!(none.confirms(&event), Everything.confirms(&event));
        }
    }

    #[test]
    fn a_claiming_event_is_never_the_hosts_own_word() {
        let policy = roster();
        let claimed: Event = mk_completed("alpha", at(), "triage", "").expect("valid");
        assert!(policy.standing(&claimed).claims() && !policy.confirms(&claimed));
    }
}
