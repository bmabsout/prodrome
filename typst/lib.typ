// prodrome-typst — layouts for a Prodrome read as Typst.
//
// AN OPTIONAL EXTRA. The Prodrome's core never assumes an item's text is
// Typst; this package is for a host whose items do, or that wants its list
// and its marks typeset. It reads DATA — the JSON the core's WebAssembly
// module answers (`entries`, `explain`, a todo's `stream`), with the marks'
// samples beside it — and computes nothing about fulfillment itself: every
// number it draws was produced by the one evaluator (SPEC §1). What it adds
// is the one reading a picture needs, the colour of a value.
//
// The data (see README.md for the whole shape):
//
//   (at: "2026-09-27T12:00:00",   the instant everything was read at
//    entries: (..),               prodrome-wasm `entries(..).entries`
//    order: (..),                 prodrome-wasm `entries(..).order`
//    records: (..),               prodrome-wasm `entries(..).records`
//    marks: ("<todo>": (back: 15, ahead: 15, values: (..))),
//    explain: ("<todo>": tree),   prodrome-wasm `explain`, optional
//    history: ("<todo>": (..)))   the todo's events, optional
//
// Every layout works in paged export (PDF, PNG, SVG) and in HTML export
// (`--features html`), where the marks become inline SVG through
// `html.frame`.

// --- colour ------------------------------------------------------------------

/// THE ONE SCALE. A fulfillment in [0, 1] is drawn in `colour(v)`, a sample
/// of this gradient: red at 0, through orange and yellow, to green at 1,
/// linear in each OKLCH channel. LOW IS URGENT: a value is how well things go
/// if nothing changes. The colour is continuous; it has no states.
#let fulfillment = gradient.linear(color.oklch(color.red), color.oklch(color.green), space: oklch)

/// The colour of a value in [0, 1].
#let colour(value) = fulfillment.sample(value * 100%)

/// The scale laid bottom (0) to top (1) over the nearest container, so a
/// stroke drawn in a box whose height is the 0–100% range is `colour(v)` at
/// every point, `v` its own height. `alpha` fades it.
#let _upward(alpha: 100%) = gradient.linear(
  ..fulfillment.stops().map(((c, at)) => (c.transparentize(100% - alpha), at)),
  space: fulfillment.space(),
  dir: btt,
  relative: "parent",
)

/// No value at all — `∅`, a todo with no price, or one that does not link.
/// Absence is not a zero, so it is never a colour of the scale.
#let unpriced = rgb("#9b9b9b")
#let ink = rgb("#222222")
#let _faint = luma(205)
#let _quiet = luma(125)

/// The words for a value, "Problem" below 0.5, "Watch" below 0.7, "Fine" from
/// there, "Unpriced" for no number: `"absent"` (a row reading `∅`) or `none`
/// (a mark or a node reading `∅`, or a row that does not link). Words only;
/// the colour is `colour`'s.
#let state-of(value) = if value == none or value == "absent" {
  "Unpriced"
} else if value < 0.5 {
  "Problem"
} else if value < 0.7 {
  "Watch"
} else {
  "Fine"
}

/// A value as the percentage the CLI prints (`0.42` is `42%`), `∅` for
/// absent, `—` for none.
#let percent(value) = if value == "absent" { "∅" } else if value == none { "—" } else {
  str(int(calc.round(value * 100))) + "%"
}

// --- instants ----------------------------------------------------------------

/// A naive instant's digits (`YYYY-MM-DD`, then `THH:MM:SS` or ` HH:MM:SS`
/// and any fraction) as a datetime; the fraction is dropped.
#let _instant(iso) = if iso.len() == 10 {
  datetime(year: int(iso.slice(0, 4)), month: int(iso.slice(5, 7)), day: int(iso.slice(8, 10)))
} else {
  datetime(
    year: int(iso.slice(0, 4)),
    month: int(iso.slice(5, 7)),
    day: int(iso.slice(8, 10)),
    hour: int(iso.slice(11, 13)),
    minute: int(iso.slice(14, 16)),
    second: int(iso.slice(17, 19)),
  )
}

/// An instant as a reader says it: "Sep 27, 2026, 12:00", or "Sep 27, 2026"
/// for a date alone.
#let when(iso) = if iso.len() == 10 {
  _instant(iso).display("[month repr:short] [day padding:none], [year]")
} else {
  _instant(iso).display("[month repr:short] [day padding:none], [year], [hour]:[minute]")
}

#let _looks-like-instant(value) = type(value) == str and value.match(regex("^\d{4}-\d{2}-\d{2}([T ]\d{2}:\d{2}:\d{2}.*)?$")) != none

// --- target-agnostic pieces --------------------------------------------------

/// Drawn content: itself on a page, an inline SVG in HTML, sized in `em` of
/// an 11pt text (so the 9pt pie is about 14px beside 17–18px text).
#let _frame(body) = context if target() == "html" { box(html.frame(body)) } else { body }

/// A block a stylesheet can find by class in HTML; a plain block on a page.
#let _div(class, body) = context if target() == "html" {
  html.elem("div", attrs: (class: class), body)
} else { block(body) }

#let _span(class, body) = context if target() == "html" {
  html.elem("span", attrs: (class: class), body)
} else { body }

/// A record's text: Typst markup when the host says its items hold Typst,
/// verbatim otherwise.
#let _text(body, markup) = if body == none or body == "" { [] } else if markup {
  eval(body, mode: "markup")
} else { body }

// --- the two marks -----------------------------------------------------------

/// THE PIE: a value's share of a disc, from twelve o'clock clockwise, in
/// `colour(value)` on a faint track of the same colour, with a thin outline
/// in it too. `value` is a number in [0, 1].
#let pie(value, size: 9pt) = {
  let c = colour(value)
  let r = size / 2
  let edge = 0.6pt
  let at(angle) = (r + r * calc.sin(angle), r - r * calc.cos(angle))
  // One vertex every 5°, so the arc is round at any size this is drawn at.
  let steps = calc.max(1, calc.ceil(value * 72))
  _frame(box(width: size, height: size, baseline: 12%, {
    place(circle(radius: r, fill: c.transparentize(78%)))
    if value >= 1 {
      place(circle(radius: r, fill: c))
    } else if value > 0 {
      place(polygon(fill: c, (r, r), ..range(steps + 1).map(i => at(360deg * value * i / steps))))
    }
    place(dx: edge / 2, dy: edge / 2, circle(radius: r - edge / 2, stroke: edge + c))
  }))
}

/// A value as a list or a page shows it: its pie and its percentage in ink.
/// No number — `"absent"` or `none` — is no pie, only a quiet "unpriced".
#let price(value) = if value == none or value == "absent" {
  _span("price unpriced", text(fill: unpriced)[unpriced])
} else {
  _span("price", [#pie(value)~#text(fill: ink, strong(percent(value)))])
}

/// Runs of consecutive samples that have a value: `∅` breaks the line, and
/// no line is drawn to or from it.
#let _runs(points) = {
  let runs = ((),)
  for point in points {
    if point.at(1) == none {
      if runs.last().len() > 0 { runs.push(()) }
    } else {
      runs.last().push(point)
    }
  }
  runs.filter(run => run.len() > 0)
}

/// THE TRACE: a todo's fulfillment over the days around now, one thick line
/// coloured at every point by `colour` of its own value, in a 0–100% frame.
/// `values` are evenly spaced from `back` days before `at` to `ahead` days
/// after it, both ends included, with one sample falling at `at` itself; a
/// `none` is `∅`, a gap in the line. The past is faded; now is a thin ink
/// line with a dot at now's value.
#let trace(values, at: "", back: 15, ahead: 15, width: 280pt, height: 108pt) = {
  let n = values.len()
  let now = int(calc.round((n - 1) * back / (back + ahead)))
  let gutter = 16pt
  let (top, margin, foot) = (4pt, 18pt, 16pt)
  let (w, h) = (width - gutter - margin, height - top - foot)
  let point((i, value)) = (w * i / (n - 1), h * (1 - value))
  let thick = 3pt
  let line-of(run, paint) = if run.len() == 1 {
    let (x, y) = point(run.first())
    place(dx: x - thick / 2, dy: y - thick / 2, circle(radius: thick / 2, fill: colour(run.first().at(1))))
  } else {
    place(curve(
      stroke: (paint: paint, thickness: thick, cap: "round", join: "round"),
      curve.move(point(run.first())),
      ..run.slice(1).map(p => curve.line(point(p))),
    ))
  }
  let indexed = values.enumerate()
  let past = _runs(indexed.slice(0, now + 1))
  let future = _runs(indexed.slice(now))
  let label(body, fill: _quiet) = text(size: 6.5pt, fill: fill, body)
  let x-of(day) = gutter + w * (day + back) / (back + ahead)
  let date(day) = if at == "" { [] } else {
    (_instant(at.slice(0, 10)) + duration(days: day)).display("[month repr:short] [day padding:none]")
  }

  _frame(box(width: width, height: height, {
    // The frame, its three levels, and the dashed half.
    place(dx: gutter, dy: top, rect(width: w, height: h, stroke: 0.5pt + _faint))
    place(dx: gutter, dy: top + h / 2, line(length: w, stroke: (paint: _faint, thickness: 0.5pt, dash: "dashed")))
    for (level, name) in ((1, "100"), (0.5, "50"), (0, "0")) {
      place(dx: 0pt, dy: top + h * (1 - level) - 4pt, box(width: gutter - 3pt, height: 8pt, align(right + horizon, label(name))))
    }
    // The line, past faded, in a box the height of 0–100% so its gradient
    // spans exactly that range.
    place(dx: gutter, dy: top, box(width: w, height: h, {
      for run in past { line-of(run, _upward(alpha: 35%)) }
      for run in future { line-of(run, _upward()) }
    }))
    // Now.
    let x = x-of(0)
    place(dx: x - 0.35pt, dy: top, rect(width: 0.7pt, height: h, fill: ink))
    let v = values.at(now)
    if v != none {
      let r = 3.2pt
      place(dx: x - r, dy: top + h * (1 - v) - r, circle(radius: r, fill: colour(v), stroke: 0.8pt + white))
    }
    // The dates: the ends, a week either side, and now.
    let dy = top + h + 4pt
    place(dx: gutter, dy: dy, label(date(-back)))
    place(dx: gutter + w - 40pt, dy: dy, box(width: 40pt, align(right, label(date(ahead)))))
    for day in (-7, 7).filter(day => -back < day and day < ahead) {
      place(dx: x-of(day) - 20pt, dy: dy, box(width: 40pt, align(center, label(date(day)))))
    }
    place(dx: x - 20pt, dy: dy, box(width: 40pt, align(center, label(fill: ink, strong(date(0))))))
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

/// The rows in the core's list order (SPEC §6.7), which `prodrome list`
/// prints too: most urgent first, then every row with no number, by id. The
/// order is the core's, so this package never sorts by value itself.
#let listed(data) = data.order.map(todo => data.entries.find(e => e.todo == todo))

#let _trace-of(data, todo) = {
  let marks = data.at("marks", default: (:)).at(todo, default: none)
  if marks != none {
    trace(marks.values, at: data.at, back: marks.back, ahead: marks.ahead)
  }
}

// --- the list ----------------------------------------------------------------

/// The roadmap: every open todo, most urgent first, each with its price and
/// a link to its page. `href` turns a todo id into that link. The closed
/// follow, in a shorter list.
#let roadmap(
  data,
  title: [Roadmap],
  href: todo => "#/todo/" + todo,
  markup: false,
) = {
  let open = listed(data).filter(e => e.state == "open" and _existed(data, e))
  let closed = data.entries.filter(e => e.state != "open").sorted(key: e => e.todo)

  heading(level: 1, title)
  _div("summary", [#open.len() open on #when(data.at) — most urgent first.])

  _div("roadmap", table(
    columns: 2,
    table.header([Price], [Item]),
    ..open
      .map(e => (
        price(e.value),
        [#link(href(e.todo), raw(e.todo)) \ #_text(_body(data, e), markup)],
      ))
      .flatten(),
  ))

  if closed.len() > 0 {
    heading(level: 2)[Closed]
    list(..closed.map(e => [
      #link(href(e.todo), raw(e.todo)) — #e.state #if e.at != "" [since #when(e.at)]
    ]))
  }
}

// --- one item ----------------------------------------------------------------

#let _children = ("terms", "term", "gate", "body", "delta", "pending")

#let _note(value) = if _looks-like-instant(value) { when(value) } else if type(value) == str { value } else {
  repr(value)
}

/// An explanation tree (prodrome-wasm `explain`), each node read the way it
/// prices: its value and its kind ("70%, flat"), its notes, and its parts
/// beneath it. A decoration of the term, never a second reading of it. A
/// node's `null` is always `∅`: every node of a term that explains at all is
/// linked.
#let explanation(node) = {
  let notes = node
    .pairs()
    .filter(((key, _)) => key not in ("kind", "value") and key not in _children)
  [#price(node.value), #node.kind]
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

/// One todo's page: its body, its price and its thirty days, its detail, why
/// it is worth what it is, and its history. `objects` is where an object's
/// file is linked (`objects/<hash>.py` on the site).
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

  _div("facts", [
    #price(entry.value) · #state-of(entry.value) · #entry.state
    #if entry.claimed != "" [ · claimed #entry.claimed]
    #if entry.unconfirmed [ · unconfirmed]
    #if entry.at not in (none, "") [ · since #when(entry.at)]
  ])
  _div("marks", _trace-of(data, todo))

  let detail = _field(record, "detail")
  if detail != "" { _div("detail", _text(detail, markup)) }

  heading(level: 2)[Price]
  let tree = data.at("explain", default: (:)).at(todo, default: none)
  if tree != none { _div("explanation", explanation(tree)) }
  if entry.value == "absent" [Absent: this item has no value, which is not a zero.]
  if entry.unlinked != none [Not priced: #entry.unlinked]

  let events = data.at("history", default: (:)).at(todo, default: ())
  if events.len() > 0 {
    heading(level: 2)[History]
    _div("history", table(
      columns: 4,
      table.header([When], [Event], [By], [Object]),
      ..events
        .map(e => (
          when(e.at),
          e.kind,
          e.actor,
          link(objects + e.hash + ".py", raw(e.hash.slice(0, 12))),
        ))
        .flatten(),
    ))
  }
}
