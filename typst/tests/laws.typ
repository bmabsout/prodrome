// The package's laws, checked by compiling this file: every `assert` holds or
// the compile fails. `nix flake check` compiles it (`checks.prodrome-typst`).
#import "@local/prodrome-typst:0.1.0": *

#let samples = range(0, 101).map(i => i / 100)

// THE SCALE IS LINEAR IN EACH OKLCH CHANNEL, from red at 0 to green at 1:
// colour(v) = oklch(65.95 + 8.01·v %, 0.227 − 0.007·v, 28.44 + 115.85·v).
#for v in samples {
  let (l, c, h, a) = colour(v).components()
  assert(calc.abs(l / 1% - (65.95 + 8.01 * v)) < 0.01, message: "lightness at " + str(v))
  assert(calc.abs(c - (0.227 - 0.007 * v)) < 0.0005, message: "chroma at " + str(v))
  assert(calc.abs(h / 1deg - (28.44 + 115.85 * v)) < 0.01, message: "hue at " + str(v))
  assert.eq(a, 100%)
}

// Its ends are Typst's own red and green.
#assert.eq(colour(0).to-hex(), color.red.to-hex())
#assert.eq(colour(1).to-hex(), color.green.to-hex())

// It is continuous: a hundredth of a value is never more than a small step
// of colour.
#for (u, v) in samples.zip(samples.slice(1)) {
  let (lu, cu, hu, _) = colour(u).components()
  let (lv, cv, hv, _) = colour(v).components()
  assert(hv > hu and (hv - hu) / 1deg < 1.2, message: "hue step at " + str(u))
  assert(lv > lu and (lv - lu) / 1% < 0.1, message: "lightness step at " + str(u))
}

// The words stay words, at their boundaries.
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

// The marks draw at every value, and a trace with gaps, lone points and no
// value at all.
#for v in (0, 0.001, 0.25, 0.5, 0.999, 1) { pie(v) }
#price("absent") #price(none) #price(0.7)
#trace((0, 0.5, 1, none, 0.3, none, 0.2, 0.4, 0.6), at: "2026-09-27T12:00:00", back: 4, ahead: 4)
#trace((none,) * 9, at: "2026-09-27T12:00:00", back: 4, ahead: 4)
#trace((0.5,) * 121, at: "2026-09-27T12:00:00")
