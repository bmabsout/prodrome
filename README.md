# prodrome

The core, in Rust: one evaluator for every host — the box (through a Python
binding), the browser and the phone (through WebAssembly). `SPEC.md` is the
contract; `conformance/` are the vectors the Python reference produced
(`scripts/conformance.py` in the parent repository regenerates them); the
laws in SPEC §9 are the tests.

    nix develop .#rust -c cargo test --manifest-path prodrome/Cargo.toml

## The browser runs it (`wasm/`)

`prodrome-wasm` is `core/` compiled to WebAssembly and nothing else: seven
functions that parse their arguments, call one thing in the core, and print the
answer. No arithmetic, no policy, and no clock — every moment is an argument,
because §1 forbids a clock in the core and a browser's is the least trustworthy
in the system.

    nix build .#prodrome-wasm

Which gives two glues over one `.wasm` (474,073 bytes; 187 KB gzipped):
`$out/web/` for the app and `$out/nodejs/` for `scripts/web-test.sh`.
`scripts/build-web.sh` copies the first beside the bundle from
`$SUZATARY_PRODROME_WASM`, exactly as it copies uPlot — that script is a
bundler and Nix is the build.

| export           | asks                                                        |
| ---------------- | ----------------------------------------------------------- |
| `verify_objects` | §3: do these bytes hash to these names, and do they form one DAG under these heads |
| `fold`           | §6.1–6.5: what does the chain believe at an instant           |
| `registers`      | §6.6: which registers have more than one live write           |
| `fulfillment`    | §7: what is this term worth now                               |
| `explain`        | §7: what is that number made of                               |
| `series_knots`   | §7 knots: what is that term's curve over a window             |
| `term_json`      | §2 → §7: a stored term's canonical print, as the JSON shape   |

JSON and strings at the boundary. `fold`'s answers are `suzatary/view.py`'s
shapes, so what comes back is the wire the web app already speaks; `env` and
`history` are §7's own shapes, so a fold's answer is a legal argument to
`fulfillment` and `series_knots` with nothing rewritten in between — a
translation step is where a second reading grows.

### How it is built

The wasm32 target comes from NIXPKGS: `rustc --print target-list` has
wasm32-unknown-unknown and `$(rustc --print sysroot)/lib/rustlib/` carries its
std, so there is no `rust-overlay` and no `fenix` input to keep in step. That
rustc links wasm with the system `lld`, which the `.#rust` shell carries.
Dependencies are vendored from `Cargo.lock` (`importCargoLock`), so the build
is pure and reaches no network. `wasm-bindgen` is pinned on BOTH sides —
`=0.2.127` in `wasm/Cargo.toml`, `wasm-bindgen-cli_0_2_127` in `flake.nix` —
because the generator and the runtime negotiate over a schema version compiled
into each. `[profile.wasm-release]` is the browser's build and nothing else's
(`opt-level = "s"`, fat LTO, one codegen unit), and `wasm-opt -Os` runs over
what bindgen emits.

### What the browser now does with it

- **Verifies the chain.** `#/chain`'s verdict is `verify_objects`, so the tab
  is no longer a second implementation of §3. It rehashes every object AND
  parses it as an envelope — which the old hand-written path could not, having
  no parser — and reports the causal order `linearise` put them in. That path
  (`web/src/verify.ts`, SubtleCrypto) survives as a labelled FALLBACK, and the
  page names whichever one ran.
- **Checks the graph.** `#/graph` folds the objects here and recomputes every
  drawn knot: same instants, same `exact`, every value within §9.8's 1e-9. The
  server's numbers, checked by the reader's own machine — one evaluator, run
  twice.
- **Folds offline.** With the objects cached (`web/src/objects.ts`, IndexedDB),
  a browser with no network folds them itself instead of showing yesterday's
  answer, and the banner says so: the numbers are this moment's, and what may
  have moved is the chain.

`web/test/wasm.test.ts` runs the same `.wasm` under node against the vectors —
564 objects rehashed and parsed, 900 fulfillment samples to 1e-9, 1429 knots at
the reference's instants exactly — because everything between the Rust and the
tab (bindgen's glue, `wasm-opt`'s rewrite, the JSON shapes and their guards)
is outside `cargo test`. `web/e2e/core.test.ts` then checks that a real browser
gets hold of it at all, which is the one thing a silent fallback would hide.

## Status

One row per module of the crate, and what it is checked against. A row is
"done" only where a vector file or the live chain says so — a module with
tests of its own and no vector behind it is not done, it is untested against
the reference. What a module CLAIMS is what its tests MEASURE.

| module       | SPEC     | state | checked against |
| ------------ | -------- | ----- | --------------- |
| `literal`    | §2       | done  | `conformance/literals.json` — 39 `values`, 564 `objects` (print∘parse = id, sha256(print) = name); property tests for no-panic and print stability |
| `event`      | §4, §3   | done  | the same 564 objects through the CLOSED vocabulary as typed envelopes; the vocabulary set itself; every stored spec EVALUATES, not just prints back |
| `store`      | §3       | done  | `conformance/dag.json` — 40 DAGs, each linearisation, tips, parents and `verify` finding; the live `events/` chain reads 564 objects, verifies clean, and in the generator's order |
| `fpl`        | §7       | done  | `conformance/fpl.json` — 150 terms, prints and JSON byte-identical, 900 fulfillment samples, every `normalized` print and every `explain` tree; largest float deviation 5.6e-16 against a 1e-9 budget. §9.5's constructor laws on random terms in `tests/fpl_laws.rs` |
| `fold`       | §6.1–6.5 | done  | `conformance/folds.json` — all 120 logs: `env` and `history_at` by kind and instant, `specs`, `content` and `flatten` by canonical print, byte-exact. §9.2–9.4 as properties in `tests/fold_laws.rs`; the live chain in `tests/live.rs` |
| `registers`  | §6.6     | done  | `conformance/dag.json`'s `conflicts` and `env` — all 40 DAGs, by exact object hash. §9.6 as properties over real two-replica stores; the registers equal the folds on every DAG, on all 120 logs, and on the live chain |
| `breaks`     | §7 knots | done  | `conformance/series.json` — 60 windows, 1429 knots, instants and `exact` identical, deviation 3.6e-16; §9.7 checked as a law in `tests/series_vectors.rs` |
| `wasm/`      | the boundary | done | the SAME `.wasm` a browser fetches, loaded under node and driven against the vectors (`web/test/wasm.test.ts`): 564 objects rehashed, parsed and linearised; 900 fulfillment samples within 1e-9; 1429 knots at the reference's instants exactly. A real Chromium reaches it in `web/e2e/core.test.ts` |

`conformance/live.json` is the one vector that is not seeded: this box's own
564-object chain, folded by the reference at a recorded instant, with the tip
it was folded over written beside it. `tests/live.rs` reproduces all five
folds and the registers on it and refuses loudly when the chain has moved —
regenerate with `scripts/conformance.py` when it does.

### The one seam between the layers, closed

A stored `SpecRevised` or `Authored` carries a §7 `Term`, and since the halves
met that field IS `fpl::Term` — no wrapper, because a wrapper is a second name
for one value and a place for a second reading to grow. `literal` owns the
grammar: one printer (`print_literal`, `print_float`, `print_str`) and one
parser (`parse_literal` against a `Vocabulary`), and `fpl` reaches it through
`Term::{to_value,from_value}` while contributing `fpl::TERM_SIGNATURES` — the
§7 half of the vocabulary `event::EVENT_VOCABULARY` reads a stored object
against. So §7's per-field bounds now travel with a stored spec: a spec that
parses is a spec that evaluates, which the placeholder could not promise.

Two types did NOT merge, deliberately. `literal::Datetime`/`Timedelta` are the
grammar's records — CPython's field bounds and normalisation, no arithmetic —
while evaluation needs `now + δ`, `done − anchor` and a window cut into 64, and
takes those from `chrono`. `fpl::instant_of`/`datetime_of` are the two total
conversions, and `fold` reads an event's `at` through the first of them. And
`Term` holds `f64`, so the event kinds that carry one are `PartialEq` and no
longer `Eq`: an `Envelope`'s identity is its print and its hash, never a
derived `Eq`, so nothing was using it.
