//! A long sequence as a balanced tree, so a change's spine through it is
//! logarithmic (design §6.2).
//!
//! A node that holds a sequence as its children is a list to the fold: one
//! changed element renames that node, and the node's algebra rereads every
//! element. Grouped into a tree of CHUNKS, each chunk a node of the host's
//! own tree type carrying its children's MEASURE (their count, or any
//! monoid's sum over them, as a finger tree's nodes do), a changed element
//! renames the chunks above it and no others, and a fold rereads only those.
//!
//! The shape is a function of the elements alone. A run of nodes closes
//! after a node whose name ends a chunk (one in four, decided by the name's
//! bytes) or at [`WIDEST`] nodes, level by level until one node is left. So
//! two hosts that hold one sequence build one tree with one name, whatever
//! edits each made to reach it, and their caches share every entry; and an
//! insertion or a deletion moves the chunk boundaries next to it and no
//! others, as a replacement does. (Inside a run of more than [`WIDEST`]
//! nodes with no boundary, an insertion moves the cuts up to the run's end:
//! rare among distinct elements, and among equal ones only the last chunk
//! changes.)

use super::fold::Tree;
use crate::event::Hash;

/// The most nodes one chunk holds: a run with no boundary in it is cut
/// here, so a run of equal elements is still a tree.
pub const WIDEST: usize = 16;

/// Whether a run closes after the node named `name`: the name's last hex
/// digit is a multiple of four, so a run is four nodes long on average.
fn ends_a_chunk(name: &Hash) -> bool {
    name.as_str()
        .chars()
        .next_back()
        .and_then(|digit| digit.to_digit(16))
        .is_some_and(|digit| digit % 4 == 0)
}

/// `elements` as one balanced tree, its interior nodes made by `chunk` from
/// their children in order: the host's constructor for a run of its nodes,
/// which names the run with [`super::name`] over its children's names and
/// holds their measure. `None` for no elements, and the element itself for
/// one.
///
/// A host builds one wherever a node of its tree would hold a long
/// sequence: the node holds the sequence's balanced tree instead, and its
/// algebra reads a chunk as the monoid sum of its children (the sequence's
/// concatenation, its count, its total).
pub fn balance<T: Tree>(elements: Vec<T>, mut chunk: impl FnMut(Vec<T>) -> T) -> Option<T> {
    let mut level = elements;
    while level.len() > 1 {
        let before = level.len();
        let mut next = Vec::new();
        let mut run = Vec::new();
        for node in level {
            let ends = ends_a_chunk(node.name());
            run.push(node);
            if ends || run.len() == WIDEST {
                next.push(chunk(std::mem::take(&mut run)));
            }
        }
        if !run.is_empty() {
            next.push(chunk(run));
        }
        // Every node ended its own run, so the level did not shrink: close
        // it as one chunk rather than climb a level that may not either.
        level = if next.len() == before {
            vec![chunk(next)]
        } else {
            next
        };
    }
    level.pop()
}
