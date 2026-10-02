//! §7 — what a view observes of a fulfillment, and when that next changes.
//!
//! A view shows a value at a PRECISION (a whole percent, a pie's drawn
//! angle), so what it draws is `view ∘ observe ∘ ⟦t⟧`, a function of the
//! OBSERVED value. An [`Observation`] is that `observe`: a first-class value,
//! the unit interval cut into equal steps with a stated rounding, composed
//! with a behaviour rather than a constant inside one.

use std::num::NonZeroU32;

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
