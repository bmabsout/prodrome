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
