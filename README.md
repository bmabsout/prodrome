# prodrome

The core, in Rust: one evaluator for every host — the box (through a Python
binding), the browser and the phone (through WebAssembly). `SPEC.md` is the
contract; `conformance/` are the vectors the Python reference produced
(`scripts/conformance.py` in the parent repository regenerates them); the
laws in SPEC §9 are the tests.

    nix develop .#rust -c cargo test --manifest-path prodrome/Cargo.toml

## Status, one row per module

What a module claims is what its tests measure — the vectors it reproduces,
not the code it contains.

| module        | spec       | status  | evidence |
| ------------- | ---------- | ------- | -------- |
| `literal`     | §2         | pending | `conformance/literals.json` |
| `event`       | §4         | pending | — |
| `store`       | §3         | pending | `conformance/dag.json` |
| `fpl`         | §7         | **built** | `conformance/fpl.json`: 150 terms, prints and JSON byte-identical, 900 fulfillment samples, every `normalized` print and every `explain` tree — largest float deviation 5.6e-16 against a 1e-9 budget. §9.5 laws on random terms in `tests/fpl_laws.rs`. |
| `fold`        | §6.1–6.5   | pending | `conformance/folds.json` |
| `registers`   | §6.6       | pending | `conformance/dag.json` |
| `breaks`      | §7 knots   | **built** | `conformance/series.json`: 60 windows, 1429 knots, instants and `exact` identical, deviation 3.6e-16; §9.7 checked as a law in `tests/series_vectors.rs`. |

`fpl` carries its own §2 printer and parser for the term constructors while
`literal` is pending; the two are written to the same rules and unify into
`literal` without a behaviour change.
## Status

One row per module of the crate, and what it is checked against. A row is
"done" only where a vector file or the live chain says so — a module with
tests of its own and no vector behind it is not done, it is untested against
the reference.

| module       | SPEC   | state | checked against |
| ------------ | ------ | ----- | --------------- |
| `literal`    | §2     | done  | `conformance/literals.json` — 39 `values`, 564 `objects` (print∘parse = id, sha256(print) = name); property tests for no-panic and print stability |
| `event`      | §4, §3 | done  | the same 564 objects through the CLOSED vocabulary as typed envelopes; the vocabulary set itself |
| `store`      | §3     | done  | `conformance/dag.json` — 40 DAGs, each linearisation, tips, parents and `verify` finding; the live `events/` chain reads 564 objects, verifies clean, and in the generator's order |
| `fpl`        | §7     | —     | `conformance/fpl.json`, `conformance/series.json` |
| `fold`       | §6.1–5 | —     | `conformance/folds.json` |
| `registers`  | §6.6   | —     | `conformance/dag.json`'s `conflicts` |
| `breaks`     | §7     | —     | `conformance/series.json` |

### The one seam between the layers

A stored `SpecRevised` or `Authored` carries a §7 `Term`. Until `fpl` lands,
`event::Spec` holds that term as the value it prints as, with §7's vocabulary
checked recursively (`event::TERM_SIGNATURES`) and its per-field bounds left
to `fpl` — enough for everything §3 and §4 promise, and deliberately not
enough to evaluate one, since fulfillment is computed in exactly one place.
`Spec::{from_value,to_value}` is where `fpl::Term` replaces it, and
`TERM_SIGNATURES` is the table that moves to `fpl` with it.
