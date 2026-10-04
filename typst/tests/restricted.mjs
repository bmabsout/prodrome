// The package compiles in typst-wasm's restricted project, whose library has
// no `html` and no `plugin`, as it does in an open one.
//
//   node typst/tests/restricted.mjs TYPST_WEB PACKAGES
//
// TYPST_WEB is the `web/` directory of `nix build .#prodrome-typst-wasm`;
// PACKAGES a package path, `local/<name>/<version>/…`, holding this package
// and every package it imports. Each package file is set at the key
// typst-wasm reads it by, each example at its own path beside its data, and
// each example is compiled in both projects. `checks.prodrome-typst-restricted`
// runs this.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { pathToFileURL } from "node:url";

const [web, packages] = process.argv.slice(2);
const glue = await import(pathToFileURL(join(web, "typst.js")).href);
glue.initSync({ module: readFileSync(join(web, "typst_bg.wasm")) });

const walk = (dir) =>
  readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });

/** Every Typst source and manifest of the package path, by typst-wasm's key. */
const files = new Map();
for (const name of readdirSync(join(packages, "local"))) {
  for (const version of readdirSync(join(packages, "local", name))) {
    const root = join(packages, "local", name, version);
    for (const path of walk(root).filter((p) => /\.(typ|toml)$/.test(p))) {
      files.set(`@local/${name}:${version}/${relative(root, path)}`, readFileSync(path, "utf8"));
    }
  }
}
const examples = join(packages, "local", "prodrome-typst", "0.1.0", "examples");
files.set("/examples/data.json", readFileSync(join(examples, "data.json"), "utf8"));

let failed = false;
for (const doc of ["roadmap", "item"]) {
  const main = `/examples/${doc}.typ`;
  for (const [name, project] of [["open", new glue.Project()], ["restricted", glue.Project.restricted()]]) {
    for (const [key, text] of files) project.set(key, text);
    project.set(main, readFileSync(join(examples, `${doc}.typ`), "utf8"));
    const { html, diagnostics } = JSON.parse(project.compile(main, "{}"));
    const errors = diagnostics.filter((d) => d.severity === "error").map((d) => d.message);
    const ok = html !== null && errors.length === 0 && html.includes("ship-the-viewer") && html.includes("42%");
    console.log(`${ok ? "ok  " : "FAIL"} ${doc}.typ in the ${name} project${errors.length ? ": " + errors.join("; ") : ""}`);
    failed ||= !ok;
  }
}
process.exit(failed ? 1 : 0);
