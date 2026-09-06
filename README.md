# prodrome

The core, in Rust: one evaluator for every host — the box (through a Python
binding), the browser and the phone (through WebAssembly). `SPEC.md` is the
contract; `conformance/` are the vectors the Python reference produced
(`scripts/conformance.py` in the parent repository regenerates them); the
laws in SPEC §9 are the tests.

    nix develop .#rust -c cargo test --manifest-path prodrome/Cargo.toml

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
