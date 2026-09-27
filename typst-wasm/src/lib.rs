//! Typst in the browser — an OPTIONAL extra, for a Prodrome whose items hold
//! Typst.
//!
//! Nothing in `prodrome-core` or `prodrome-cli` knows this crate exists, and
//! nothing in them assumes a store's text is Typst: a record's body is a
//! string, and what it means is the host's business. This crate is one host's
//! answer — the viewer's — and it lives outside the core's workspace, with its
//! own lock file, so the core's dependency tree never grows a typesetter.
//!
//! THIN ON PURPOSE, like `prodrome-wasm`: every export parses its arguments,
//! calls one thing in Typst, and prints the answer as JSON.
//!
//! | function       | asks                                                  |
//! | -------------- | ----------------------------------------------------- |
//! | `add_font`     | here is a font file; how many faces did it hold        |
//! | `compile_html` | this source, these files: what HTML, or what went wrong |
//! | `highlight`    | which spans of this source are which syntax            |
//! | `complete`     | what could go at this cursor                           |
//! | `hover`        | what is the thing under this cursor                    |
//!
//! ERRORS ARE VALUES. Nothing here throws: a document with an error answers
//! `{html: null, diagnostics: [...]}`, each diagnostic with the span it is
//! about, because a live preview shows a half-typed document on every
//! keystroke and a failure is its most common answer.
//!
//! EVERY OFFSET IS UTF-16, the unit a browser's strings and selections count
//! in; see [`offsets`].

use std::collections::BTreeMap;

use serde::Serialize;
use typst::diag::{Severity, SourceDiagnostic};
use typst::syntax::{highlight as tag_of, LinkedNode, Side, Source};
use typst::WorldExt;
use typst_html::{HtmlDocument, HtmlOptions};
use typst_ide::{Completion, CompletionKind, Tooltip};
use wasm_bindgen::prelude::*;

pub mod offsets;
pub mod world;

use offsets::Offsets;
use world::Sandbox;

/// The Typst version this module was built against — the version the page's
/// documents are written for.
#[wasm_bindgen]
pub fn typst_version() -> String {
    "0.15.1".to_owned()
}

/// Add every face in one font file (TTF, OTF or a collection) to every later
/// compilation. Answers the number of faces; zero means not a font.
#[wasm_bindgen]
pub fn add_font(data: Vec<u8>) -> usize {
    world::add_font(data)
}

// --- shapes on the wire ------------------------------------------------------

/// One diagnostic. `file` is the caller's key for the file it is about
/// (`/main.typ` for the source itself); `from`/`to` are UTF-16 offsets into
/// that file's text, absent when the diagnostic points at nothing.
#[derive(Serialize)]
struct Diagnostic {
    severity: &'static str,
    message: String,
    hints: Vec<String>,
    file: Option<String>,
    from: Option<usize>,
    to: Option<usize>,
}

#[derive(Serialize)]
struct Compiled {
    html: Option<String>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Serialize, PartialEq, Debug)]
struct Highlight {
    from: usize,
    to: usize,
    tag: &'static str,
}

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

fn json<T: Serialize>(value: &T) -> String {
    // Every shape above is strings, numbers and options: printing one cannot
    // fail, and if it somehow did, `null` is an answer a caller can read.
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

// --- compile -----------------------------------------------------------------

fn diagnostic(world: &Sandbox, diag: &SourceDiagnostic) -> Diagnostic {
    let severity = match diag.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    let file = diag.span.id();
    let range = world.range(diag.span);
    let (from, to) = match (file.and_then(|id| world.text(id)), range) {
        (Some(text), Some(range)) => {
            let offsets = Offsets::of(text);
            (Some(offsets.utf16(range.start)), Some(offsets.utf16(range.end)))
        }
        _ => (None, None),
    };
    Diagnostic {
        severity,
        message: diag.message.to_string(),
        hints: diag.hints.iter().map(|hint| hint.v.to_string()).collect(),
        file: file.map(|id| world.key(id)),
        from,
        to,
    }
}

fn refusal(message: String) -> Compiled {
    Compiled {
        html: None,
        diagnostics: vec![Diagnostic {
            severity: "error",
            message,
            hints: Vec::new(),
            file: None,
            from: None,
            to: None,
        }],
    }
}

fn compile(source: &str, files_json: &str) -> Compiled {
    let files: BTreeMap<String, String> = match serde_json::from_str(files_json) {
        Ok(files) => files,
        Err(e) => return refusal(format!("files must be a JSON object of path to text: {e}")),
    };
    let world = match Sandbox::new(source, files) {
        Ok(world) => world,
        Err(e) => return refusal(e),
    };
    let warned = typst::compile::<HtmlDocument>(&world);
    let mut diagnostics: Vec<Diagnostic> =
        warned.warnings.iter().map(|d| diagnostic(&world, d)).collect();
    let html = match warned.output {
        Ok(document) => match typst_html::html(&document, &HtmlOptions { pretty: false }) {
            Ok(html) => Some(html),
            Err(errors) => {
                diagnostics.extend(errors.iter().map(|d| diagnostic(&world, d)));
                None
            }
        },
        Err(errors) => {
            diagnostics.extend(errors.iter().map(|d| diagnostic(&world, d)));
            None
        }
    };
    // Memoised results older than a few compilations are dropped, so a page
    // left open over an afternoon of keystrokes does not grow without bound.
    typst::comemo::evict(10);
    Compiled { html, diagnostics }
}

/// Compile `source` to one HTML document.
///
/// `files_json` is a JSON object from a path to that file's text: `/data.json`
/// in the project, or `@local/prodrome-typst:0.1.0/lib.typ` inside a package,
/// which is how `#import "@local/prodrome-typst:0.1.0"` finds its files.
///
/// Answers `{html, diagnostics}` as JSON: `html` is the document, or `null`
/// when an error stopped it; `diagnostics` holds the errors and warnings, each
/// `{severity, message, hints, file, from, to}`.
#[wasm_bindgen]
pub fn compile_html(source: &str, files_json: &str) -> String {
    json(&compile(source, files_json))
}

// --- highlight ---------------------------------------------------------------

/// Every leaf's tag, taking the innermost tagged node it sits in — the
/// nesting `typst_syntax::highlight_html` prints, flattened into spans that do
/// not overlap, so a page can colour them without building a tree.
fn spans(source: &Source) -> Vec<Highlight> {
    fn walk(node: &LinkedNode, inherited: Option<&'static str>, offsets: &Offsets, out: &mut Vec<Highlight>) {
        let tag = tag_of(node).map(|tag| tag.css_class()).or(inherited);
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

// --- complete and hover ------------------------------------------------------

/// A completion's `apply` is snippet syntax — `${name}` for a placeholder,
/// `${}` for an empty one. A textarea has no placeholders, so they are
/// removed, and the cursor goes where the first one was.
fn unsnippet(apply: &str) -> (String, usize) {
    let mut text = String::new();
    let mut cursor = None;
    let mut rest = apply;
    while let Some(start) = rest.find("${") {
        let Some(len) = rest[start..].find('}') else { break };
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

/// What could go at `cursor` (UTF-16) in `source`: `{from, items}`, or `null`
/// when nothing completes there. `explicit` is a request the reader made
/// (Ctrl+Space) rather than one typing implied, and widens what is offered.
#[wasm_bindgen]
pub fn complete(source: &str, cursor: usize, explicit: Option<bool>) -> String {
    let Ok(world) = Sandbox::new(source, BTreeMap::new()) else {
        return "null".to_owned();
    };
    let main = world.main_source();
    let offsets = Offsets::of(main.text());
    let at = offsets.byte(cursor);
    let found = typst_ide::autocomplete(
        &world,
        None::<&HtmlDocument>,
        main,
        at,
        explicit.unwrap_or(false),
    );
    match found {
        Some((from, completions)) => json(&Completions {
            from: offsets.utf16(from),
            items: completions.into_iter().map(item).collect(),
        }),
        None => "null".to_owned(),
    }
}

/// What the thing under `cursor` (UTF-16) is: `{kind, text}` with `kind`
/// `"text"` or `"code"`, or `null`.
#[wasm_bindgen]
pub fn hover(source: &str, cursor: usize) -> String {
    let Ok(world) = Sandbox::new(source, BTreeMap::new()) else {
        return "null".to_owned();
    };
    let main = world.main_source();
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
    fn a_document_compiles_to_html() {
        let answer = compile("= Hello\n\nA *strong* word.", "{}");
        let html = answer.html.expect("it compiles");
        assert!(html.contains("<h2>Hello</h2>") || html.contains("Hello"), "{html}");
        assert!(html.contains("<strong>strong</strong>"), "{html}");
    }

    #[test]
    fn an_error_is_a_value_with_its_span() {
        // `é` is two bytes and one UTF-16 unit: the span must count units.
        let source = "é #nope";
        let answer = compile(source, "{}");
        assert!(answer.html.is_none());
        let error = answer
            .diagnostics
            .iter()
            .find(|d| d.severity == "error")
            .expect("an error");
        assert_eq!(error.file.as_deref(), Some("/main.typ"));
        assert_eq!((error.from, error.to), (Some(3), Some(7)));
    }

    #[test]
    fn files_are_found_by_key_and_packages_by_spec() {
        let files = serde_json::json!({
            "/data.json": "{\"n\": 7}",
            "@local/pkg:0.1.0/typst.toml":
                "[package]\nname = \"pkg\"\nversion = \"0.1.0\"\nentrypoint = \"lib.typ\"\n",
            "@local/pkg:0.1.0/lib.typ": "#let twice(n) = 2 * n",
        });
        let answer = compile(
            "#import \"@local/pkg:0.1.0\": twice\n#twice(json(\"/data.json\").n)",
            &files.to_string(),
        );
        let html = answer.html.unwrap_or_else(|| panic!("{}", json(&answer.diagnostics)));
        assert!(html.contains("14"), "{html}");
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        let answer = compile("#read(\"/absent.txt\")", "{}");
        assert!(answer.html.is_none());
        assert!(!answer.diagnostics.is_empty());
    }

    #[test]
    fn bad_files_are_refused_as_a_value() {
        let answer = compile("hi", "[1, 2]");
        assert!(answer.html.is_none());
        assert_eq!(answer.diagnostics.len(), 1);
    }

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
        assert!(labels.contains(&"heading"), "{labels:?}");
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
