// The Prodrome in the browser: a READ-ONLY viewer, and an example of how a
// host uses the Prodrome.
//
// The site serves the store as it is on disk — `objects/<name>.py`, each an
// object's canonical print — plus `index.json`, the list of names. This page
// fetches them, folds and prices them with the core compiled to WebAssembly
// (prodrome-wasm), and typesets the result with Typst compiled to
// WebAssembly (typst-wasm) and the `prodrome-typst` package. Nothing is
// computed on a server, and nothing is written anywhere.
//
// ROUTES ARE A CONTRACT: `#/` is the list and `#/todo/<id>` an item. Other
// tools link to them (the issue mirror links `#/todo/gh-<number>`).

import initProdrome, * as prodrome from "prodrome-wasm";
import initTypst, * as typstWasm from "typst-wasm";
import { editor } from "./editor";
import { wall, type Naive } from "./time";
import { body, compilePage, itemDocument, listDocument, scratchDocument, type Typst } from "./typst";
import { read, type Entry, type ObjectIn, type View } from "./view";

declare const __BUILD__: string;

/** The fonts the site bundles for Typst — Libertinus and Source Serif, OFL. */
const FONTS = [
  "LibertinusSerif-Regular.otf",
  "LibertinusSerif-Italic.otf",
  "LibertinusSerif-Bold.otf",
  "LibertinusMath-Regular.otf",
  "SourceSerif4-Regular.otf",
  "SourceSerif4-It.otf",
];

/** An instant as a reader says it; a naive instant's digits read as UTC (see time.ts). */
const READABLE = new Intl.DateTimeFormat("en-US", {
  dateStyle: "medium",
  timeStyle: "short",
  hourCycle: "h23",
  timeZone: "UTC",
});

type Route = { page: "list" } | { page: "item"; todo: string };

export function route(hash: string): Route {
  const item = /^#\/todo\/(.+)$/.exec(hash);
  if (item) {
    try {
      return { page: "item", todo: decodeURIComponent(item[1]) };
    } catch {
      return { page: "item", todo: item[1] };
    }
  }
  return { page: "list" };
}

const page = document.getElementById("page") as HTMLElement;
const status = document.getElementById("status") as HTMLElement;

function say(text: string) {
  status.textContent = text;
}

async function fetchObjects(): Promise<ObjectIn[]> {
  const index = await fetch("index.json", { cache: "no-cache" });
  if (!index.ok) throw new Error(`index.json: ${index.status} ${index.statusText}`);
  const names = (await index.json()) as string[];
  return Promise.all(
    names.map(async (hash) => {
      const response = await fetch(`objects/${hash}.py`);
      if (!response.ok) throw new Error(`object ${hash}: ${response.status}`);
      return { hash, text: await response.text() };
    }),
  );
}

async function loadTypst(): Promise<Typst> {
  await initTypst({ module_or_path: `wasm/typst_bg.wasm?v=${__BUILD__}` });
  const fonts = await Promise.all(
    FONTS.map(async (name) => {
      const response = await fetch(`fonts/${name}`);
      return response.ok ? new Uint8Array(await response.arrayBuffer()) : null;
    }),
  );
  for (const font of fonts) if (font) typstWasm.add_font(font);
  return typstWasm;
}

/** A row's value as the list prints it: a percentage, `∅` for absent, `—` unlinked. */
function percent(value: Entry["value"]): string {
  if (value === "absent") return "∅";
  if (value === null) return "—";
  return `${Math.round(value * 100)}%`;
}

/** Before Typst has arrived, or if it never does: the list as plain HTML, in the core's order. */
function plain(view: View): HTMLElement {
  const list = document.createElement("ol");
  const rows = new Map(view.entries.map((e) => [e.todo, e]));
  const open = view.order.map((todo) => rows.get(todo) as Entry).filter((e) => e.state === "open");
  for (const entry of open) {
    const li = document.createElement("li");
    const a = Object.assign(document.createElement("a"), { href: `#/todo/${entry.todo}` });
    a.textContent = entry.todo;
    const value = percent(entry.value);
    li.append(`${value} `, a, ` ${view.created[entry.todo]?.text ?? ""}`);
    list.append(li);
  }
  return list;
}

function show(nodes: Node, typeset: boolean) {
  const container = document.createElement("div");
  container.className = typeset ? "typst" : "plain";
  container.append(nodes);
  page.replaceChildren(container);
}

async function main() {
  if ("serviceWorker" in navigator) {
    navigator.serviceWorker.register("sw.js").catch(() => undefined);
  }
  say("loading the core…");
  await initProdrome({ module_or_path: `wasm/prodrome_bg.wasm?v=${__BUILD__}` });
  say("fetching the objects…");
  const objects = await fetchObjects();
  const typst = loadTypst();

  // THE ONE READ OF THE CLOCK, as in the command line: the core has none,
  // and a host names the instant it asks about.
  const now: Naive = wall(new Date());
  const readAt = `${objects.length} objects, read ${READABLE.format(new Date(now))}`;

  let current = 0;
  const render = async () => {
    const ticket = ++current;
    const at = route(location.hash);
    const focus = at.page === "item" ? at.todo : null;
    const view = read(prodrome, objects, now, focus);
    if (at.page === "list") show(plain(view), false);
    say(`${readAt} — loading Typst…`);

    const engine = await typst;
    if (ticket !== current) return;
    const compiled = compilePage(engine, at.page === "list" ? listDocument : itemDocument, view);
    if (compiled.html === null) {
      const errors = compiled.diagnostics.map((d) => d.message).join("; ");
      say(`Typst could not typeset this page: ${errors}`);
      if (at.page === "list") return;
      show(plain(view), false);
      return;
    }
    const content = document.createDocumentFragment();
    content.append(body(compiled.html));
    if (!compiled.markup) {
      const note = document.createElement("p");
      note.className = "note";
      note.textContent = "This text is not valid Typst, so it is shown as written.";
      content.append(note);
    }
    if (at.page === "item" && view.entries.some((e) => e.todo === at.todo)) {
      content.append(editor(engine, view, scratchDocument()));
    }
    show(content, true);
    document.title = at.page === "item" ? `${at.todo} — Prodrome` : "Roadmap — Prodrome";
    say(readAt);
    document.body.dataset.state = "typeset";
    window.scrollTo(0, 0);
  };

  window.addEventListener("hashchange", () => {
    document.body.dataset.state = "loading";
    render().catch(fail);
  });
  await render();
}

function fail(error: unknown) {
  say(`Something went wrong: ${error instanceof Error ? error.message : String(error)}`);
  document.body.dataset.state = "failed";
}

main().catch(fail);
