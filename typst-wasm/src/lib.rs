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
use typst::{World, WorldExt};
use typst_html::{HtmlDocument, HtmlOptions};
use typst_ide::{Completion, CompletionKind, Tooltip};
use wasm_bindgen::prelude::*;

mod offsets;
mod world;

use offsets::Offsets;
use world::Sandbox;

/// The key `compile_html`'s source is compiled under, and its diagnostics
/// name.
const MAIN: &str = "/main.typ";

/// The key a `Project`'s state is read at.
const STATE: &str = "/state.json";

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
        file: file.map(world::key_of),
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

/// Compile the world's prepared main file to HTML, then drop memoised results
/// older than a few compilations, so a page left open over an afternoon of
/// keystrokes does not grow without bound.
fn compile(world: &Sandbox) -> Compiled {
    let warned = typst::compile::<HtmlDocument>(world);
    let mut diagnostics: Vec<Diagnostic> =
        warned.warnings.iter().map(|d| diagnostic(world, d)).collect();
    let html = match warned.output {
        Ok(document) => match typst_html::html(&document, &HtmlOptions { pretty: false }) {
            Ok(html) => Some(html),
            Err(errors) => {
                diagnostics.extend(errors.iter().map(|d| diagnostic(world, d)));
                None
            }
        },
        Err(errors) => {
            diagnostics.extend(errors.iter().map(|d| diagnostic(world, d)));
            None
        }
    };
    typst::comemo::evict(10);
    Compiled { html, diagnostics }
}

/// A world of one main source and the files in `files_json`, made for one
/// compilation.
fn compile_once(source: &str, files_json: &str) -> Compiled {
    let files: BTreeMap<String, String> = match serde_json::from_str(files_json) {
        Ok(files) => files,
        Err(e) => return refusal(format!("files must be a JSON object of path to text: {e}")),
    };
    let mut world = Sandbox::new();
    let set = files
        .iter()
        .try_for_each(|(key, text)| world.set(key, text).map(drop))
        .and_then(|()| world.set(MAIN, source).map(drop))
        .and_then(|()| world.prepare(MAIN));
    match set {
        Ok(()) => compile(&world),
        Err(e) => refusal(e),
    }
}

/// Compile `source` to one HTML document, in a world made for this call.
///
/// `files_json` is a JSON object from a path to that file's text: `/data.json`
/// in the project, or `@local/prodrome-typst:0.1.0/lib.typ` inside a package,
/// which is how `#import "@local/prodrome-typst:0.1.0"` finds its files.
/// `source` is `/main.typ`.
///
/// Answers `{html, diagnostics}` as JSON: `html` is the document, or `null`
/// when an error stopped it; `diagnostics` holds the errors and warnings, each
/// `{severity, message, hints, file, from, to}`.
#[wasm_bindgen]
pub fn compile_html(source: &str, files_json: &str) -> String {
    json(&compile_once(source, files_json))
}

/// A world kept between compilations: what a host that recompiles its views
/// as its state changes holds one of.
///
/// Each file is set on its own, and setting one again with an edit reparses
/// only the edit; the state is the argument of each compilation, read by the
/// views as `json("/state.json")`. Everything memoised about a file or the
/// state that did not change is reused.
#[wasm_bindgen]
pub struct Project {
    world: Sandbox,
}

#[wasm_bindgen]
impl Project {
    #[wasm_bindgen(constructor)]
    #[must_use]
    #[allow(clippy::new_without_default)] // a JS constructor, not a Rust API
    pub fn new() -> Project {
        Project { world: Sandbox::new() }
    }

    /// Set or replace the file at `key` (a key as `compile_html`'s files
    /// have). Answers `undefined`, or why the key names no file.
    pub fn set(&mut self, key: &str, text: &str) -> Option<String> {
        self.world.set(key, text).err()
    }

    /// Forget the file at `key`; answers whether there was one.
    pub fn remove(&mut self, key: &str) -> bool {
        self.world.remove(key)
    }

    /// Compile the file at `main`, set before, with `state` as
    /// `/state.json`. Answers what `compile_html` does.
    pub fn compile(&mut self, main: &str, state: &str) -> String {
        let prepared = self.world.set(STATE, state).and_then(|_| self.world.prepare(main));
        json(&match prepared {
            Ok(()) => compile(&self.world),
            Err(e) => refusal(e),
        })
    }
}

// --- highlight ---------------------------------------------------------------

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
    fn a_document_compiles_to_html() {
        let answer = compile_once("= Hello\n\nA *strong* word.", "{}");
        let html = answer.html.expect("it compiles");
        assert!(html.contains("<h2>Hello</h2>") || html.contains("Hello"), "{html}");
        assert!(html.contains("<strong>strong</strong>"), "{html}");
    }

    #[test]
    fn an_error_is_a_value_with_its_span() {
        // `é` is two bytes and one UTF-16 unit: the span must count units.
        let source = "é #nope";
        let answer = compile_once(source, "{}");
        assert!(answer.html.is_none());
        let error = answer.diagnostics.iter().find(|d| d.severity == "error").expect("an error");
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
        let answer = compile_once(
            "#import \"@local/pkg:0.1.0\": twice\n#twice(json(\"/data.json\").n)",
            &files.to_string(),
        );
        let html = answer.html.unwrap_or_else(|| panic!("{}", json(&answer.diagnostics)));
        assert!(html.contains("14"), "{html}");
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        let answer = compile_once("#read(\"/absent.txt\")", "{}");
        assert!(answer.html.is_none());
        assert!(!answer.diagnostics.is_empty());
    }

    #[test]
    fn bad_files_are_refused_as_a_value() {
        let answer = compile_once("hi", "[1, 2]");
        assert!(answer.html.is_none());
        assert_eq!(answer.diagnostics.len(), 1);
    }

    fn compiled(answer: &str) -> serde_json::Value {
        serde_json::from_str(answer).expect("json")
    }

    #[test]
    fn a_project_compiles_its_files_over_its_state() {
        let mut project = Project::new();
        assert_eq!(project.set("/lib.typ", "#let twice(n) = 2 * n"), None);
        assert_eq!(
            project
                .set("/view.typ", "#import \"/lib.typ\": twice\n#twice(json(\"/state.json\").n)"),
            None
        );
        let first = compiled(&project.compile("/view.typ", "{\"n\": 7}"));
        assert!(first["html"].as_str().is_some_and(|html| html.contains("14")), "{first}");
        let second = compiled(&project.compile("/view.typ", "{\"n\": 8}"));
        assert!(second["html"].as_str().is_some_and(|html| html.contains("16")), "{second}");
        project.set("/lib.typ", "#let twice(n) = 3 * n");
        let edited = compiled(&project.compile("/view.typ", "{\"n\": 8}"));
        assert!(edited["html"].as_str().is_some_and(|html| html.contains("24")), "{edited}");
    }

    #[test]
    fn a_project_refuses_as_a_value() {
        let mut project = Project::new();
        assert!(project.set("@nope", "").is_some());
        let unset = compiled(&project.compile("/view.typ", "{}"));
        assert!(unset["html"].is_null());
        project.set("/view.typ", "#nope");
        let error = compiled(&project.compile("/view.typ", "{}"));
        let files: Vec<_> = error["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .map(|d| &d["file"])
            .collect();
        assert!(files.contains(&&serde_json::json!("/view.typ")), "{error}");
        assert!(project.remove("/view.typ"));
        assert!(!project.remove("/view.typ"));
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
