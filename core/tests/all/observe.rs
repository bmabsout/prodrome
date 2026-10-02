//! An observation is a quantisation of `[0, 1] ∪ {∅}`: monotone on numbers,
//! onto its steps' ends, and `∅` only for `∅`.

use std::num::NonZeroU32;

use prodrome::observe::{Observation, Rounding};
use proptest::prelude::*;

fn an_observation() -> impl Strategy<Value = Observation> {
    (
        1u32..=1000,
        prop::sample::select(vec![Rounding::Down, Rounding::Nearest, Rounding::Up]),
    )
        .prop_map(|(levels, rounding)| {
            Observation::new(NonZeroU32::new(levels).expect("positive"), rounding)
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// A larger value is never observed as a smaller step, so each step
    /// observes an interval of values.
    #[test]
    fn an_observation_is_monotone(
        observation in an_observation(),
        a in 0.0f64..=1.0,
        b in 0.0f64..=1.0,
    ) {
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        prop_assert!(observation.observe(Some(low)) <= observation.observe(Some(high)));
    }

    /// The ends of `[0, 1]` are the first and last steps, and `∅` is observed
    /// as itself and is the only value that is.
    #[test]
    fn an_observation_keeps_its_ends_and_absence(
        observation in an_observation(),
        x in 0.0f64..=1.0,
    ) {
        prop_assert_eq!(observation.observe(Some(0.0)), Some(0));
        prop_assert_eq!(observation.observe(Some(1.0)), Some(observation.levels().get()));
        prop_assert_eq!(observation.observe(None), None);
        prop_assert!(observation.observe(Some(x)).is_some());
    }

    /// Down and up bracket the nearest, and agree exactly on the steps' ends.
    #[test]
    fn the_roundings_bracket_one_another(levels in 1u32..=1000, x in 0.0f64..=1.0) {
        let levels = NonZeroU32::new(levels).expect("positive");
        let read = |rounding| Observation::new(levels, rounding).observe(Some(x));
        prop_assert!(read(Rounding::Down) <= read(Rounding::Nearest));
        prop_assert!(read(Rounding::Nearest) <= read(Rounding::Up));
        prop_assert!(read(Rounding::Up) <= read(Rounding::Down).map(|step| step + 1));
    }
}

#[test]
fn a_percent_is_a_hundred_steps_to_the_nearest() {
    let percent = Observation::PERCENT;
    assert_eq!(percent.levels().get(), 100);
    assert_eq!(percent.rounding(), Rounding::Nearest);
    assert_eq!(percent.observe(Some(0.424)), Some(42));
    assert_eq!(percent.observe(Some(0.4251)), Some(43));
    assert_eq!(percent.observe(Some(0.999)), Some(100));
}
