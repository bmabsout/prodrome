//! Design §6.2: caches are tabulations of pure functions.
//!
//! A cache has no meaning of its own: reading through it is computing the
//! function (`memo f = f`). What it holds is an approximation of that
//! function's graph, a partial function ordered by inclusion, so it is a
//! store with one register per key in the flat order, joined by union, from
//! which any entry may be dropped ([`cache`]).
//!
//! Hierarchical caching is then one combinator, the memoised fold over a
//! content-addressed tree of any shape ([`fold()`]): a node's [`name`]
//! covers its children's names, Merkle style, the algebra is memoised at
//! every node, and what a fold looked at is a [`Report`]. A long sequence
//! is given a balanced shape ([`balance()`]) so a change's spine through it
//! is logarithmic.
//!
//! The module depends on the hash type and on nothing else of the crate: no
//! schema, no store and no FPL.

#![warn(clippy::pedantic)]

pub mod balance;
pub mod cache;
pub mod fold;

pub use balance::balance;
pub use cache::{Cache, Evict, Finding, Keep, Key, Lookup, Lru};
pub use fold::{fold, Algebra, Counts, Outcome, Report, Tree, Visit};

use crate::event::Hash;

/// A node's content name: its own bytes and its children's names, in
/// order, so it covers the whole subtree and a change anywhere below renames
/// exactly the path up to it, as a git tree's name does. Its own bytes are
/// length-prefixed and a name is fixed-width, so no two nodes print alike.
///
/// The same function names an algebra: its code's bytes, and the names of
/// what that code reads as its children.
pub fn name<'a>(own: &[u8], children: impl IntoIterator<Item = &'a Hash>) -> Hash {
    let mut bytes = format!("memo {}:", own.len()).into_bytes();
    bytes.extend_from_slice(own);
    for child in children {
        bytes.push(b' ');
        bytes.extend_from_slice(child.as_str().as_bytes());
    }
    Hash::of_bytes(&bytes)
}
