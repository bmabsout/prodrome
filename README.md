# prodrome

The core, in Rust: one evaluator for every host — the box (through the Python
binding in `py/`), the browser and the phone. Since 2026-09-06 that sentence
is literal rather than aspirational: the parent repository's Python holds the
term and event RECORDS, the `mk_*` smart constructors and the canonical
printer, and every number, fold, normal form and knot it answers with came
from here. There is no second implementation to fall back to.

`SPEC.md` is the contract; `conformance/` are the vectors the Python reference
produced WHILE IT WAS STILL A REFERENCE (`scripts/conformance.py` in the parent
repository regenerates them, but regeneration now reads this crate through the
binding, so it re-states the vectors rather than re-deriving them — they are
frozen evidence of what the reference said, and that is their value); the laws
in SPEC §9 are the tests.

    nix develop .#rust -c cargo test --manifest-path prodrome/Cargo.toml
    nix flake check          # the same suites, plus clippy, from the vendored lock

## The browser runs it (`wasm/`)

`prodrome-wasm` is `core/` compiled to WebAssembly and nothing else: eight
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
| `entries`        | §6.7: every todo as the folds see it — the composition, once  |
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
| `view`       | §6.7     | done  | `conformance/view.json` — the LAST derivation in this directory, generated from the Python composition before this module existed: 40 forked DAGs at five instants under both trust policies (400 cases, 1,180 entries; 31 with a refused claim, 93 with a conflicted register), plus the live chain's 250 todos at their own tip. Every field of every row, `value` to 1e-9, largest deviation 1.1e-16. §9.9 as a property in `tests/fold_laws.rs`, stated through the route each field does NOT take |
| `wasm/`      | the boundary | done | the SAME `.wasm` a browser fetches, loaded under node and driven against the vectors (`web/test/wasm.test.ts`): 564 objects rehashed, parsed and linearised; 900 fulfillment samples within 1e-9; 1429 knots at the reference's instants exactly. A real Chromium reaches it in `web/e2e/core.test.ts` |

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
`fulfillment`, `fulfillments`, `explain`, `normalize`, `to_json`, `from_json`,
`checklist`, `series_knots` (§7).

`fulfillments` is `fulfillment` for a LIST, and it exists for one reason: the
environment is converted out of Python per call, so pricing 184 todos against
177 bindings parsed 177 instants 184 times — 31.5 ms of a 37 ms fold. It
decides nothing the singular does not (the same core call per term, in the
order given); it converts the environment first. `suzatary/fpl.py`'s `prices`
is the caller, and `tests/test_fpl.py::PricingAList` pins the two against each
other term for term, because speed is all it may buy.

One deliberate narrowing: `parse_literal(text)` reads a stored OBJECT, not any
value of the grammar. The reference's parser DISPATCHES INTO the `mk_*`
validators, so `Flat(value=2.0)` is a refusal there; `literal::parse_literal`
alone knows names and arity and would hand back a value §7 forbids. Going
through `parse_envelope` is what makes the two refuse the same texts — which
`tests/test_differential.py` checks against the reference directly.

### The differential test was the switch's gate, and the switch is thrown

`tests/test_differential.py` (in the parent repository) ran the Python
reference and this crate in ONE process, on the same generated inputs and on
the live chain, and compared what each answered NOW — a stronger claim than
the vectors make, because a vector file pins the core against numbers the
reference produced once and a reference that has changed since is a reference
nothing is checking. Prints, hashes, linearisations, register frontiers and
`verify` findings byte for byte; fulfillments, `explain` trees and knot values
to 1e-9. It passed, and on 2026-09-06 the Python evaluator was deleted.

So most of that file is gone with it. What is left is what still HAS two
sides: the §2 printer (`suzatary/literals.py` prints, this crate parses and
prints back), the `mk_*` bounds (both parsers dispatch into their validators,
so a refusal must be a refusal on both hosts), and §3's linearisation, read
and write path (`store.linearise` and `read_dag` are still Python — this crate
exposes a linearisation only through a `Store` on disk — and so is the append
that holds the flock). Running both sides of a DELEGATED function would be
running one side twice and calling the agreement evidence.

What checks this crate now: `conformance/*.json`, SPEC §9's laws in
`core/tests/`, `tests/live.rs` on the box's own chain, and the parent
repository's `tests/test_laws.py` and `tests/test_registers.py`, which state
the same laws through the Python surface and therefore state them about this
crate.

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
