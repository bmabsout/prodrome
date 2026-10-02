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
use crate::fpl::{
    div_delta, eval, in_force, last_tended, offset, phase, power_mean, Closed, Delta, Env, Instant,
    Outcome, WITHIN_SAMPLES,
};
use crate::term::{normalize, Term, TermF};

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
///
/// MONOTONICITY IS THE LAW [`next_change`] RESTS ON, and the only one: a
/// monotone map of an affine stretch is monotone, so bisection finds its
/// step, and a monotone map of an interval is one step exactly when its two
/// ends are. Any finer or coarser cut of `[0, 1]` is the same type with a
/// different `levels`; the observation is an argument, never a constant of
/// the term or the view.
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
/// which kind of answer `at` is, a fact about the behaviour and not about how
/// the term is written: the observed value's NEXT STEP, where it differs from
/// the value now (or, `None`, a proof that it never does), or a CONSERVATIVE
/// bound, never later than the next step and possibly earlier. An early
/// answer costs a redraw that changes nothing; a late one would show a stale
/// number, so no answer is late.
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
/// CONSERVATIVE everywhere else, where the evaluator samples (`Within`, a
/// composite of moving parts) or reads history (`After`, `Recur`), or folds
/// time (`Periodic`). There the answer is the first instant at which an
/// ENCLOSURE of the term over `[now, t]` (an interval of numbers, and whether
/// `∅` is possible) stops being one observed value: each atom's exact range
/// over the stretch, a monotone composite applied to its children's bounds,
/// a window widened by its span, a lookup split where a binding or a tending
/// comes into force, a cycle folded onto its period. Up to `t` the enclosure
/// proves the observed value has not moved, so the answer is never late; it
/// is early where the enclosure is loose (members moving in opposite
/// directions). An enclosure over all time that is one observed value
/// answers never, exactly. The search looks about a thousand years ahead (2⁵⁵ µs) and
/// answers that horizon if nothing ends sooner, which is early, not late.
///
/// The enclosure applies the evaluator's own float operations to bounds, so
/// it is sound wherever those are monotone: `+`, `·`, `/`, `min` and `max`
/// are, as correctly rounded; `powf`, `exp` and `ln` are faithfully rounded
/// on every platform this builds for, though IEEE does not promise it.
#[must_use]
pub fn next_change(term: &Closed, now: Instant, env: &Env, observation: Observation) -> NextChange {
    let breaks = breakpoints(term.normalize().term());
    if breaks.exact {
        NextChange {
            at: next_step(term.term(), now, env, observation, &breaks),
            exact: true,
        }
    } else {
        // An enclosure over all time that is one value proves "never", which
        // is then the step itself.
        let at = bounded(term.term(), now, env, observation);
        NextChange {
            at,
            exact: at.is_none(),
        }
    }
}

/// How far ahead, in powers of two of a microsecond, the conservative search
/// looks: 2⁵⁵ µs is about 1142 years.
const HORIZON: u32 = 55;

/// The conservative answer: the first microsecond `t` at which the enclosure
/// over `[now, t]` is not one observed value, found by doubling and then
/// bisection. Sound whatever the enclosure's tightness: the answer `t` is
/// returned only where `[now, t − 1µs]` was proved one value, or `t` is the
/// first microsecond after `now`.
fn bounded(term: &Term, now: Instant, env: &Env, observation: Observation) -> Option<Instant> {
    let here = observation.observe(eval(term, now, env));
    let holds =
        |to: Option<Instant>| range(term, Span { from: now, to }, env).is(observation, here);
    if holds(None) {
        return None;
    }
    let base = grain(now);
    let at = |us: i64| base + Duration::microseconds(us);
    let holds_until = |us: i64| holds(Some(at(us)));
    // `same` is proved one value, or is 0, which nothing needs proved.
    let mut same = 0;
    for bit in 0..=HORIZON {
        let changed = 1i64 << bit;
        if !holds_until(changed) {
            let mut changed = changed;
            while changed - same > 1 {
                let middle = same + (changed - same) / 2;
                if holds_until(middle) {
                    same = middle;
                } else {
                    changed = middle;
                }
            }
            return Some(at(changed));
        }
        same = changed;
    }
    Some(at(same))
}

/// A stretch of time, `[from, to]`, for ever where `to` is `None`.
#[derive(Debug, Clone, Copy)]
struct Span {
    from: Instant,
    to: Option<Instant>,
}

impl Span {
    fn holds(self, at: Instant) -> bool {
        self.from <= at && self.to.is_none_or(|to| at <= to)
    }

    fn shifted(self, by: Delta) -> Span {
        Span {
            from: self.from + by,
            to: self.to.map(|to| to + by),
        }
    }

    /// The stretch before `at` and the stretch from it, either possibly empty.
    fn split(self, at: Instant) -> (Option<Span>, Option<Span>) {
        if at <= self.from {
            (None, Some(self))
        } else if self.to.is_some_and(|to| at > to) {
            (Some(self), None)
        } else {
            (
                Some(Span {
                    from: self.from,
                    to: Some(at - Duration::microseconds(1)),
                }),
                Some(Span { from: at, ..self }),
            )
        }
    }

    /// The stretch cut where each of `changes` comes into force, each part
    /// with what is in force over it: `first` until the first change.
    fn parts<T: Copy>(
        self,
        first: T,
        changes: impl IntoIterator<Item = (Instant, T)>,
    ) -> Vec<(T, Span)> {
        let mut parts = vec![];
        let (mut current, mut rest) = (first, Some(self));
        for (at, next) in changes {
            let Some(span) = rest else { break };
            if at <= span.from {
                current = next;
                continue;
            }
            let (before, after) = span.split(at);
            parts.extend(before.map(|before| (current, before)));
            (current, rest) = (next, after);
        }
        parts.extend(rest.map(|span| (current, span)));
        parts
    }
}

/// What a term can read over a stretch: whether `∅` is possible, and the
/// least and greatest number, `None` where it is `∅` throughout.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Range {
    absent: bool,
    values: Option<(f64, f64)>,
}

impl Range {
    const ABSENT: Range = Range {
        absent: true,
        values: None,
    };

    fn point(value: Option<f64>) -> Range {
        value.map_or(Range::ABSENT, |value| Range {
            absent: false,
            values: Some((value, value)),
        })
    }

    fn hull(self, other: Range) -> Range {
        Range {
            absent: self.absent || other.absent,
            values: hull_values(self.values, other.values),
        }
    }

    /// Through a function that is monotone increasing on `[0, 1]`.
    fn map(self, f: impl Fn(f64) -> f64) -> Range {
        Range {
            values: self.values.map(|(low, high)| (f(low), f(high))),
            ..self
        }
    }

    fn map_high(self, f: impl Fn(f64) -> f64) -> Range {
        Range {
            values: self.values.map(|(low, high)| (low, f(high))),
            ..self
        }
    }

    /// Whether every reading in the range is observed as `step`. Monotone
    /// observation makes the two ends enough.
    fn is(self, observation: Observation, step: Option<u32>) -> bool {
        match (step, self.values) {
            (None, values) => values.is_none(),
            (Some(_), None) => false,
            (Some(_), Some((low, high))) => {
                !self.absent
                    && observation.observe(Some(low)) == step
                    && observation.observe(Some(high)) == step
            }
        }
    }
}

fn hull_values(one: Option<(f64, f64)>, other: Option<(f64, f64)>) -> Option<(f64, f64)> {
    match (one, other) {
        (Some((a, b)), Some((c, d))) => Some((a.min(c), b.max(d))),
        (one, other) => one.or(other),
    }
}

fn hull_of(ranges: impl IntoIterator<Item = Range>) -> Range {
    ranges
        .into_iter()
        .reduce(Range::hull)
        .expect("a stretch has a part")
}

/// The enclosure of `term` over `span`: exact on the exact fragment, and
/// composed from the children's enclosures above it.
fn range(term: &Term, span: Span, env: &Env) -> Range {
    let breaks = breakpoints(&normalize(term));
    if breaks.exact {
        return exact_range(term, span, env, &breaks);
    }
    match term.out() {
        TermF::Conj { terms, p } => conj_range(terms.iter().map(|t| range(t, span, env)), *p),
        TermF::Least { terms } => least_range(terms.iter().map(|t| range(t, span, env))),
        TermF::Offset { delta, term } => range(term, span, env).map(|x| offset(x, *delta)),
        TermF::Importance { w, term } => range(term, span, env).map(|x| x.powf(*w)),
        TermF::Shift { delta, term } => range(term, span.shifted(*delta), env),
        // An absent gate is no gate; otherwise `max(1 − gate, body)`,
        // antitone in the gate and monotone in the body.
        TermF::Gate { gate, body } => modulated(
            range(gate, span, env),
            range(body, span, env),
            |(g_low, g_high), (b_low, b_high)| {
                ((1.0 - g_high).max(b_low), (1.0 - g_low).max(b_high))
            },
        ),
        // An absent delta is no offset; `offset` is monotone in both.
        TermF::OffsetBy { delta, term } => modulated(
            range(delta, span, env),
            range(term, span, env),
            |(d_low, d_high), (x_low, x_high)| (offset(x_low, d_low), offset(x_high, d_high)),
        ),
        // Every sample of every instant of the span lies in the span widened
        // by the window, and a power mean lies between its least and greatest
        // member, each clamped up to 0.001.
        TermF::Within { window, term, .. } => {
            let reach = Duration::microseconds(
                microseconds(div_delta(*window, WITHIN_SAMPLES)) * WITHIN_SAMPLES,
            );
            let widened = Span {
                to: span.to.map(|to| to + reach),
                ..span
            };
            range(term, widened, env).map_high(|high| high.max(0.001))
        }
        TermF::After {
            event,
            anchor,
            term,
            pending,
            ..
        } => after_range(event, *anchor, term, pending, span, env),
        // `pending` until the first tending, and after each the term re-anchored to it.
        TermF::Recur {
            todo,
            anchor,
            term,
            pending,
        } => {
            let tendings = env.tended.get(todo).into_iter().flatten();
            hull_of(
                span.parts(
                    last_tended(env, todo, span.from),
                    tendings.map(|at| (*at, Some(*at))),
                )
                .into_iter()
                .map(|(tended, part)| match tended {
                    None => range(pending, part, env),
                    Some(tended) => range(term, part.shifted(*anchor - tended), env),
                }),
            )
        }
        TermF::Periodic {
            period,
            anchor,
            term,
        } => hull_of(
            cycle(*period, *anchor, span)
                .into_iter()
                .map(|part| range(term, part, env)),
        ),
        TermF::Piecewise { head, pieces } => {
            let first = in_force(head, pieces, span.from).1;
            hull_of(
                span.parts(first, pieces.iter().map(|(at, piece)| (*at, piece)))
                    .into_iter()
                    .map(|(piece, part)| range(piece, part, env)),
            )
        }
        // Exact, and taken above.
        TermF::Flat { .. } | TermF::Decay { .. } | TermF::Curve { .. } | TermF::Absent => {
            exact_range(term, span, env, &breaks)
        }
        TermF::Ref { todo } => unreachable!("Ref({todo:?}) inside a Closed term"),
    }
}

/// A term `of` modulated by `by`, which is no modulation where `by` is `∅`:
/// `f` takes the two bounds to the result's, so it must be monotone in each
/// end it reads. `∅` where `of` is.
fn modulated(by: Range, of: Range, f: impl Fn((f64, f64), (f64, f64)) -> (f64, f64)) -> Range {
    let modulated = by.values.zip(of.values).map(|(by, of)| f(by, of));
    Range {
        absent: of.absent,
        values: if by.absent {
            hull_values(of.values, modulated)
        } else {
            modulated
        },
    }
}

/// `After`: the least over the candidates, each `pending` until its binding
/// comes into force and its reading from then.
fn after_range(
    event: &str,
    anchor: Instant,
    term: &Term,
    pending: &Term,
    span: Span,
    env: &Env,
) -> Range {
    let Some(candidates) = env.outcomes.get(event) else {
        return range(pending, span, env);
    };
    least_range(candidates.iter().map(|candidate| {
        let Some(outcome) = candidate else {
            return range(pending, span, env);
        };
        hull_of(
            span.parts(None, [(outcome.at(), Some(*outcome))])
                .into_iter()
                .map(|(binding, part)| match binding {
                    None => range(pending, part, env),
                    Some(Outcome::Completed(done)) => range(term, part.shifted(anchor - done), env),
                    Some(Outcome::Cancelled(_)) => Range::point(Some(1.0)),
                }),
        )
    }))
}

/// The stretches of one cycle `span` folds onto: all of it, one stretch of
/// it, or two where the span wraps.
fn cycle(period: Delta, anchor: Instant, span: Span) -> Vec<Span> {
    let last = anchor + period - Duration::microseconds(1);
    let stretch = |from, to| Span { from, to: Some(to) };
    match span.to {
        Some(to) if to - span.from < period => {
            let start = phase(period, anchor, span.from);
            let stop = start + (to - span.from);
            if stop <= last {
                vec![stretch(start, stop)]
            } else {
                vec![stretch(start, last), stretch(anchor, stop - period)]
            }
        }
        _ => vec![stretch(anchor, last)],
    }
}

/// An exact term over a stretch: affine between its breakpoints, so its
/// least and greatest readings are at the stretch's ends and at, or a
/// microsecond either side of, a breakpoint inside it.
fn exact_range(term: &Term, span: Span, env: &Env, breaks: &Breaks) -> Range {
    let one = Duration::microseconds(1);
    let mut at: BTreeSet<Instant> = span.to.into_iter().chain([span.from]).collect();
    for cut in breaks.slopes.iter().chain(&breaks.jumps) {
        at.extend(
            [*cut - one, *cut, *cut + one]
                .into_iter()
                .filter(|t| span.holds(*t)),
        );
    }
    hull_of(at.into_iter().map(|t| Range::point(eval(term, t, env))))
}

/// `Conj`: the power mean of the members that have a value. With every member
/// valued throughout, it is monotone in each; otherwise it lies between the
/// least and greatest of whichever are present, each clamped up to 0.001.
fn conj_range(members: impl Iterator<Item = Range>, p: f64) -> Range {
    let members: Vec<Range> = members.collect();
    if members.is_empty() {
        return Range::point(Some(power_mean(&[], p)));
    }
    let absent = members.iter().all(|member| member.absent);
    let bounds: Vec<(f64, f64)> = members.iter().filter_map(|member| member.values).collect();
    if bounds.is_empty() {
        return Range::ABSENT;
    }
    let values = if members.iter().any(|member| member.absent) {
        (
            bounds.iter().map(|b| b.0).fold(f64::INFINITY, f64::min),
            bounds.iter().map(|b| b.1).fold(0.001, f64::max),
        )
    } else {
        let (lows, highs): (Vec<f64>, Vec<f64>) = bounds.into_iter().unzip();
        (power_mean(&lows, p), power_mean(&highs, p))
    };
    Range {
        absent,
        values: Some(values),
    }
}

/// `Least`: the least of the members that have a value. Its floor is the
/// least floor; its ceiling the least ceiling of the members valued
/// throughout, which are always among those present, or failing one the
/// greatest ceiling.
fn least_range(members: impl Iterator<Item = Range>) -> Range {
    let members: Vec<Range> = members.collect();
    let bounds: Vec<(f64, f64)> = members.iter().filter_map(|member| member.values).collect();
    if bounds.is_empty() {
        return Range::ABSENT;
    }
    let low = bounds.iter().map(|b| b.0).fold(f64::INFINITY, f64::min);
    let always: Vec<f64> = members
        .iter()
        .filter(|member| !member.absent)
        .filter_map(|member| member.values.map(|b| b.1))
        .collect();
    let high = if always.is_empty() {
        bounds.iter().map(|b| b.1).fold(f64::NEG_INFINITY, f64::max)
    } else {
        always.into_iter().fold(f64::INFINITY, f64::min)
    };
    Range {
        absent: members.iter().all(|member| member.absent),
        values: Some((low, high)),
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
