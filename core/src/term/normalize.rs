use std::collections::BTreeSet;

use crate::fpl::{in_force, piecewise, Instant};
use crate::term::{Term, TermF};

/// The Piecewise-outermost normal form (§7). A pointwise layer lifts over the
/// merged partition of its children's schedules and a `Shift` translates the
/// instants it crosses; `Within`, `After`, `Recur` and `Periodic` do not
/// commute with a partition and stay where they are.
pub fn normalize(term: &Term) -> Term {
    term.cata(|layer| match layer {
        TermF::Piecewise { head, pieces } => piecewise(head, pieces),
        TermF::Shift { delta, term } => match term.into_out() {
            // ⟦Shift(δ, pw)⟧(now) = ⟦pw⟧(now + δ), so a piece from τ is in force from τ − δ.
            TermF::Piecewise { head, pieces } => piecewise(
                Term::new(TermF::Shift { delta, term: head }),
                pieces
                    .into_iter()
                    .map(|(at, term)| (at - delta, Term::new(TermF::Shift { delta, term })))
                    .collect(),
            ),
            term => Term::new(TermF::Shift {
                delta,
                term: Term::new(term),
            }),
        },
        layer @ (TermF::Conj { .. }
        | TermF::Offset { .. }
        | TermF::Gate { .. }
        | TermF::Importance { .. }
        | TermF::OffsetBy { .. }) => pointwise(layer),
        layer => Term::new(layer),
    })
}

/// A pointwise layer over its children's schedules: the layer over what each
/// child reads before any transition, and again at every instant one changes.
fn pointwise(layer: TermF<Term>) -> Term {
    let schedules: Vec<&[(Instant, Term)]> = layer
        .children()
        .into_iter()
        .filter_map(|child| match child.out() {
            TermF::Piecewise { pieces, .. } => Some(pieces.as_slice()),
            _ => None,
        })
        .collect();
    if schedules.is_empty() {
        return Term::new(layer);
    }
    let instants: BTreeSet<Instant> = schedules
        .iter()
        .flat_map(|pieces| pieces.iter().map(|(at, _)| *at))
        .collect();
    let at = |moment: Option<Instant>| {
        Term::new(layer.map(|child| match (child.out(), moment) {
            (TermF::Piecewise { head, .. }, None) => head.clone(),
            (TermF::Piecewise { head, pieces }, Some(moment)) => {
                in_force(head, pieces, moment).1.clone()
            }
            _ => child.clone(),
        }))
    };
    piecewise(
        at(None),
        instants.into_iter().map(|m| (m, at(Some(m)))).collect(),
    )
}
