# prodrome

The core, in Rust: one evaluator for every host — the box (through the Python
binding in `py/`, which is built), the browser and the phone (through
WebAssembly, which is not). `SPEC.md` is the contract; `conformance/` are the
vectors the Python reference produced (`scripts/conformance.py` in the parent
repository regenerates them); the laws in SPEC §9 are the tests.

    nix develop .#rust -c cargo test --manifest-path prodrome/Cargo.toml
    nix flake check          # the same suites, plus clippy, from the vendored lock

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

## The Python binding (`py/`)

`prodrome-py` is a second workspace member: a PyO3 module named `prodrome`
that exposes this crate to the Python layers. It is a CONVERSION LAYER and
nothing else — every function parses its arguments, calls exactly one thing in
`prodrome-core`, and prints the answer back. There is no `match` over a
`TermF` in it and no rule about trust; a body here that decided anything would
be the second evaluator SPEC §1 forbids.

    nix build .#prodrome-py
    nix develop .#triage -c python3 -c "import prodrome; print(prodrome.SAMPLES)"

The `.#triage` shell's python carries it, so every gate that shell runs — the
unittest suite included — has `import prodrome` available. Built by
`buildPythonPackage` + nixpkgs' `maturinBuildHook` from
`rustPlatform.importCargoLock ./prodrome/Cargo.lock`, so a build fetches
nothing; `nix flake check` runs `cargo test` and clippy over the workspace the
same way. NOT abi3: Nix builds it against the exact interpreter the shell
carries, and portability nothing consumes is not worth the fast paths the
stable ABI forbids.

Everything crosses the boundary as the value's OWN IDENTITY, so a
disagreement between the two implementations is a disagreement in a string:

| Python sees            | is                                                     |
| ---------------------- | ------------------------------------------------------ |
| an object, event, term | its canonical §2 print, a `str`                         |
| an instant             | `datetime.isoformat()`, µs only when non-zero           |
| an environment         | `{todo: {"kind": "Completed"\|"Cancelled", "at": iso}}` |
| `explain` / `to_json`  | the §7 wire dicts                                       |
| a refusal              | `ValueError`                                            |

The surface: `parse_literal`/`print_literal`/`seal_hash` (§2–3);
`Store(root, untrusted)` with `read_dag`/`objects`/`events`/`tips`/`tip`/
`verify`/`append`/`merge`/`adopt` (§3); `env_at`, `specs_at`, `authored_at`,
`flatten`, `history_at`, a `History` handle and `fold_registers` returning a
`Folded` with `env_of`/`specs_of`/`content_of`/`conflicts_of`/`extend` (§6);
`fulfillment`, `explain`, `normalize`, `to_json`, `from_json`, `checklist`,
`series_knots` (§7).

One deliberate narrowing: `parse_literal(text)` reads a stored OBJECT, not any
value of the grammar. The reference's parser DISPATCHES INTO the `mk_*`
validators, so `Flat(value=2.0)` is a refusal there; `literal::parse_literal`
alone knows names and arity and would hand back a value §7 forbids. Going
through `parse_envelope` is what makes the two refuse the same texts — which
`tests/test_differential.py` checks against the reference directly.

### The differential test is the switch's gate

`tests/test_differential.py` (in the parent repository) runs the Python
reference and this crate in ONE process, on the same generated inputs and on
the live chain, and compares what each answers NOW. That is a stronger claim
than the vectors make: a vector file pins the core against numbers the
reference produced once, and a reference that has changed since is a reference
nothing is checking. Prints, hashes, linearisations, register frontiers and
`verify` findings are compared byte for byte; fulfillments, `explain` trees
and knot values to 1e-9. Its generators are `scripts/conformance.py`'s and
`tests/test_laws.py`'s, imported rather than written again.

Until it passes there are two readings of the semantics and no evidence they
are one, so it is what the Python evaluator's deletion waits on.

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
