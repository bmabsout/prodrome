//! `complete` and `hover`: the editor's completion and tooltips, from
//! typst-ide.

use serde::Serialize;
use typst::syntax::{Side, Source};
use typst::World;
use typst_html::HtmlDocument;
use typst_ide::{Completion, CompletionKind, Tooltip};
use wasm_bindgen::prelude::*;

use crate::offsets::Offsets;
use crate::world::Sandbox;
use crate::{json, MAIN};

#[derive(Serialize)]
struct Item {
    label: String,
    kind: CompletionKind,
    /// The text to insert, snippet placeholders removed.
    insert: String,
    /// Where the cursor goes within `insert` (UTF-16): at its first
    /// placeholder, or at its end.
    cursor: usize,
    detail: Option<String>,
}

#[derive(Serialize)]
struct Completions {
    /// The UTF-16 offset the completions replace from, up to the cursor.
    from: usize,
    items: Vec<Item>,
}

#[derive(Serialize)]
struct Hover {
    kind: &'static str,
    text: String,
}

/// A world of `source` alone, as `/main.typ`, and its parse.
fn alone(source: &str) -> Option<(Sandbox, Source)> {
    let mut world = Sandbox::new();
    let id = world.set(MAIN, source).ok()?;
    world.prepare(MAIN).ok()?;
    let main = world.source(id).ok()?;
    Some((world, main))
}

/// A completion's `apply` is snippet syntax — `${name}` for a placeholder,
/// `${}` for an empty one. A textarea has no placeholders, so they are
/// removed, and the cursor goes where the first one was.
fn unsnippet(apply: &str) -> (String, usize) {
    let mut text = String::new();
    let mut cursor = None;
    let mut rest = apply;
    while let Some(start) = rest.find("${") {
        let Some(len) = rest[start..].find('}') else {
            break;
        };
        text.push_str(&rest[..start]);
        cursor.get_or_insert(text.encode_utf16().count());
        rest = &rest[start + len + 1..];
    }
    text.push_str(rest);
    let end = text.encode_utf16().count();
    (text, cursor.unwrap_or(end))
}

fn item(completion: Completion) -> Item {
    let apply = completion.apply.as_deref().unwrap_or(completion.label.as_str());
    let (insert, cursor) = unsnippet(apply);
    Item {
        label: completion.label.to_string(),
        kind: completion.kind,
        insert,
        cursor,
        detail: completion.detail.map(|detail| detail.to_string()),
    }
}

/// typst-ide offers everything that fits the context and leaves matching
/// what was typed to the editor. Here: labels starting with it first, then
/// labels containing it, case-insensitively, each in typst-ide's order.
fn narrowed(completions: Vec<Completion>, typed: &str) -> Vec<Completion> {
    let typed = typed.to_lowercase();
    let (mut starts, mut rest): (Vec<_>, Vec<_>) = completions
        .into_iter()
        .filter(|c| c.label.to_lowercase().contains(&typed))
        .partition(|c| c.label.to_lowercase().starts_with(&typed));
    starts.append(&mut rest);
    starts
}

/// What could go at `cursor` (UTF-16) in `source`: `{from, items}`, or `null`
/// when nothing completes there. `explicit` is a request the reader made
/// (Ctrl+Space) rather than one typing implied, and widens what is offered.
#[wasm_bindgen]
pub fn complete(source: &str, cursor: usize, explicit: Option<bool>) -> String {
    let Some((world, main)) = alone(source) else {
        return "null".to_owned();
    };
    let main = &main;
    let offsets = Offsets::of(main.text());
    let at = offsets.byte(cursor);
    let found =
        typst_ide::autocomplete(&world, None::<&HtmlDocument>, main, at, explicit.unwrap_or(false));
    match found {
        Some((from, completions)) => {
            let typed = main.text().get(from..at).unwrap_or("");
            json(&Completions {
                from: offsets.utf16(from),
                items: narrowed(completions, typed).into_iter().map(item).collect(),
            })
        }
        None => "null".to_owned(),
    }
}

/// What the thing under `cursor` (UTF-16) is: `{kind, text}` with `kind`
/// `"text"` or `"code"`, or `null`.
#[wasm_bindgen]
pub fn hover(source: &str, cursor: usize) -> String {
    let Some((world, main)) = alone(source) else {
        return "null".to_owned();
    };
    let main = &main;
    let at = Offsets::of(main.text()).byte(cursor);
    match typst_ide::tooltip(&world, None::<&HtmlDocument>, main, at, Side::After) {
        Some(Tooltip::Text(text)) => json(&Hover { kind: "text", text: text.to_string() }),
        Some(Tooltip::Code(text)) => json(&Hover { kind: "code", text: text.to_string() }),
        None => "null".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_offers_functions_after_a_hash() {
        let answer = complete("#hea", 4, Some(true));
        let value: serde_json::Value = serde_json::from_str(&answer).expect("json");
        assert_eq!(value["from"], 1);
        let labels: Vec<&str> = value["items"]
            .as_array()
            .expect("items")
            .iter()
            .filter_map(|item| item["label"].as_str())
            .collect();
        assert_eq!(labels.first(), Some(&"heading"), "{labels:?}");
        assert!(labels.iter().all(|l| l.to_lowercase().contains("hea")), "{labels:?}");
    }

    #[test]
    fn snippets_lose_their_placeholders() {
        assert_eq!(unsnippet("heading(${})"), ("heading()".to_owned(), 8));
        assert_eq!(unsnippet("${lhs} + ${rhs}"), (" + ".to_owned(), 0));
        assert_eq!(unsnippet("plain"), ("plain".to_owned(), 5));
    }

    #[test]
    fn hover_names_a_function() {
        let answer = hover("#heading[x]", 3);
        assert!(answer.contains("heading") || answer != "null", "{answer}");
    }
}
