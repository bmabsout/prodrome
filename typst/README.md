# prodrome-typst

A Typst package that lays out a Prodrome: the roadmap list ordered by
fulfillment, one item's page, and the two FPL marks.

**An optional extra.** The Prodrome's core (`core/`, `cli/`) never depends on
Typst and never assumes an item's text is Typst. This package is for a host
whose items hold Typst, or that simply wants its roadmap typeset. The browser
viewer in `viewer/` is one such host, and an example of how to use it.

**It computes nothing about fulfillment.** Every number it draws was answered
by the core (compiled to WebAssembly, or any other implementation of SPEC.md).
The one reading it adds is the state a value falls in, for colour.

| State    | Value         | Colour    |
| -------- | ------------- | --------- |
| Problem  | below 0.5     | `#e55900` |
| Watch    | below 0.7     | `#a44554` |
| Fine     | 0.7 and above | `#379775` |
| Unpriced | `∅` or none   | grey      |

Low is urgent: a value says how well things go if nothing changes.

## Use

```typst
#import "@local/prodrome-typst:0.1.0": roadmap, item, ring, bar

#let data = json("data.json")
#roadmap(data)                         // the list, links to #/todo/<id>
#item(data, "ship-the-viewer")         // one item's page
#ring(data.marks.at("ship-the-viewer").ring)
```

Every layout works in paged export (PDF, PNG, SVG) and in HTML export
(`typst compile --features html --format html`), where the marks become inline
SVG. The examples compile both ways:

```console
$ mkdir -p pkgs/local/prodrome-typst && ln -s "$PWD/typst" pkgs/local/prodrome-typst/0.1.0
$ typst compile --package-path pkgs typst/examples/roadmap.typ
$ typst compile --package-path pkgs --features html --format html typst/examples/item.typ
```

`nix flake check` does the same (`checks.prodrome-typst`), and compiles
`tests/laws.typ`, whose asserts are the package's laws.

## The data

One JSON object, in the shapes prodrome-wasm answers, with the marks'
samples beside them. `examples/data.json` is a real one, made from a small
store by the viewer's own `viewer/src/view.ts`.

| Key       | What                                                              | From                            |
| --------- | ----------------------------------------------------------------- | ------------------------------- |
| `at`      | the instant everything was read at, ISO                           | `entries(..).at`                |
| `entries` | one row per todo (SPEC §6.7)                                      | `entries(..).entries`           |
| `order`   | the rows' todo ids in the core's list order                       | `entries(..).order`             |
| `records` | the records the rows name, by object name                        | `entries(..).records`           |
| `created` | per todo, its first `Created`'s `at` and its last one's `text`    | `entries(..).created`           |
| `marks`   | per todo, `ring` (72 hourly values from now) and `bar` (`values`, one per day, and `now`, the index of the present); `null` where the todo reads `∅` | `fulfillment`, `series_knots` |
| `explain` | per todo, the explanation tree (optional)                         | `explain`                       |
| `history` | per todo, its events: `at`, `kind`, `actor`, `hash` (optional)    | `fold(..).stream`               |
| `focus`   | the todo a page is about, or `null`                               | the host                        |

## What it exports

- `roadmap(data, title:, href:, markup:)`: the open items that exist at `at`,
  in the core's list order (ascending value, then every item with no number,
  ties by id: the order `prodrome list` prints), then the closed.
- `item(data, todo, markup:, objects:, back:)`: body, price and state, the
  marks, detail, the price as the explanation reads it ("70%, flat"), and the
  history, each event dated and linked to its object file.
- `ring(values, size:, thickness:)`: the next hours clockwise from twelve
  o'clock, one wedge per sample, with a tick at now.
- `bar(values, now:, width:, height:)`: one cell per day, the past faded, a
  tick at now.
- `state-of`, `colour-of`, `percent`, `price`, `when` (an instant as a reader
  says it), `explanation`, `listed`, and
  the colours `problem`, `watch`, `fine`, `unpriced`.

`markup: true` reads an item's body and detail as Typst markup; the default
shows them verbatim, because nothing promises a store's text is Typst. In HTML
export each piece carries a class (`roadmap`, `price problem`, `marks`,
`explanation`, …) for a stylesheet to find.
