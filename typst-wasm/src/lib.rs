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
//! | export         | feature     | asks                                             |
//! | -------------- | ----------- | ------------------------------------------------ |
//! | `add_font`     |             | here is a font file; how many faces did it hold   |
//! | `Project`      |             | these files, kept: what HTML over this state      |
//! | `compile_html` | `oneshot`   | this source, these files: what HTML               |
//! | `highlight`    | `highlight` | which spans of this source are which syntax       |
//! | `complete`     | `ide`       | what could go at this cursor                      |
//! | `hover`        | `ide`       | what is the thing under this cursor               |
//!
//! TWO MODULES, ONE CRATE. The default features are the viewer's module
//! (`nix build .#prodrome-typst-wasm`), editor and all. A host that compiles
//! views over its state builds `--no-default-features`
//! (`.#prodrome-typst-wasm-views`): the compiler, `Project` and `add_font`.
//! Each feature stands alone and they compose; what one adds is measured in
//! the CHANGELOG.
//!
//! A `Project` IS A WORLD KEPT, as typst-cli's watch mode keeps one: a host
//! sets its view modules once and each again only when it changes, and
//! passes the state to each compilation, so a module that did not change is
//! neither copied in nor reparsed, and nothing memoised over what did not
//! change is recomputed (see `world.rs`). `typst-wasm/bench/` measures it
//! against `compile_html`.
//!
//! ERRORS ARE VALUES. Nothing here throws: a document with an error answers
//! `{html: null, diagnostics: [...]}`, each diagnostic with the span it is
//! about, because a live preview shows a half-typed document on every
//! keystroke and a failure is its most common answer.
//!
//! EVERY OFFSET IS UTF-16, the unit a browser's strings and selections count
//! in; see `offsets.rs`.

use serde::Serialize;
use typst::diag::{Severity, SourceDiagnostic};
use typst::WorldExt;
use typst_html::{HtmlDocument, HtmlOptions};
use wasm_bindgen::prelude::*;

#[cfg(feature = "highlight")]
mod highlight;
#[cfg(feature = "ide")]
mod ide;
mod offsets;
#[cfg(feature = "oneshot")]
mod oneshot;
mod world;

use offsets::Offsets;
use world::Sandbox;

/// The key a source compiled alone is compiled under (`compile_html`'s, and
/// the editor's), and its diagnostics name.
#[cfg(any(feature = "oneshot", feature = "ide"))]
const MAIN: &str = "/main.typ";

/// The key a `Project`'s state is read at.
const STATE: &str = "/state.json";

/// The Typst version this module was built against — the version the page's
/// documents are written for.
#[wasm_bindgen]
#[must_use]
pub fn typst_version() -> String {
    "0.15.1".to_owned()
}

/// Add every face in one font file (TTF, OTF or a collection) to every later
/// compilation. Answers the number of faces; zero means not a font.
#[wasm_bindgen]
#[allow(clippy::must_use_candidate)] // JavaScript calls it for its effect
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

    /// A project for markup whose author is not trusted: its library has no
    /// `html` module and no `plugin`, under any name, `eval` included, so a
    /// document naming either fails to compile with a diagnostic saying so.
    /// A document that names neither compiles to the same HTML either way.
    #[must_use]
    pub fn restricted() -> Project {
        Project { world: Sandbox::restricted() }
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_restricted_project_cannot_reach_html_or_plugin() {
        let page = "= A page\n\nWith *strong* text and #link(\"https://example.org\")[a link].";
        let (mut open, mut closed) = (Project::new(), Project::restricted());
        for project in [&mut open, &mut closed] {
            project.set("/view.typ", page);
        }
        let same = compiled(&open.compile("/view.typ", "{}"));
        assert!(same["html"].is_string(), "{same}");
        assert_eq!(compiled(&closed.compile("/view.typ", "{}"))["html"], same["html"]);
        for reach in [
            "#html.elem(\"script\")[x]",
            "#std.html.elem(\"script\")[x]",
            "#eval(\"html.elem(\\\"script\\\")[x]\")",
            "#eval(\"std.html\")",
            "#plugin(bytes(()))",
            "#eval(\"plugin\")",
        ] {
            open.set("/view.typ", reach);
            closed.set("/view.typ", reach);
            let refused = compiled(&closed.compile("/view.typ", "{}"));
            assert!(refused["html"].is_null(), "{reach}: {refused}");
            let named = refused["diagnostics"].as_array().expect("diagnostics").iter().any(|d| {
                d["severity"] == "error"
                    && d["message"]
                        .as_str()
                        .is_some_and(|m| m.contains("html") || m.contains("plugin"))
            });
            assert!(named, "{reach}: {refused}");
            // The same document reaches them in an open project: it is the
            // library that refuses, not the document that is broken.
            let reached = compiled(&open.compile("/view.typ", "{}"));
            let unknown =
                reached["diagnostics"].as_array().expect("diagnostics").iter().any(|d| {
                    d["message"].as_str().is_some_and(|m| m.starts_with("unknown variable"))
                });
            assert!(!unknown, "{reach}: {reached}");
        }
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
}
