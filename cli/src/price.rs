//! The command line's two shapes of price, as §7 terms, and the one place an
//! instant is read.

use prodrome::fpl::{
    self, datetime_of, delta_from_hours, instant_of, mk_decay, mk_flat, Instant, Term,
};
use prodrome::literal::Datetime;

use crate::command::Price;
use crate::Error;

/// THE SCALE. A priority is a percentage of fulfillment, so LOW IS URGENT —
/// the number a person types and the value the fold computes are the same
/// number, and there is no second scale to invert.
const SCALE: f64 = 100.0;

/// The hour a `--deadline` day means. A date is not an instant, and picking
/// one here — once, stated — is better than every caller picking midnight and
/// then wondering why a Tuesday deadline bit on Monday night.
const DEADLINE_HOUR: u32 = 17;

fn percent(name: &str, n: u8) -> Result<f64, Error> {
    if n > 100 {
        return Err(Error::Usage(format!("{name} must be 0–100, got {n}")));
    }
    Ok(f64::from(n) / SCALE)
}

/// A `--priority`/`--deadline` pair as the term it names, or `None` when the
/// caller named neither.
///
/// Clap has already refused both at once (the `price` group), so the shape
/// here is a choice and not a precedence.
pub fn term_of(price: &Price) -> Result<Option<Term>, Error> {
    match (price.priority, price.deadline.as_deref()) {
        (Some(n), _) => Ok(Some(mk_flat(percent("--priority", n)?)?)),
        (None, Some(day)) => {
            let end_date = deadline_instant(day)?;
            if !(price.lead_up_days.is_finite() && price.lead_up_days > 0.0) {
                return Err(Error::Usage(format!(
                    "--lead-up-days must be positive, got {}",
                    price.lead_up_days
                )));
            }
            Ok(Some(mk_decay(
                percent("--start", price.start)?,
                percent("--end", price.end)?,
                end_date,
                delta_from_hours(price.lead_up_days * 24.0),
                None,
            )?))
        }
        (None, None) => Ok(None),
    }
}

/// `YYYY-MM-DD` at [`DEADLINE_HOUR`]. A time of day in the argument is
/// refused rather than honoured: `--deadline` takes a day, and admitting two
/// spellings would make the hour above sometimes true.
fn deadline_instant(day: &str) -> Result<Instant, Error> {
    if day.len() != 10 || !day.is_char_boundary(10) {
        return Err(Error::Usage(format!(
            "--deadline takes a day, YYYY-MM-DD, got {day:?}"
        )));
    }
    let midnight = fpl::parse_iso(day)?;
    Ok(midnight + fpl::delta_from_hours(f64::from(DEADLINE_HOUR)))
}

/// An ISO instant off the command line, or the machine's clock.
///
/// THE ONLY READ OF THE CLOCK IN THIS CRATE. The core has none: an event's
/// `at` is data, and folding is a query at an instant the caller names. A host
/// still has to name one when the caller did not, and this is where it does.
pub fn instant(named: Option<&str>) -> Result<Datetime, Error> {
    match named {
        Some(text) => Ok(datetime_of(fpl::parse_iso(text)?)?),
        None => Ok(datetime_of(chrono::Local::now().naive_local())?),
    }
}

/// An instant as the string every boundary in this repository spells it with.
pub fn iso(at: Datetime) -> String {
    fpl::iso(instant_of(at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use prodrome::fpl::{fulfillment, print_term};
    use std::collections::BTreeMap;

    fn price() -> Price {
        Price {
            priority: None,
            deadline: None,
            start: 55,
            end: 5,
            lead_up_days: 3.0,
        }
    }

    #[test]
    fn a_priority_is_a_flat_term_at_that_percentage() {
        let term = term_of(&Price {
            priority: Some(70),
            ..price()
        })
        .expect("70 is a priority")
        .expect("a priority is a price");
        assert_eq!(print_term(&term), "Flat(value=0.7)");
    }

    #[test]
    fn low_is_urgent() {
        let env = BTreeMap::new();
        let now = fpl::parse_iso("2026-09-10T09:00:00").expect("an instant");
        let urgent = term_of(&Price {
            priority: Some(10),
            ..price()
        })
        .expect("valid")
        .expect("a price");
        let parked = term_of(&Price {
            priority: Some(90),
            ..price()
        })
        .expect("valid")
        .expect("a price");
        assert!(fulfillment(&urgent, now, &env) < fulfillment(&parked, now, &env));
    }

    #[test]
    fn a_deadline_is_a_decay_onto_five_in_the_evening() {
        let term = term_of(&Price {
            deadline: Some("2026-09-15".to_owned()),
            ..price()
        })
        .expect("a day")
        .expect("a deadline is a price");
        assert_eq!(
            print_term(&term),
            "Decay(start=0.55, end=0.05, end_date=datetime(2026, 9, 15, 17, 0, 0), lead_up=timedelta(days=3), start_date=None)"
        );
    }

    #[test]
    fn a_deadline_bites_as_it_approaches() {
        let env = BTreeMap::new();
        let term = term_of(&Price {
            deadline: Some("2026-09-15".to_owned()),
            ..price()
        })
        .expect("valid")
        .expect("a price");
        let far = fpl::parse_iso("2026-09-01T09:00:00").expect("an instant");
        let near = fpl::parse_iso("2026-09-15T09:00:00").expect("an instant");
        assert!(fulfillment(&term, near, &env) < fulfillment(&term, far, &env));
    }

    #[test]
    fn neither_flag_is_no_price_and_not_a_zero() {
        assert!(term_of(&price()).expect("no price is allowed").is_none());
    }

    #[test]
    fn a_percentage_above_a_hundred_is_refused() {
        assert!(term_of(&Price {
            priority: Some(101),
            ..price()
        })
        .is_err());
    }

    #[test]
    fn a_deadline_with_a_time_of_day_is_refused() {
        assert!(term_of(&Price {
            deadline: Some("2026-09-15T09:00:00".to_owned()),
            ..price()
        })
        .is_err());
    }

    #[test]
    fn an_instant_is_a_day_or_a_moment() {
        assert_eq!(
            iso(instant(Some("2026-09-09")).expect("a day")),
            "2026-09-09T00:00:00"
        );
        assert_eq!(
            iso(instant(Some("2026-09-09T17:30:00")).expect("a moment")),
            "2026-09-09T17:30:00"
        );
        assert!(instant(Some("yesterday")).is_err());
    }
}
