//! Design §6.2: caches are tabulations of pure functions.
//!
//! A cache has no meaning of its own: reading through it is computing the
//! function (`memo f = f`). What it holds is an approximation of that
//! function's graph, a partial function ordered by inclusion, so it is a
//! store with one register per key in the flat order, joined by union, from
//! which any entry may be dropped ([`cache`]).
//!
//! The module depends on the hash type and on nothing else of the crate: no
//! schema, no store and no FPL.

#![warn(clippy::pedantic)]

pub mod cache;

pub use cache::{Cache, Evict, Finding, Keep, Key, Lookup, Lru};
