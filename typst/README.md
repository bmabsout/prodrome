# prodrome-typst

A Typst package that lays out a Prodrome: the roadmap list ordered by
fulfillment, one item's page, and the two FPL marks: a value's pie and a
todo's thirty days.

**An optional extra.** The Prodrome's core (`core/`, `cli/`) never depends on
Typst and never assumes an item's text is Typst. This package is for a host
whose items hold Typst, or that simply wants its roadmap typeset. The browser
viewer in `viewer/` is one such host, and an example of how to use it.

**It computes nothing about fulfillment.** Every number it draws was answered
by the core (compiled to WebAssembly, or any other implementation of SPEC.md).
The one reading it adds is a value's colour.

**The colour is continuous.** A fulfillment `v` in [0, 1] is drawn in
`colour(v)`, a sample of one gradient, `fulfillment`:

```typst
#let fulfillment = gradient.linear(color.oklch(color.red), color.oklch(color.green), space: oklch)
#let colour(value) = fulfillment.sample(value * 100%)
```

It is linear in each OKLCH channel, `oklch(65.95 + 8.01·v %, 0.227 − 0.007·v,
28.44 + 115.85·v)`, and runs red, orange, yellow, green. Low is urgent: a
value says how well things go if nothing changes. `∅`, or a todo that does
not link, has no value and no colour of the scale; it reads "unpriced" in
grey. Problem (below 0.5), Watch (below 0.7) and Fine are words
(`state-of`), never colours. `tests/laws.typ` holds the scale to that formula.

## Use

```typst
#import "@local/prodrome-typst:0.1.0": roadmap, item, pie, trace

#let data = json("data.json")
#roadmap(data)                         // the list, links to #/todo/<id>
#item(data, "ship-the-viewer")         // one item's page
#pie(0.42)                             // a value's pie
#let m = data.marks.at("ship-the-viewer")
#trace(m.values, at: data.at, back: m.back, ahead: m.ahead)
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
| `marks`   | per todo, `back` and `ahead` (days) and `values`, evenly spaced from `back` days before `at` to `ahead` days after it, both ends included, one of them at `at`; `null` where it reads `∅` | `fulfillment`, `series_knots` |
| `explain` | per todo, the explanation tree (optional)                         | `explain`                       |
| `history` | per todo, its events: `at`, `kind`, `actor`, `hash` (optional)    | `fold(..).stream`               |
| `focus`   | the todo a page is about, or `null`                               | the host                        |

## What it exports

- `roadmap(data, title:, href:, markup:)`: the open items that exist at `at`,
  in the core's list order (ascending value, then every item with no number,
  ties by id: the order `prodrome list` prints), then the closed.
- `item(data, todo, markup:, objects:, back:)`: body, price and state, the
  thirty days, detail, the price as the explanation reads it ("70%, flat"),
  and the history, each event dated and linked to its object file.
- `pie(value, size:)`: the value's share of a disc from twelve o'clock
  clockwise, in `colour(value)` on a faint track, with a thin outline.
- `trace(values, at:, back:, ahead:, width:, height:)`: one thick line, each
  point in `colour` of its own value, in a 0–100% frame with a dashed half;
  the past faded, now a thin ink line and a dot, dated at the ends, a week
  either side, and now.
- `fulfillment`, `colour`, `state-of`, `percent`, `price` (a pie and the
  percentage, or "unpriced"), `when` (an instant as a reader says it),
  `explanation`, `listed`, and the colours `unpriced` and `ink`.

`markup: true` reads an item's body and detail as Typst markup; the default
shows them verbatim, because nothing promises a store's text is Typst. In HTML
export each piece carries a class (`roadmap`, `price problem`, `marks`,
`explanation`, …) for a stylesheet to find.
