// prodrome-typst — layouts for a Prodrome read as Typst.
//
// AN OPTIONAL EXTRA. The Prodrome's core never assumes an item's text is
// Typst; this package is for a host whose items do, or that wants its list
// and its marks typeset. It reads DATA — the JSON the core's WebAssembly
// module answers (`entries`, `explain`, a todo's `stream`), with the marks'
// samples beside it — and computes nothing about fulfillment itself: every
// number it draws was produced by the one evaluator (SPEC §1). What it adds
// is the one reading a picture needs, the state a value falls in.
//
// The data (see README.md for the whole shape):
//
//   (at: "2026-09-27T12:00:00",   the instant everything was read at
//    entries: (..),               prodrome-wasm `entries(..).entries`
//    records: (..),               prodrome-wasm `entries(..).records`
//    marks: ("<todo>": (ring: (72 values), bar: (values: (30), now: 10))),
//    explain: ("<todo>": tree),   prodrome-wasm `explain`, optional
//    history: ("<todo>": (..)))   the todo's events, optional
//
// Every layout works in paged export (PDF, PNG, SVG) and in HTML export
// (`--features html`), where the marks become inline SVG through
// `html.frame`.

// --- states ------------------------------------------------------------------

/// The three states a fulfillment falls in, and their colours. LOW IS URGENT:
/// a value is how well things go if nothing changes.
#let problem = rgb("#e55900")
#let watch = rgb("#a44554")
#let fine = rgb("#379775")
/// No value at all — a todo with no price. Absence is not a zero.
#let unpriced = rgb("#9b9b9b")
#let ink = rgb("#222222")

/// "Problem" below 0.5, "Watch" below 0.7, "Fine" from there, "Unpriced" for
/// `none`.
#let state-of(value) = if value == none {
  "Unpriced"
} else if value < 0.5 {
  "Problem"
} else if value < 0.7 {
  "Watch"
} else {
  "Fine"
}

#let colour-of(value) = (
  Problem: problem,
  Watch: watch,
  Fine: fine,
  Unpriced: unpriced,
).at(state-of(value))

/// A value as the percentage the CLI prints (`0.42` is `42%`), `—` for none.
#let percent(value) = if value == none { "—" } else {
  str(int(calc.round(value * 100))) + "%"
}

// --- target-agnostic pieces --------------------------------------------------

/// Drawn content: itself on a page, an inline SVG in HTML.
#let _frame(body) = context if target() == "html" { html.frame(body) } else { body }

/// A block a stylesheet can find by class in HTML; a plain block on a page.
#let _div(class, body) = context if target() == "html" {
  html.elem("div", attrs: (class: class), body)
} else { block(body) }

#let _span(class, body) = context if target() == "html" {
  html.elem("span", attrs: (class: class), body)
} else { body }

/// A value, coloured by its state.
#let price(value) = _span("price " + lower(state-of(value)), text(
  fill: colour-of(value),
  weight: "bold",
  percent(value),
))

/// A record's text: Typst markup when the host says its items hold Typst,
/// verbatim otherwise.
#let _text(body, markup) = if body == none or body == "" { [] } else if markup {
  eval(body, mode: "markup")
} else { body }

// --- the two marks -----------------------------------------------------------

/// THE RING: the next hours, clockwise from twelve o'clock (now), one wedge
/// per sample, each in its state's colour. `values` is conventionally 72
/// hourly samples; any count draws.
#let ring(values, size: 2.4em, thickness: 0.34) = {
  let n = calc.max(values.len(), 1)
  let r = size / 2
  let inner = r * (1 - thickness)
  let at(radius, angle) = (r + radius * calc.sin(angle), r - radius * calc.cos(angle))
  _frame(box(width: size, height: size, {
    for (i, value) in values.enumerate() {
      let a = 360deg * i / n
      let b = 360deg * (i + 1) / n
      let c = colour-of(value)
      place(polygon(
        fill: c,
        stroke: 0.25pt + c,
        at(r, a), at(r, (a + b) / 2), at(r, b), at(inner, b), at(inner, a),
      ))
    }
    // Now: a tick across the ring at twelve.
    place(line(start: (r, 0pt), end: (r, r - inner), stroke: 1.2pt + ink))
  }))
}

/// THE BAR: one cell per day, the past faded, a tick at now, then the
/// future. `values` is conventionally 30 daily samples and `now` the index
/// (fractional allowed) of the present within them.
#let bar(values, now: 0, width: 12em, height: 0.7em) = {
  let n = calc.max(values.len(), 1)
  let w = width / n
  let lip = 0.18em
  _frame(box(width: width, height: height + 2 * lip, {
    for (i, value) in values.enumerate() {
      let c = colour-of(value)
      let c = if i + 1 <= now { c.transparentize(70%) } else { c }
      place(dx: w * i, dy: lip, rect(width: w, height: height, fill: c, stroke: 0.2pt + c))
    }
    place(dx: w * calc.min(now, n) - 0.5pt, rect(width: 1pt, height: height + 2 * lip, fill: ink))
  }))
}

// --- reading the data --------------------------------------------------------

#let _record(data, entry) = if entry.content == none { none } else {
  data.at("records", default: (:)).at(entry.content, default: none)
}

#let _field(record, name) = if record == none { "" } else {
  let value = record.at(name, default: "")
  if value == none { "" } else { value }
}

/// What a todo asks for: its record's body, else its `Created`'s text — the
/// body `prodrome list` prints.
#let _body(data, entry) = {
  let body = _field(_record(data, entry), "body")
  if body != "" { body } else {
    data.at("created", default: (:)).at(entry.todo, default: (text: "")).text
  }
}

/// Whether the todo had been created by the instant the data was read at. A
/// row exists for every todo the store mentions, the future's included.
#let _existed(data, entry) = {
  let created = data.at("created", default: none)
  if created == none { true } else {
    let first = created.at(entry.todo, default: none)
    first != none and first.at <= data.at
  }
}

#let _marks(data, todo) = data.at("marks", default: (:)).at(todo, default: none)

/// Most urgent first: ascending in value, the unpriced last, ties by id — the
/// order `prodrome list` prints.
#let by-fulfillment(entries) = entries
  .sorted(key: e => e.todo)
  .sorted(key: e => if e.value == none { 2.0 } else { e.value })

#let _ring-of(data, todo, size: 2.4em) = {
  let marks = _marks(data, todo)
  if marks != none and "ring" in marks { ring(marks.ring, size: size) }
}

#let _bar-of(data, todo, width: 12em) = {
  let marks = _marks(data, todo)
  if marks != none and "bar" in marks {
    bar(marks.bar.values, now: marks.bar.now, width: width)
  }
}

// --- the list ----------------------------------------------------------------

/// The roadmap: every open todo, most urgent first, each with its ring, its
/// price, its bar and a link to its page. `href` turns a todo id into that
/// link. The closed follow, in a shorter list.
#let roadmap(
  data,
  title: [Roadmap],
  href: todo => "#/todo/" + todo,
  markup: false,
) = {
  let open = by-fulfillment(data.entries.filter(e => e.state == "open" and _existed(data, e)))
  let closed = data.entries.filter(e => e.state != "open").sorted(key: e => e.todo)

  heading(level: 1, title)
  _div("summary", [#open.len() open at #raw(data.at) — most urgent first.])

  _div("roadmap", table(
    columns: 4,
    table.header([Next 72 h], [Price], [Item], [30 days]),
    ..open
      .map(e => (
        _ring-of(data, e.todo),
        price(e.value),
        [#link(href(e.todo), raw(e.todo)) \ #_text(_body(data, e), markup)],
        _bar-of(data, e.todo),
      ))
      .flatten(),
  ))

  if closed.len() > 0 {
    heading(level: 2)[Closed]
    list(..closed.map(e => [
      #link(href(e.todo), raw(e.todo)) — #e.state #if e.at != "" [since #e.at]
    ]))
  }
}

// --- one item ----------------------------------------------------------------

#let _children = ("terms", "term", "gate", "body", "delta", "pending")

#let _note(value) = if type(value) == str { value } else { repr(value) }

/// An explanation tree (prodrome-wasm `explain`): each node's kind and value,
/// its notes, and its parts beneath it. A decoration of the term, never a
/// second reading of it.
#let explanation(node) = {
  let notes = node
    .pairs()
    .filter(((key, _)) => key not in ("kind", "value") and key not in _children)
  [#raw(node.kind) #price(node.value)]
  if notes.len() > 0 {
    [ — ]
    notes.map(((key, value)) => [#emph(key): #_note(value)]).join([, ])
  }
  let parts = ()
  for key in _children {
    if key in node {
      let child = node.at(key)
      if type(child) == array {
        parts += child.map(c => (key, c))
      } else {
        parts.push((key, child))
      }
    }
  }
  if parts.len() > 0 {
    list(..parts.map(((key, child)) => [#emph(key) #explanation(child)]))
  }
}

/// One todo's page: its body, its price and marks, its detail, the spec that
/// prices it and why it is worth what it is, and its history. `objects` is
/// where an object's file is linked (`objects/<hash>.py` on the site).
#let item(
  data,
  todo,
  markup: false,
  objects: "objects/",
  back: "#/",
) = {
  let entry = data.entries.find(e => e.todo == todo)
  if entry == none {
    heading(level: 1)[No item #raw(todo)]
    [Nothing in this store mentions #raw(todo). #link(back)[Back to the list.]]
    return
  }
  let record = _record(data, entry)
  let body = _body(data, entry)

  _div("crumbs", [#link(back)[← Roadmap] · #raw(todo)])
  heading(level: 1, if body == "" { raw(todo) } else { _text(body, markup) })

  _div("marks", {
    _ring-of(data, todo, size: 4em)
    context if target() != "html" { h(1em) }
    _bar-of(data, todo, width: 16em)
  })
  _div("facts", [
    #price(entry.value) · #state-of(entry.value) · #entry.state
    #if entry.claimed != "" [ · claimed #entry.claimed]
    #if entry.unconfirmed [ · unconfirmed]
    #if entry.at not in (none, "") [ · since #entry.at]
  ])

  let detail = _field(record, "detail")
  if detail != "" { _div("detail", _text(detail, markup)) }

  heading(level: 2)[Price]
  if entry.spec == none {
    [No spec: this item has no price, which is not a zero.]
  } else {
    raw(entry.spec, block: true, lang: "python")
  }
  if entry.unlinked != none [Not priced: #entry.unlinked]

  let tree = data.at("explain", default: (:)).at(todo, default: none)
  if tree != none {
    heading(level: 2)[Explanation]
    _div("explanation", explanation(tree))
  }

  let events = data.at("history", default: (:)).at(todo, default: ())
  if events.len() > 0 {
    heading(level: 2)[History]
    _div("history", table(
      columns: 4,
      table.header([When], [Event], [By], [Object]),
      ..events
        .map(e => (
          e.at,
          e.kind,
          e.actor,
          link(objects + e.hash + ".py", raw(e.hash.slice(0, 12))),
        ))
        .flatten(),
    ))
  }
}
