//! Design §6.3: a prodrome of prodromes is a prodrome.
//!
//! A register's value may be a history: a conversation inside the store of
//! conversations, a card's state inside the page's, an agent's batch beside
//! the chain it targets. Histories nest, and a nest FLATTENS into one
//! history keyed by path, which is a monad ([`History`]) built from the
//! free monoid of paths ([`Path`]) and the finite powerset.

#![warn(clippy::pedantic)]

mod history;
mod path;

pub use history::History;
pub use path::{Path, Segment};
