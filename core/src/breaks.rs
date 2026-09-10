//! §7 breakpoints and the series a graph draws.
//!
//! See ../../SPEC.md. Implemented against ../../conformance/series.py, and
//! ported from the reference `view.py` (`Breaks`, `_breakpoints`,
//! `_operator_breaks`, `_constant`, `series_of`).
//!
//! The question this module answers is where a term stops being a straight
//! line. Flat, Decay, Curve and a Piecewise of those are EXACT: knots at the
//! window edges, at every slope change inside, and a second on either side of
//! every jump, and a straight line between two adjacent knots IS the curve
//! (§9.7). Everything else is sampled on top, and `exact` tells the reader
//! which it got — a number on screen the evaluator never produced is the
//! failure this flag exists to prevent.

use std::collections::BTreeSet;

use chrono::Duration;

use crate::fpl::{div_delta, fulfillment, normalize, Env, Instant, Term, TermF};

/// For terms that are not piecewise-linear: every ~1.9 h over a week, every
/// ~9 h over a month.
pub const SAMPLES: i64 = 96;

/// Where a term changes slope, where it jumps, and whether that is the whole
/// story.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Breaks {
    pub slopes: Vec<Instant>,
    pub jumps: Vec<Instant>,
    /// Every part is piecewise-linear, so knots at these instants ARE the curve.
    pub exact: bool,
}

impl Breaks {
    fn none() -> Self {
        Breaks {
            slopes: vec![],
            jumps: vec![],
            exact: true,
        }
    }
    fn sampled() -> Self {
        Breaks {
            slopes: vec![],
            jumps: vec![],
            exact: false,
        }
    }
}

/// The slope changes and jumps of a term, and whether that is the whole story.
///
/// A jump is a discontinuity — the curve needs a knot a second BEFORE it too,
/// or a line drawn across it would be a ramp the evaluator never produced. A
/// Decay jumps twice: at `start_date` (1.0 down to the pre-window 0.98) and at
/// the window's start (0.98 down to `start`); its descent to `end` is
/// continuous. A Piecewise jumps at every piece — a revision, a completion, a
/// reopening — and is exact when its parts are; a composite part makes the
/// whole sampled, but its transitions are still knots, since a sample grid
/// would draw a ramp across the instant a todo was completed.
pub fn breakpoints(term: &Term) -> Breaks {
    match term.out() {
        TermF::Flat { .. } => Breaks::none(),
        TermF::Decay {
            end_date,
            lead_up,
            start_date,
            ..
        } => {
            let mut jumps: BTreeSet<Instant> = BTreeSet::new();
            jumps.insert(*end_date - *lead_up);
            if let Some(sd) = start_date {
                jumps.insert(*sd);
            }
            Breaks {
                slopes: vec![*end_date],
                jumps: jumps.into_iter().collect(),
                exact: true,
            }
        }
        TermF::Curve { points } => Breaks {
            slopes: points.iter().map(|pt| pt.at).collect(),
            jumps: vec![],
            exact: true,
        },
        TermF::Piecewise { head, pieces } => {
            let parts: Vec<Breaks> = std::iter::once(breakpoints(head))
                .chain(pieces.iter().map(|(_, t)| breakpoints(t)))
                .collect();
            Breaks {
                slopes: parts
                    .iter()
                    .flat_map(|p| p.slopes.iter().copied())
                    .collect(),
                jumps: parts
                    .iter()
                    .flat_map(|p| p.jumps.iter().copied())
                    .chain(pieces.iter().map(|(at, _)| *at))
                    .collect(),
                exact: parts.iter().all(|p| p.exact),
            }
        }
        _ => operator_breaks(term),
    }
}

/// The operators: two preserve piecewise-linearity outright, three preserve
/// constancy, the rest are sampled.
fn operator_breaks(term: &Term) -> Breaks {
    match term.out() {
        // x·(1−|δ|) + max(0, δ) is affine in x: same breakpoints, same exactness.
        TermF::Offset { term: inner, .. } => breakpoints(inner),
        // A shift by δ reads the inner term at now + δ: every breakpoint moves by −δ.
        TermF::Shift { delta, term: inner } => {
            let within = breakpoints(inner);
            Breaks {
                slopes: within.slopes.iter().map(|b| *b - *delta).collect(),
                jumps: within.jumps.iter().map(|b| *b - *delta).collect(),
                exact: within.exact,
            }
        }
        TermF::OffsetBy { delta, term: inner } => {
            if constant(inner) {
                // offset(c, δ) is affine in δ on [0, 1]: the delta's breakpoints.
                breakpoints(delta)
            } else if constant(delta) {
                breakpoints(inner)
            } else {
                Breaks::sampled()
            }
        }
        // A power mean, a max, or a power of constants is a constant.
        TermF::Conj { .. } | TermF::Gate { .. } | TermF::Importance { .. }
            if parts(term).iter().all(|p| constant(p)) =>
        {
            Breaks::none()
        }
        _ => Breaks::sampled(),
    }
}

fn parts(term: &Term) -> Vec<&Term> {
    match term.out() {
        TermF::Conj { terms, .. } => terms.iter().collect(),
        TermF::Gate { gate, body } => vec![gate, body],
        TermF::Importance { term, .. } => vec![term],
        _ => vec![],
    }
}

/// A term with no slope change and no jump, exactly: a constant.
pub fn constant(term: &Term) -> bool {
    let breaks = breakpoints(term);
    breaks.exact && breaks.slopes.is_empty() && breaks.jumps.is_empty()
}

/// One point of a drawn curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Knot {
    pub at: Instant,
    pub value: f64,
}

/// A term's fulfillment over a window, and whether the knots ARE the curve.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub knots: Vec<Knot>,
    pub exact: bool,
}

/// The curve the graph draws for one term over `[from, to]`.
///
/// Exact where the term is piecewise-linear: knots at the window edges, at
/// every slope change inside it, and a second on either side of every jump.
/// Composite terms get `SAMPLES` evenly spaced evaluations on top. Every knot
/// is evaluated against `env_at(t)`, the environment AS OF that instant, so a
/// dependency an `After` reads is unbound before it completed and bound from
/// then — the same rule for the environment as for the term.
pub fn series_knots(
    term: &Term,
    from: Instant,
    to: Instant,
    env_at: impl Fn(Instant) -> Env,
) -> Series {
    // The schedule at the root, where the breakpoints can read it.
    let spec = normalize(term);
    let breaks = breakpoints(&spec);
    let mut instants: BTreeSet<Instant> = BTreeSet::from([from, to]);
    for b in breaks.slopes.iter().chain(&breaks.jumps) {
        if from < *b && *b < to {
            instants.insert(*b);
        }
    }
    for j in &breaks.jumps {
        if from < *j && *j < to {
            instants.insert(*j - Duration::seconds(1));
        }
    }
    if !breaks.exact {
        // The reference's `(end - start) / SAMPLES` is microsecond-exact and
        // rounded half to even; `i * step` is then exact. Anything coarser
        // would put the samples on instants the reference never evaluated.
        let step = div_delta(to - from, SAMPLES)
            .num_microseconds()
            .expect("a window inside the microsecond range");
        for i in 0..=SAMPLES {
            instants.insert(from + Duration::microseconds(step * i));
        }
    }
    Series {
        knots: instants
            .into_iter()
            .map(|at| Knot {
                at,
                value: fulfillment(&spec, at, &env_at(at)),
            })
            .collect(),
        exact: breaks.exact,
    }
}
