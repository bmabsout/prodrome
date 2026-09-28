// The package's laws, checked by compiling this file: every `assert` holds or
// the compile fails. `nix flake check` compiles it (`checks.prodrome-typst`).
#import "@local/prodrome-typst:0.1.0": *

// An instant reads as a date, whatever its separator and fraction.
#assert.eq(when("2026-09-27T19:51:25.123456"), "Sep 27, 2026, 19:51")
#assert.eq(when("2026-09-24 12:00:00"), "Sep 24, 2026, 12:00")
#assert.eq(when("2026-09-24"), "Sep 24, 2026")
