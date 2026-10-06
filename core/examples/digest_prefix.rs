//! How much of a digest's printed view survives one more line.
//!
//! A model reading a view is prompted with it printed, part after part, and
//! its prompt cache serves the longest prefix it has seen before. So what
//! a line costs a reader is the view's printed bytes after the first part
//! the line changed. Over a log of `LINES` lines, each of 20 to 200 bytes,
//! every settled node summarised in 60 to 160, this prints, for each
//! budget, the share of the view's bytes that is a prefix of the next
//! view's, over `APPENDS` appends ending at the log's last line.
//!
//!     cargo run --release -p prodrome-core --example digest_prefix

use prodrome::digest::{tree, view, Leaf, View};
use prodrome::event::Hash;
use prodrome::memo;

const LINES: usize = 10_000;
const APPENDS: usize = 200;

/// A number in `low..=high` drawn from a name.
fn draw(name: &Hash, salt: &[u8], low: u32, high: u32) -> u32 {
    let hash = memo::name(salt, [name]);
    let value = u32::from_str_radix(&hash.as_str()[..8], 16).expect("hex");
    low + value % (high - low + 1)
}

fn summary(name: &Hash) -> Option<u32> {
    Some(draw(name, b"summary", 60, 160))
}

/// The bytes of `before` printed up to its first part that `after` does
/// not print in the same place.
fn kept(before: &View, after: &View) -> u64 {
    let lines = |part: &prodrome::digest::Part| part.span.len() == 1;
    before
        .parts
        .iter()
        .zip(&after.parts)
        .take_while(|(old, new)| old == new)
        .map(|(part, _)| {
            if lines(part) {
                u64::from(draw(&part.node, b"line", 20, 200))
            } else {
                u64::from(summary(&part.node).expect("summarised"))
            }
        })
        .sum()
}

fn main() {
    let log: Vec<Leaf<()>> = (0..LINES)
        .map(|at| {
            let name = memo::name(b"line", [&memo::name(&at.to_be_bytes(), [])]);
            Leaf {
                bytes: draw(&name, b"line", 20, 200),
                name,
                measure: (),
            }
        })
        .collect();
    for budget in [4_096, 16_384, 65_536] {
        let mut shares = Vec::new();
        let mut whole = 0;
        let mut previous = None;
        for lines in LINES - APPENDS..=LINES {
            let tree = tree(log[..lines].to_vec()).expect("lines");
            let now = view(&tree, summary, budget);
            if let Some(before) = previous.replace(now.clone()) {
                let kept = kept(&before, &now);
                if kept == before.bytes {
                    whole += 1;
                }
                #[allow(clippy::cast_precision_loss)]
                shares.push(kept as f64 / before.bytes as f64);
            }
        }
        shares.sort_by(f64::total_cmp);
        #[allow(clippy::cast_precision_loss)]
        let mean = shares.iter().sum::<f64>() / shares.len() as f64;
        println!(
            "budget {budget:>6}: prefix kept mean {:.1}%, median {:.1}%, least {:.1}%, whole in {whole} of {}",
            100.0 * mean,
            100.0 * shares[shares.len() / 2],
            100.0 * shares[0],
            shares.len(),
        );
    }
}
