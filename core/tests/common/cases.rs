//! HOW MANY CASES A LAW RUNS: every property test's configuration, in one
//! place. A law states its FULL count, the one `nix flake check`, `main` and
//! the nightly run. `PRODROME_MAX_CASES=n` caps every law at `n` cases, so
//! the pull request loop asks each law the same question over fewer draws,
//! and no law is skipped: a cap is a positive number, and a law whose full
//! count is under it runs that count.
//!
//! Included by path, so the core's integration tests, its unit tests and the
//! mirror's tests all read this file and there is no second rule.

use proptest::test_runner::Config;

/// The variable that caps every law's cases.
pub const MAX_CASES: &str = "PRODROME_MAX_CASES";

/// A law's configuration: `full` cases, or the cap when it is lower.
///
/// # Panics
///
/// When `PRODROME_MAX_CASES` is set and is not a positive integer: a cap
/// that does not parse is a mistake to say, not a reason to run the full
/// count or none.
#[must_use]
pub fn cases(full: u32) -> Config {
    Config::with_cases(cap().map_or(full, |cap| full.min(cap)))
}

fn cap() -> Option<u32> {
    let value = std::env::var(MAX_CASES).ok()?;
    match value.trim().parse::<u32>() {
        Ok(cap) if cap > 0 => Some(cap),
        _ => panic!("{MAX_CASES} must be a positive integer, not {value:?}"),
    }
}
