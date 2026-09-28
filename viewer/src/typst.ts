// The documents the viewer typesets, and the one door into typst-wasm.
//
// Each page is a two-line Typst document that imports `prodrome-typst` and
// hands it the data; the package's files are bundled into the app, and the
// data is passed as a virtual file, so a document compiled here is the same
// document `typst compile` reads from `typst/examples/`.

import libTyp from "../../typst/lib.typ";
import packageToml from "../../typst/typst.toml";
import type { View } from "./view";

/** The part of typst-wasm this reads — the glue module satisfies it. */
export interface Typst {
  compile_html(source: string, files_json: string): string;
  highlight(source: string): string;
  complete(source: string, cursor: number, explicit?: boolean | null): string;
  hover(source: string, cursor: number): string;
  add_font(data: Uint8Array): number;
}

export interface Diagnostic {
  severity: "error" | "warning";
  message: string;
  hints: string[];
  file: string | null;
  from: number | null;
  to: number | null;
}

export interface Compiled {
  html: string | null;
  diagnostics: Diagnostic[];
}

const PACKAGE = "@local/prodrome-typst:0.1.0";

export const IMPORT = `#import "${PACKAGE}": *`;

/** The package's files and the data, keyed the way typst-wasm finds them. */
export function files(view: View): Record<string, string> {
  return {
    [`${PACKAGE}/typst.toml`]: packageToml,
    [`${PACKAGE}/lib.typ`]: libTyp,
    "/data.json": JSON.stringify(view),
  };
}

export function listDocument(markup: boolean): string {
  return `${IMPORT}\n#roadmap(json("/data.json"), markup: ${markup})\n`;
}

/** The item named by the data's `focus`, so an id is never spliced into code. */
export function itemDocument(markup: boolean): string {
  return `${IMPORT}\n#let data = json("/data.json")\n#item(data, data.focus, markup: ${markup})\n`;
}

/** The editor's starting text: a small document over the same data. */
export function scratchDocument(): string {
  return [
    IMPORT,
    `#let data = json("/data.json")`,
    `#let e = data.entries.find(e => e.todo == data.focus)`,
    `// A scratch pad over this item's data. Nothing here is saved.`,
    ``,
    `= Notes on #raw(e.todo)`,
    ``,
    `Worth #price(e.value) now, which is *#state-of(e.value)*.`,
    ``,
    `#let m = data.marks.at(e.todo, default: none)`,
    `#if m != none { trace(m.values, at: data.at, back: m.back, ahead: m.ahead) }`,
    ``,
  ].join("\n");
}

export function compile(typst: Typst, source: string, view: View): Compiled {
  return JSON.parse(typst.compile_html(source, JSON.stringify(files(view)))) as Compiled;
}

/**
 * Compile a page with the items' text read as Typst, and again verbatim if
 * that fails: nothing promises a store's text is Typst, and one item's stray
 * `#` should cost that reading, not the page.
 */
export function compilePage(
  typst: Typst,
  document: (markup: boolean) => string,
  view: View,
): Compiled & { markup: boolean } {
  const typeset = compile(typst, document(true), view);
  if (typeset.html !== null) return { ...typeset, markup: true };
  return { ...compile(typst, document(false), view), markup: false };
}

const DROPPED = new Set(["SCRIPT", "IFRAME", "OBJECT", "EMBED", "LINK", "META", "BASE", "FORM"]);

/**
 * The body of a compiled document, as nodes safe to put in the page. Typst
 * escapes text, but `html.elem` can spell any element, and an item's text is
 * typeset as markup; so scripts, frames and handlers are dropped, and only
 * links within the site or to http(s) survive. The page's Content Security
 * Policy forbids inline script as well — this is the second lock.
 */
export function body(html: string): DocumentFragment {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const walker = doc.createTreeWalker(doc.body, NodeFilter.SHOW_ELEMENT);
  const doomed: Element[] = [];
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const el = node as Element;
    if (DROPPED.has(el.tagName.toUpperCase())) {
      doomed.push(el);
      continue;
    }
    for (const attr of Array.from(el.attributes)) {
      const name = attr.name.toLowerCase();
      const value = attr.value.trim().toLowerCase();
      const url = name === "href" || name === "src" || name === "xlink:href" || name === "action";
      if (name.startsWith("on") || (url && !/^(#|https?:|objects\/|[a-z0-9._-]+(\/|$))/.test(value))) {
        el.removeAttribute(attr.name);
      }
    }
  }
  for (const el of doomed) el.remove();
  const out = document.createDocumentFragment();
  out.append(...Array.from(doc.body.childNodes).map((n) => document.importNode(n, true)));
  return out;
}
