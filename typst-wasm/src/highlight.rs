//! `highlight`: the editor's syntax colouring.

use serde::Serialize;
use typst::syntax::{highlight as tag_of, LinkedNode, Source, Tag};
use wasm_bindgen::prelude::*;

use crate::json;
use crate::offsets::Offsets;

#[derive(Serialize, PartialEq, Debug)]
struct Highlight {
    from: usize,
    to: usize,
    tag: &'static str,
}

/// Every leaf's tag, taking the innermost tagged node it sits in — the
/// nesting `typst_syntax::highlight_html` prints, flattened into spans that do
/// not overlap, so a page can colour them without building a tree.
fn spans(source: &Source) -> Vec<Highlight> {
    fn walk(
        node: &LinkedNode,
        inherited: Option<&'static str>,
        offsets: &Offsets,
        out: &mut Vec<Highlight>,
    ) {
        let tag = tag_of(node).map(Tag::css_class).or(inherited);
        if node.children().len() == 0 {
            let range = node.range();
            if let (Some(tag), false) = (tag, range.is_empty()) {
                let (from, to) = (offsets.utf16(range.start), offsets.utf16(range.end));
                match out.last_mut() {
                    Some(last) if last.tag == tag && last.to == from => last.to = to,
                    _ => out.push(Highlight { from, to, tag }),
                }
            }
            return;
        }
        for child in node.children() {
            walk(&child, tag, offsets, out);
        }
    }
    let offsets = Offsets::of(source.text());
    let mut out = Vec::new();
    walk(&LinkedNode::new(source.root()), None, &offsets, &mut out);
    out
}

/// The syntax highlighting of `source`: `[{from, to, tag}]`, UTF-16, sorted
/// and disjoint, `tag` being Typst's own CSS class (`typ-key`, `typ-str`, …).
#[wasm_bindgen]
pub fn highlight(source: &str) -> String {
    json(&spans(&Source::detached(source)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlighting_is_sorted_disjoint_and_in_units() {
        let source = "#let ü = \"x\"";
        let spans = spans(&Source::detached(source));
        assert!(spans.windows(2).all(|w| w[0].to <= w[1].from));
        let units = source.encode_utf16().count();
        assert!(spans.iter().all(|s| s.from < s.to && s.to <= units));
        assert!(spans.iter().any(|s| s.tag == "typ-key"));
        assert!(spans.iter().any(|s| s.tag == "typ-str" && s.to == units));
    }
}
