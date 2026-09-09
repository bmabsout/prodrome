# prodrome

A temporal, content-addressed event database with fulfillment-priority
semantics.

[![CI](https://github.com/bmabsout/prodrome/actions/workflows/ci.yml/badge.svg)](https://github.com/bmabsout/prodrome/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

- **Content-addressed.** Every object is one file named by the SHA-256 of its
  bytes. Reads re-hash before they parse.
- **Causal, not clocked.** Objects name their parents; history is a DAG. A
  writer's timestamp is data for queries and never decides a merge.
- **Time-travelling queries.** A fold answers what was believed at any
  instant. Registers expose concurrent writes as conflicts instead of picking
  a winner.
- **Fulfillment as a function of time.** FPL, a small fuzzy temporal logic,
  gives each todo a spec whose value in `[0, 1]` at an instant is how well it
  is being met; the complement is urgency.
- **Replicas.** Stores exchange objects and merge heads; a trust policy decides
  which writers' claims bind.
- **Runs in the browser.** The same core compiles to WebAssembly.

## Installation

The crate is not on crates.io yet. Depend on it by revision:

```toml
[dependencies]
prodrome = { package = "prodrome-core", git = "https://github.com/bmabsout/prodrome", rev = "<commit>" }
```

Requires Rust 1.85 or later.

## Usage

```rust
use std::collections::BTreeSet;

use prodrome::event::{mk_created, mk_spec_revised, TodoId};
use prodrome::fold::{env_at, evaluation_env, flatten, Untrusted};
use prodrome::fpl::{delta_from_hours, fulfillment, instant_of, mk_decay};
use prodrome::literal::Datetime;
use prodrome::registers::nodes_of;
use prodrome::store::EventStore;
use prodrome::view::entries;

// A store is a directory. The second argument is the deployment's trust
// policy (§5): the actors whose lifecycle events are claims, not bindings.
let store = EventStore::new(&dir, BTreeSet::new());

// An event carries the instant its writer stamped on it. The store has no
// clock: `at` is data, and nothing here reads the machine's.
let at = Datetime::new(2026, 9, 8, 9, 0, 0, 0)?;
store.append(mk_created("todo-1", at, "bassel", "publish the crate", "")?, None)?;

// A spec is an FPL term — what this todo is worth as a function of time.
let deadline = instant_of(Datetime::new(2026, 9, 15, 17, 0, 0, 0)?);
let spec = mk_decay(0.55, 0.05, deadline, delta_from_hours(72.0), None)?;
store.append(mk_spec_revised("todo-1", at, "bassel", spec, "")?, None)?;

// Read the DAG back: every object rehashed on the way in, in causal order.
let objects = store.read_dag_named()?;
assert_eq!(objects.len(), 2);
assert!(store.verify().is_empty(), "no finding against this store");

// Fold at an instant, and price what the fold believes.
let now = Datetime::new(2026, 9, 14, 9, 0, 0, 0)?;
let untrusted = Untrusted::none();
let events = store.events()?;
let env = env_at(&events, now, &untrusted);
let functions = flatten(&events, now, &untrusted)?;
let todo = TodoId::new("todo-1")?;
let value = fulfillment(&functions[&todo], instant_of(now), &evaluation_env(&env));
assert!((0.0..=1.0).contains(&value));

// Or the whole composition at once: one row per todo the chain mentions,
// each with its outcome, its function, its price and its conflicts (§6.7).
let rows = entries(&nodes_of(&objects), now, &untrusted)?;
assert_eq!(rows.len(), 1);
assert_eq!(rows[0].state(), "open");
assert_eq!(rows[0].value(), Some(value));
```

This example is the crate's doctest; `cargo test` compiles and runs it.

## Concepts

**Objects and the DAG.** An object is a `Sealed(prev, event)` with one parent
or a `Woven(parents, event)` merge with several. Its name is the hash of its
canonical print. `verify` reports anything that does not hash to its name,
rest on a missing parent, or form a cycle.

**Folds.** `env_at` gives each todo's outcome at an instant, `specs_at` its
spec in force, `authored_at` its current record, and `flatten` its whole
history as one function of time. `history` is the environment as a function
of time.

**Registers.** The same DAG read per `(kind, todo)`: a register holds the
frontier of writes nothing later descends from. One write is a value, more is
a conflict the caller is shown. On any DAG the registers agree with the folds.

**Trust.** A set of untrusted actor names. Their lifecycle events are stored
and shown as claims and never folded; their content records still bind.

**FPL.** Terms such as `Flat`, `Decay`, `Conj` (a power mean, so the weakest
member dominates), `Within` (sampled over a window), `After` (anchored to
another todo's completion) and `Piecewise`. `fulfillment` evaluates a term at
an instant; `explain` returns the same computation with a value at every node;
`series_knots` gives a term's curve over a window.

**View.** `entries` composes the above into one row per todo: outcome, claim,
function, value, current content, conflicts, and the objects that mention it.

The precise semantics are in [`SPEC.md`](SPEC.md).

## Storage format

```
<root>/
  objects/<sha256>.py     one object, its canonical print
  HEAD                    the single tip
  refs/<sha256>           one file per head, only while there is more than one
  .lock                   held during an append
```

Objects are append-only and never rewritten. A print is a Python-literal
expression from a closed grammar (SPEC §2); the parser admits that grammar and
nothing else, and nothing is evaluated.

## WebAssembly

```console
$ nix build .#prodrome-wasm
```

Produces `result/web/` for a bundler and `result/nodejs/` for a script. The
exports (`verify_objects`, `fold`, `registers`, `entries`, `fulfillment`,
`explain`, `series_knots`, `term_json`, `lifecycle`, `seal`, `merge_object`)
each parse their arguments, call the core, and return JSON.

## Documentation

- [`SPEC.md`](SPEC.md): the contract, with the laws in §9.
- `cargo doc --open`: the API.

## Testing

```console
$ nix flake check              # tests and clippy, in the sandbox
$ nix develop -c cargo test    # the same with a toolchain in hand
```

`conformance/` holds vectors an implementation must reproduce; the laws in
SPEC §9 are property tests over generated logs and two-replica stores.

## Repository layout

```
core/         prodrome-core: literal, event, store, fpl, fold, registers, breaks, view
wasm/         prodrome-wasm: the core compiled for the browser
conformance/  the vectors
SPEC.md       the specification
```

## Contributing

Issues and pull requests are welcome. Changes to semantics need a change to
`SPEC.md` in the same pull request, and a law or a vector that pins them.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
