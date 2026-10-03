// What a host recompiling its views pays, old export against new.
//
//   node typst-wasm/bench/views.mjs FULL_WEB VIEWS_WEB
//
// FULL_WEB and VIEWS_WEB are the `web/` directories of
// `nix build .#prodrome-typst-wasm` and `.#prodrome-typst-wasm-views`;
// `checks.prodrome-typst-wasm-bench` runs this. One page of 250 list items
// over a state, compiled by three engines, each in a node process of its own
// so that the first compilation is cold — the engine compiles the module's
// code lazily, and shares it between instances of the same bytes:
//
//   compile_html  the viewer's module, a world made per call from one JSON
//                 object holding every file
//   Project       the viewer's module, the world kept between compilations
//   Project       the views module, the same
//
// and for each: the first compilation, then the median of RUNS compilations
// with the state identical, with one item changed, and with every item
// changed; then an editor's keystroke, one character typed into a 2 KB
// document and the document recompiled (the file set again, for a
// Project); then the module's memory after MANY compilations of one change
// each. A wasm memory never shrinks, so that is its high-water mark: it stays
// flat only if `comemo::evict` bounds what a long session keeps.
//
// Every compilation is checked for the HTML it should hold, so a run that
// times a failure fails.

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath, pathToFileURL } from "node:url";

const RUNS = 51;
const MANY = 500;
const ITEMS = 250;

const LIB = `#let entry(it) = [*#it.name*: #it.text (#it.n)]`;
const VIEW = [
  `#import "/lib.typ": entry`,
  `#let s = json("/state.json")`,
  `= #s.title`,
  `#list(..s.items.map(entry))`,
  ``,
].join("\n");

/** About 2 KB of markup, and where in it a reader is typing. */
const DRAFT = Array.from(
  { length: 24 },
  (_, i) => `== Section ${i}\n\nSome *strong* words, _emphasis_ and \`code\`, then a list:\n- one\n- two\n\n`,
).join("");
const CARET = DRAFT.indexOf("Some", DRAFT.length / 2);

function state(n) {
  return {
    title: "A page of items",
    items: Array.from({ length: ITEMS }, (_, i) => ({
      name: `item ${i}`,
      text: `the text of item ${i}, a sentence long`,
      n: n(i),
    })),
  };
}

async function load(dir) {
  const glue = await import(pathToFileURL(join(dir, "typst.js")).href);
  const wasm = glue.initSync({ module: readFileSync(join(dir, "typst_bg.wasm")) });
  return { glue, memory: () => wasm.memory.buffer.byteLength };
}

function oneshot(glue) {
  return (text) => glue.compile_html(VIEW, JSON.stringify({ "/lib.typ": LIB, "/state.json": text }));
}

function project(glue) {
  const p = new glue.Project();
  for (const [key, text] of [["/lib.typ", LIB], ["/view.typ", VIEW]]) {
    const refused = p.set(key, text);
    if (refused !== undefined) throw new Error(refused);
  }
  return (text) => p.compile("/view.typ", text);
}

function editing(api, glue) {
  if (api === "compile_html") return (text) => glue.compile_html(text, "{}");
  const p = new glue.Project();
  return (text) => {
    p.set("/draft.typ", text);
    return p.compile("/draft.typ", "{}");
  };
}

/** Type one more character at the caret, recompile, answer the ms taken. */
function keystrokes(edit) {
  let typed = "";
  return () => {
    typed += "x";
    const text = DRAFT.slice(0, CARET) + typed + DRAFT.slice(CARET);
    const start = performance.now();
    const answer = JSON.parse(edit(text));
    const ms = performance.now() - start;
    if (answer.html === null || !answer.html.includes(`${typed}Some`)) {
      throw new Error(`no ${typed}Some in ${JSON.stringify(answer).slice(0, 500)}`);
    }
    return ms;
  };
}

/** Compile, check the HTML holds `expect`, answer the milliseconds taken. */
function timed(compile, value, expect) {
  const text = JSON.stringify(value);
  const start = performance.now();
  const answer = JSON.parse(compile(text));
  const ms = performance.now() - start;
  if (answer.html === null || !answer.html.includes(expect)) {
    throw new Error(`no ${expect} in ${JSON.stringify(answer).slice(0, 500)}`);
  }
  return ms;
}

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const mb = (bytes) => (bytes / 2 ** 20).toFixed(1);

async function bench(api, dir) {
  const { glue, memory } = await load(dir);
  const compile = api === "compile_html" ? oneshot(glue) : project(glue);
  const row = {};
  row.cold = timed(compile, state(() => 0), "(0)");
  row.identical = median(Array.from({ length: RUNS }, () => timed(compile, state(() => 0), "(0)")));
  let run = 0;
  const one = () => {
    run += 1;
    const k = run % ITEMS;
    return timed(compile, state((i) => (i === k ? run : 0)), `(${run})`);
  };
  row.one = median(Array.from({ length: RUNS }, one));
  row.every = median(
    Array.from({ length: RUNS }, () => {
      run += 1;
      return timed(compile, state((i) => run + i), `(${run + ITEMS - 1})`);
    }),
  );
  const keystroke = keystrokes(editing(api, glue));
  keystroke();
  row.keystroke = median(Array.from({ length: RUNS }, keystroke));
  const before = memory();
  for (let i = 0; i < MANY; i += 1) one();
  row.memory = `${mb(before)} → ${mb(memory())}`;
  return row;
}

const [first, second, third] = process.argv.slice(2);
if (first === "--one") {
  // A child: one engine, one row, as JSON.
  console.log(JSON.stringify(await bench(second, third)));
} else {
  if (!first || !second) throw new Error("usage: views.mjs FULL_WEB VIEWS_WEB");
  const script = fileURLToPath(import.meta.url);
  const engines = [
    ["compile_html", first, "compile_html, viewer's module"],
    ["Project", first, "Project, viewer's module"],
    ["Project", second, "Project, views module"],
  ];
  const ms = (x) => `${x.toFixed(1)} ms`;
  console.log(`${ITEMS} list items; medians of ${RUNS}; memory in MiB before and after ${MANY} recompiles`);
  console.log(["engine", "cold", "identical", "one changed", "every changed", "keystroke", "memory"].join("\t"));
  for (const [api, dir, name] of engines) {
    const r = JSON.parse(execFileSync(process.execPath, [script, "--one", api, dir], { encoding: "utf8" }));
    console.log([name, ms(r.cold), ms(r.identical), ms(r.one), ms(r.every), ms(r.keystroke), r.memory].join("\t"));
  }
}
