// The package's laws, checked by compiling this file: every `assert` holds or
// the compile fails. `nix flake check` compiles it (`checks.prodrome-typst`).
#import "@local/prodrome-typst:0.1.0": *

#let samples = range(0, 101).map(i => i / 100)

// The words for a value are the design system's, as are its scale and its
// marks, whose laws are `typst-design`'s `tests/laws.typ`. What the layouts
// rely on of them is held here: the words stay words, at their boundaries.
#assert.eq(samples.map(state-of).dedup(), ("Problem", "Watch", "Fine"))
#assert.eq(state-of(0.49), "Problem")
#assert.eq(state-of(0.5), "Watch")
#assert.eq(state-of(0.7), "Fine")
#assert.eq(state-of("absent"), "Unpriced")
#assert.eq(state-of(none), "Unpriced")

// An instant reads as a date, whatever its separator and fraction.
#assert.eq(when("2026-09-27T19:51:25.123456"), "Sep 27, 2026, 19:51")
#assert.eq(when("2026-09-24 12:00:00"), "Sep 24, 2026, 12:00")
#assert.eq(when("2026-09-24"), "Sep 24, 2026")

// The marks draw at every value, and the thirty days with gaps, lone points
// and no value at all.
#for v in (0, 0.001, 0.25, 0.5, 0.999, 1, "absent", none) { price(v) }
#let days(values, back: 4, ahead: 4) = (
  at: "2026-09-27T12:00:00",
  marks: (t: (back: back, ahead: ahead, values: values)),
)
#trace-of(days((0, 0.5, 1, none, 0.3, none, 0.2, 0.4, 0.6)), "t")
#trace-of(days((none,) * 9), "t")
#trace-of(days((0.5,) * 121, back: 15, ahead: 15), "t")
#assert.eq(trace-of((at: "2026-09-27T12:00:00"), "t"), none)

// AN EXPLANATION READS IN WORDS. Every kind of term says what it is with
// percentages, spans and dates, and never a field's name.
#let plain(c) = if type(c) == str { c } else if c.func() == raw { c.text } else if c.has("text") {
  plain(c.text)
} else if c.has("children") { c.children.map(plain).join(default: "") } else if c.has("body") {
  plain(c.body)
} else if c.func() == [ ].func() { " " } else { "" }
#let says(node) = plain(_says(node)).trim()
#let at = "2026-09-27T12:00:00"
#let readings = (
  ((kind: "flat", value: 0.7), "flat"),
  (
    (kind: "decay", value: 0.42, start: 0.55, end: 0.05, endDate: "2026-09-29T17:00:00", leadUpHours: 72),
    "falling from 55% to 5% by Sep 29, 17:00, over 3 days",
  ),
  (
    (kind: "decay", value: 1, start: 0.55, end: 0.05, endDate: "2026-09-29T17:00:00", leadUpHours: 36, startDate: "2026-09-01T00:00:00"),
    "falling from 55% to 5% by Sep 29, 17:00, over 36 hours; 100% before Sep 1, 00:00",
  ),
  (
    (kind: "curve", value: 0.6, points: ((at: "2026-09-01T00:00:00", value: 0.9), (at: "2026-09-15T00:00:00", value: 0.5, label: "x"), (at: "2026-10-01T00:00:00", value: 0.2))),
    "along a curve from 90% on Sep 1, 00:00 to 20% on Oct 1, 00:00, by way of one other date",
  ),
  (
    (kind: "conj", value: 0.5, p: -1.0, certifies: 0.3, shares: (0.6, 0.4), terms: ((kind: "flat", value: 0.3), (kind: "flat", value: 0.9))),
    "a mean held down by the weakest of 2 parts, so no part is below 30%",
  ),
  ((kind: "offset", value: 0.8, delta: 0.5, term: (kind: "flat", value: 0.6)), "pulled 50% of the way up to 100%"),
  ((kind: "offset", value: 0.3, delta: -0.5, term: (kind: "flat", value: 0.6)), "pulled 50% of the way down to 0%"),
  (
    (kind: "gate", value: 0.6, gate: (kind: "flat", value: 0.9), body: (kind: "flat", value: 0.6)),
    "its own price, counted only as far as its gate is met",
  ),
  ((kind: "shift", value: 0.6, deltaHours: 48, term: (kind: "flat", value: 0.6)), "as its part will read in 2 days"),
  ((kind: "shift", value: 0.6, deltaHours: -6, term: (kind: "flat", value: 0.6)), "as its part read 6 hours earlier"),
  (
    (kind: "within", value: 0.6, windowHours: 168, p: 1.0, peakAt: "2026-09-30T12:00:00", peakShare: 0.1, term: (kind: "flat", value: 0.6)),
    "the average over the next 7 days, weighed most at Sep 30, 12:00",
  ),
  ((kind: "importance", value: 0.36, w: 2.0, term: (kind: "flat", value: 0.6)), "its part made more urgent, to the power 2"),
  (
    (kind: "after", value: 0.5, event: "a", anchor: at, bound: "pending", needsHours: 24, term: (kind: "flat", value: 0.2), pending: (kind: "flat", value: 0.5)),
    "waiting on a, which needs 1 day once it is done",
  ),
  (
    (kind: "after", value: 0.2, event: "a", anchor: at, bound: "completed", term: (kind: "flat", value: 0.2), pending: (kind: "flat", value: 0.5)),
    "since a was done, planned for Sep 27, 12:00",
  ),
  (
    (kind: "after", value: 1, event: "a", anchor: at, bound: "cancelled", term: (kind: "flat", value: 0.2), pending: (kind: "flat", value: 0.5)),
    "a was cancelled, so this no longer waits on it",
  ),
  (
    (kind: "recur", value: 0.5, todo: "brush", anchor: at, bound: "pending", term: (kind: "flat", value: 0.2), pending: (kind: "flat", value: 0.5)),
    "a recurring brush, not done yet",
  ),
  (
    (kind: "recur", value: 0.9, todo: "brush", anchor: at, bound: "tended", tended: "2026-09-25T12:00:00", agoHours: 50.5, term: (kind: "flat", value: 0.9), pending: (kind: "flat", value: 0.5)),
    "a recurring brush, last done Sep 25, 12:00, about 2 days ago",
  ),
  (
    (kind: "periodic", value: 0.9, periodHours: 168, anchor: at, cycleStart: "2026-09-22T00:00:00", term: (kind: "flat", value: 0.9)),
    "repeating every 7 days; this cycle began Sep 22, 00:00",
  ),
  (
    (kind: "piecewise", value: 1, pieces: 1, since: "2026-09-24T12:00:00", term: (kind: "flat", value: 1)),
    "the price set on Sep 24, 12:00, one of two dated prices",
  ),
  ((kind: "piecewise", value: 0.9, pieces: 2, since: "", term: (kind: "flat", value: 0.9)), "the first of 3 dated prices"),
  (
    (kind: "offsetBy", value: 0.7, delta: (kind: "flat", value: 0.5), term: (kind: "flat", value: 0.6)),
    "its own price, offset by another",
  ),
  ((kind: "ref", value: 0.6, todo: "b"), "as b is priced"),
  ((kind: "absent", value: none), "no claim on attention"),
)
#for (node, words) in readings {
  assert.eq(says(node), words)
  // A field a reader would only know from the code: camelCase, or a
  // one-word note that is not a word of the sentence.
  let fields = ("p", "w", "delta", "anchor", "bound", "certifies", "shares", "pieces", "points", "start", "end")
  for key in node.keys().filter(k => k.match(regex("[A-Z]")) != none or k in fields) {
    assert(not says(node).contains(regex("\\b" + key + "\\b")), message: key + " shown in: " + says(node))
  }
  explanation(node)
}
