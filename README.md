# prodrome

A temporal, content-addressed event database with fulfillment-priority
semantics.

[![CI](https://github.com/bmabsout/prodrome/actions/workflows/ci.yml/badge.svg)](https://github.com/bmabsout/prodrome/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

- **Fulfillment as a function of time.** Each objective carries a spec in
  FPL, a small fuzzy temporal logic, whose value in `[0, 1]` says how well the
  objective is being met at any instant; the complement is its urgency. A
  deadline decays, a conjunction is as good as its weakest member, a window
  is sampled over the days ahead, a term can anchor to another objective's
  completion.
- **Time-based queries.** Ask at any time `t` for the objectives that were
  open then and their priorities then, and the answer is what was believed at
  `t`.
- **A CRDT.** Every store is a full replica. Write offline, exchange objects
  with any other store, and all of them converge on the same history without
  a clock to agree on. Concurrent writes to the same field stay visible as a
  conflict for a person to settle; nothing is resolved silently.
- **Content-addressed.** Each object is one file named by the SHA-256 of its
  bytes, so a store is tamper-evident and can be verified from the files
  alone.
- **Trust as policy.** Untrusted writers' claims are stored and shown but do
  not change what is believed.
- **Runs in the browser.** The same core compiles to WebAssembly.

## Installation

The crate is not on crates.io yet. Depend on it by revision:

```toml
[dependencies]
prodrome = { package = "prodrome-core", git = "https://github.com/bmabsout/prodrome", rev = "<commit>" }
```

Requires Rust 1.85 or later.

A host implements `payload::Payload` — the constructor name its records are
stored under, their fields in order, the vocabulary those fields nest, a parse
and a print, and the two readings the folds take (the spec a record carries and
its checklist length). Every type below is generic over it. The default
`reference` feature ships `prodrome::reference::Todo`, the payload
`conformance/` was taken with and a worked example; a host with its own can
turn it off with `default-features = false`.

## Usage

```rust
use std::collections::BTreeSet;

use prodrome::event::{mk_created, mk_spec_revised, TodoId};
use prodrome::fold::{env_at, evaluation_env, flatten, Untrusted};
use prodrome::fpl::{delta_from_hours, fulfillment, instant_of, mk_decay};
use prodrome::literal::Datetime;
use prodrome::reference::Todo;
use prodrome::registers::nodes_of;
use prodrome::store::EventStore;
use prodrome::view::entries;

// A store is a directory. The type parameter is the HOST's record shape: what
// this deployment attaches to a todo, as a `payload::Payload`. `reference::Todo`
// is the one this repository's vectors were taken with; a host implements its
// own. The second argument is the deployment's trust policy (§5): the actors
// whose lifecycle events are claims, not bindings.
let store = EventStore::<Todo>::new(&dir, BTreeSet::new());

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

**Records and payloads.** Five event kinds are the database's — `Created`,
`Completed`/`Cancelled`/`Reopened`, `SpecRevised` — and their fields are its
semantics. The sixth is yours: a record is `KIND(todo, at, actor, <your
fields>)`, where your `Payload` supplies the name, the fields, their parse and
print, and the only two things the folds read out of one (the spec it carries
and how many checklist items it has). The stored bytes are frozen the moment
you ship them, exactly as the database's own are.

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
each parse their arguments, call the core, and return JSON. A record crosses as
its payload's own field names; the module is built with one payload, named once
at the top of `wasm/src/lib.rs`, so a host with its own compiles its own wasm
from this crate with that line changed.

## Documentation

- [`SPEC.md`](SPEC.md): the contract, with the laws in §9.
- [`CHANGELOG.md`](CHANGELOG.md): what changed, and what the stored bytes promise.
- `cargo doc --open`: the API.

## Testing

```console
$ nix flake check              # tests and clippy, in the sandbox
$ nix develop -c cargo test    # the same with a toolchain in hand
```

`conformance/` holds vectors an implementation must reproduce; the laws in
SPEC §9 are property tests over generated logs and two-replica stores.
`conformance/view/*.json` (§6.7) is the one exception to "taken from the
reference": there is no reference to take it from any more, so it is SEEDED
instead — this crate's own log generator (`core/tests/common/mod.rs`'s
`a_log`), under a fixed seed. It is still FROZEN: `cargo test` only reads it,
and regenerating it is a separate, manual step —
`cargo run --example generate_view_vectors -p prodrome-core` — that `nix
flake check` and CI never run.

## Repository layout

```
core/         prodrome-core: literal, payload, event, store, fpl, fold, registers, breaks, view
              plus `reference`, the payload the vectors were taken with
wasm/         prodrome-wasm: the core compiled for the browser, built with that payload
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
