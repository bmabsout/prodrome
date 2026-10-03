//! Design §6.3: a prodrome of prodromes is a prodrome.
//!
//! A register's value may be a history: a conversation inside the store of
//! conversations, a card's state inside the page's, an agent's batch beside
//! the chain it targets. Such a register holds its history BY ITS HEADS
//! ([`Heads`]), as a git tree holds a subtree by its name, and a schema
//! says which of its registers do ([`Nests`]).
//!
//! Histories nest, and a nest FLATTENS into one history keyed by path,
//! which is a monad ([`History`]) built from the free monoid of paths
//! ([`Path`]) and the finite powerset. A [`Nest`], read from each level's
//! replica ([`Level`]), is a tree of content-addressed histories: its join
//! renames no object and rewrites no dep, causality crosses its levels only
//! where a pointer names what it saw ([`Nest::past`]), and the memoised
//! fold folds it as any other tree.

#![warn(clippy::pedantic)]

mod heads;
mod history;
mod level;
mod path;

pub use heads::{pointer, Heads, Nests};
pub use history::History;
pub use level::{Holding, Leaf, Level, Nest};
pub use path::{Path, Segment};
