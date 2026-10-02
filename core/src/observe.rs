//! §7 — what a view observes of a fulfillment, and when that next changes.
//!
//! A view shows a value at a PRECISION (a whole percent, a pie's drawn
//! angle), so what it draws is `view ∘ observe ∘ ⟦t⟧`, a function of the
//! OBSERVED value. An [`Observation`] is that `observe`: a first-class value,
//! the unit interval cut into equal steps with a stated rounding, composed
//! with a behaviour rather than a constant inside one.

use std::collections::BTreeSet;
use std::num::NonZeroU32;

use chrono::{Duration, Timelike};

use crate::breaks::{breakpoints, Breaks};
use crate::fpl::{eval, Closed, Delta, Env, Instant};
use crate::term::Term;

/// How a value between two steps is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rounding {
    /// The step at or below: `⌊x·n⌋`.
    Down,
    /// The nearest step, a half read upward: `⌊x·n + ½⌋`, JavaScript's
    /// `Math.round` on `[0, n]`.
    Nearest,
    /// The step at or above: `⌈x·n⌉`.
    Up,
}

/// A quantisation of `[0, 1] ∪ {∅}`: `levels` equal steps, a value read as the
/// step `rounding` takes it to, and `∅` read as itself.
///
/// Laws (`tests/all/observe.rs`): `observe` is monotone on numbers, reads `0`
/// as step 0 and `1` as step `levels`, and reads `∅`, and only `∅`, as `∅`.
/// So the set of values one step observes is an interval, which is what lets
/// a behaviour's observed value be a step function of time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Observation {
    levels: NonZeroU32,
    rounding: Rounding,
}

impl Observation {
    /// A whole percent, rounded to the nearest: what a list row prints.
    pub const PERCENT: Observation = Observation {
        levels: match NonZeroU32::new(100) {
            Some(levels) => levels,
            None => unreachable!(),
        },
        rounding: Rounding::Nearest,
    };

    /// `[0, 1]` cut into `levels` steps, read by `rounding`. Total: a zero
    /// step count is not a value of the argument's type.
    #[must_use]
    pub const fn new(levels: NonZeroU32, rounding: Rounding) -> Observation {
        Observation { levels, rounding }
    }

    #[must_use]
    pub const fn levels(self) -> NonZeroU32 {
        self.levels
    }

    #[must_use]
    pub const fn rounding(self) -> Rounding {
        self.rounding
    }

    /// The step `value` is observed as, `None` for `∅`. A number outside
    /// `[0, 1]` (no term reads one) is read as the nearer end.
    #[must_use]
    pub fn observe(self, value: Option<f64>) -> Option<u32> {
        let scaled = value?.clamp(0.0, 1.0) * f64::from(self.levels.get());
        let step = match self.rounding {
            Rounding::Down => scaled.floor(),
            Rounding::Nearest => (scaled + 0.5).floor(),
            Rounding::Up => scaled.ceil(),
        };
        // `step` is a whole number in `[0, levels]`, so the cast is exact.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(step as u32)
    }
}

/// When a behaviour's observed value next changes after an instant.
///
/// `at` is `None` where the observed value never changes again. `exact` says
/// which kind of answer `at` is: the observed value's NEXT STEP, where it
/// differs from the value now, or a CONSERVATIVE bound, never later than the
/// next step and possibly earlier. An early answer costs a redraw that changes
/// nothing; a late one would show a stale number, so no answer is late.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NextChange {
    pub at: Option<Instant>,
    pub exact: bool,
}

/// The first instant after `now` at which `observation` of `⟦term⟧(·, env)`
/// differs from its observation at `now`: the push half of push-pull, a
/// behaviour naming its next discontinuity so a client redraws then and not
/// on a ticking clock. Time is microseconds, the evaluator's own grain.
///
/// `env` is a snapshot, read as [`crate::fpl::fulfillment`] reads it: a binding or a
/// tending dated after `now` comes into force at its instant, so it is a
/// change this answer sees. History not yet in `env` is a new input, not time.
///
/// EXACT on the exact fragment (§7 breakpoints: `Flat`, `Decay`, `Curve`,
/// `Absent`, and `Piecewise`, `Offset`, `Shift` and `Least` over them, and
/// constant composites). There the term is affine between its breakpoints,
/// read off each atom's closed form (a `Decay`'s window, a `Curve`'s points,
/// a `Piecewise`'s knots, two lines' crossing), so its observation is
/// monotone between them, and the step inside one is found by bisection
/// against the evaluator itself: the answer agrees with [`crate::fpl::fulfillment`] bit
/// for bit rather than with a second formula. Past the last breakpoint an
/// exact term is constant, so it never changes there.
///
/// CONSERVATIVE everywhere else, where the evaluator samples or reads history.
#[must_use]
pub fn next_change(term: &Closed, now: Instant, env: &Env, observation: Observation) -> NextChange {
    let breaks = breakpoints(term.normalize().term());
    if breaks.exact {
        NextChange {
            at: next_step(term.term(), now, env, observation, &breaks),
            exact: true,
        }
    } else {
        NextChange {
            at: Some(grain(now) + Duration::microseconds(1)),
            exact: false,
        }
    }
}

/// The exact answer: segment by segment between the breakpoints after `now`.
fn next_step(
    term: &Term,
    now: Instant,
    env: &Env,
    observation: Observation,
    breaks: &Breaks,
) -> Option<Instant> {
    let here = observation.observe(eval(term, now, env));
    let base = grain(now);
    let at = |us: i64| base + Duration::microseconds(us);
    let differs = |us: i64| observation.observe(eval(term, at(us), env)) != here;
    let cuts: BTreeSet<Instant> = breaks
        .slopes
        .iter()
        .chain(&breaks.jumps)
        .copied()
        .filter(|cut| *cut > now)
        .collect();
    let mut start = 0;
    for cut in cuts {
        let end = microseconds(cut - base);
        // Between two breakpoints the term is affine, so its observation is
        // monotone there; a breakpoint itself is read on its own, since a
        // jump or a crossing lands on it.
        if let Some(us) = first_in(start + 1, end - 1, differs) {
            return Some(at(us));
        }
        if differs(end) {
            return Some(at(end));
        }
        start = end;
    }
    // Constant past the last breakpoint.
    differs(start + 1).then(|| at(start + 1))
}

/// The first `us` in `[low, high]` where `differs`, which is false and then
/// true across the range.
fn first_in(low: i64, high: i64, differs: impl Fn(i64) -> bool) -> Option<i64> {
    if low > high {
        return None;
    }
    if differs(low) {
        return Some(low);
    }
    if !differs(high) {
        return None;
    }
    let (mut same, mut changed) = (low, high);
    while changed - same > 1 {
        let middle = same + (changed - same) / 2;
        if differs(middle) {
            changed = middle;
        } else {
            same = middle;
        }
    }
    Some(changed)
}

/// `now` on the microsecond grid, at or before it: every instant a step is
/// looked for at is `grain(now)` plus whole microseconds, so the first is
/// after `now` and a breakpoint, itself on the grid, is one of them.
fn grain(now: Instant) -> Instant {
    now - Duration::nanoseconds(i64::from(now.nanosecond() % 1000))
}

fn microseconds(span: Delta) -> i64 {
    span.num_microseconds()
        .expect("a span between two of a term's instants fits in microseconds")
}
