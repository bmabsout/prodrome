# The viewer: a Prodrome in the browser

<https://bmabsout.github.io/prodrome/> shows this crate's
[roadmap](roadmap.md), read-only. It is a static site. The page fetches the
objects of the `roadmap-data` branch as they are on disk. It folds and
prices them with the core compiled to WebAssembly, and it typesets the
result with Typst compiled to WebAssembly. No server computes anything.

- `#/` is the list, most urgent first. Each row draws its price as a small
  pie.
- `#/todo/<id>` is one item: its body, price, explanation and history. It
  draws thirty days of the item's value, fifteen back and fifteen ahead, as
  one line.

Both marks use one continuous colour scale: red at 0, through orange and
yellow, to green at 1. The item page also has a small Typst editor with
highlighting and completion, as a demo; it saves nothing.

## It is an example, and it is optional

`core/` and `cli/` never depend on Typst, and nothing in them assumes that a
store's text is Typst. Typst lives in three directories of its own, for a
host whose items hold Typst:

```
typst-wasm/   Typst 0.15.1 for the browser: HTML export, highlighting, completion,
              and a world kept between compilations for a host's views
              (its own cargo workspace and lock file, outside the core's)
typst/        prodrome-typst, a Typst package: the list, an item, the two marks
viewer/       the static web app, in TypeScript, built by nix with tsc and esbuild
```

The command line's `list` and `show` stay plain text: to typeset them would
make `cli/` depend on Typst. To get a PDF, compile the package's layouts with
the `typst` binary (see [`typst/README.md`](../typst/README.md)).

## Run it locally

Against a checkout of `roadmap-data`:

```console
$ nix build .#prodrome-viewer -o result-viewer
$ git worktree add ../roadmap-data roadmap-data
$ sh viewer/assemble.sh result-viewer ../roadmap-data/roadmap site
$ python3 -m http.server -d site 8000     # then open http://localhost:8000/
```

`assemble.sh` works on any store, so the same commands show your own store.
If Playwright is installed, `node viewer/test/smoke.mjs site <todo>` drives
the list and one item in headless Chromium.

## GitHub Pages

The Pages workflow, `.github/workflows/pages.yml`, runs the same build and
assembly on every push to `main`, and after every verified push to
`roadmap-data`.
