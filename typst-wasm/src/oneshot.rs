//! `compile_html`: a world made for one call, every file in one JSON object
//! — what the viewer calls.

use std::collections::BTreeMap;

use wasm_bindgen::prelude::*;

use crate::world::Sandbox;
use crate::{compile, json, refusal, Compiled, MAIN};

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
}
