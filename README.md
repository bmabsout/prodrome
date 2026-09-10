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
- **Standing is the host's.** The database does not decide whom to believe: it
  asks your policy whether an event binds or only claims, and keeps both
  readings either way.
- **Runs in the browser.** The same core compiles to WebAssembly.

## This crate's roadmap is a Prodrome

`roadmap/` is a store of objects like any other, holding what this crate has
left to do, and `prodrome-cli` folds it at whatever instant you ask for. The
list is ascending in fulfillment, so the top of it is the most urgent thing.
CI runs `verify` and `list` over the directory on every pull request, which is
why the example cannot go stale.

```console
$ cargo run -p prodrome-cli -- list
$ cargo run -p prodrome-cli -- list --at 2026-09-09
```

The second is the roadmap as it stood on 2026-09-09: two open questions about
the format, both of them since answered.

## Installation

The crate is not on crates.io yet. Depend on it by revision:

```toml
[dependencies]
prodrome = { package = "prodrome-core", git = "https://github.com/bmabsout/prodrome", rev = "<commit>" }
```

Requires Rust 1.85 or later.

The `prodrome` binary is `prodrome-cli` in the same workspace —
`cargo install --path cli`, or `nix build .#prodrome-cli`. It is a host like
any other: the reference payload, the reference policy, and a clock, over the
core.

A host implements `payload::Payload` — the constructor name its records are
stored under, their fields in order, the vocabulary those fields nest, a parse
and a print, and the two readings the folds take (the spec a record carries and
its checklist length). Every type below is generic over it. The default
`reference` feature ships `prodrome::reference::Todo`, the payload
`conformance/` was taken with and a worked example; a host with its own can
turn it off with `default-features = false`.

A host also supplies a `policy::Policy` — one function from an event to
`Binds` or `Claims`, which is the whole of §5. `policy::Untrusted` is the
reference one (a roster of actor names whose lifecycle events claim) and
`Untrusted::none()` stands behind every writer; a deployment with a different
rule implements the trait instead.

## Usage

```rust
use prodrome::event::{mk_created, mk_spec_revised, TodoId};
use prodrome::fold::{env_at, evaluation_env, flatten};
use prodrome::fpl::{delta_from_hours, fulfillment, instant_of, mk_decay};
use prodrome::literal::Datetime;
use prodrome::policy::Untrusted;
use prodrome::reference::Todo;
use prodrome::registers::nodes_of;
use prodrome::store::EventStore;
use prodrome::view::entries;

// A store is a directory. The first type parameter is the HOST's record shape:
// what this deployment attaches to a todo, as a `payload::Payload`.
// `reference::Todo` is the one this repository's vectors were taken with; a
// host implements its own. The second argument is the deployment's STANDING
// policy (§5) — which events bind and which only claim. `Untrusted` is the
// reference policy: a roster of actor names whose lifecycle events are claims.
let store = EventStore::<Todo>::new(&dir, Untrusted::none());

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
let policy = store.policy();
let events = store.events()?;
let env = env_at(&events, now, policy);
let functions = flatten(&events, now, policy)?;
let todo = TodoId::new("todo-1")?;
let value = fulfillment(&functions[&todo], instant_of(now), &evaluation_env(&env));
assert!((0.0..=1.0).contains(&value));

// Or the whole composition at once: one row per todo the chain mentions,
// each with its outcome, its function, its price and its conflicts (§6.7).
let rows = entries(&nodes_of(&objects), now, policy)?;
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

**Standing.** The database does not decide whom to believe. It asks the host's
`Policy` one question per event — does this BIND, or does it only CLAIM — and
keeps both readings: the confirmed one, under that policy, and the claimed one,
under the policy where everything binds. A claim is stored and shown and never
folded. `Untrusted`, the reference policy, is a set of actor names whose
lifecycle events claim; a host with a different rule writes six lines of its
own.

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
each parse their arguments, call the core, and return JSON. The JSON shape of a
term is this crate's — `wasm/src/json.rs`, since 0.4 — because JSON is
JavaScript's literal grammar and the core has its own. A record crosses as
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

THE VECTORS ARE LITERALS of the crate's own grammar (§2), read by the crate's
own parser against a second closed vocabulary — evidence for "one grammar, one
printer, one parser" written in a second format was a second format to keep in
step. Every stored object, event and term inside one is a string holding its
canonical print, because those bytes are what the vector is evidence of.

`conformance/view/*.py` (§6.7) is the one exception to "taken from the
reference": there is no reference to take it from any more, so it is SEEDED
instead — this crate's own log generator (`core/tests/common/mod.rs`'s
`a_log`), under a fixed seed. It is still FROZEN: `cargo test` only reads it,
and regenerating it is a separate, manual step —
`cargo run --example generate_view_vectors -p prodrome-core` — that `nix
flake check` and CI never run.

`wasm/conformance/` holds the JSON boundary's own evidence: the `to_json` and
`explain` shapes for the same 150 terms, frozen when the codec moved there.

## Repository layout

```
core/         prodrome-core: literal, payload, policy, event, store, fpl, fold, registers, breaks, view
              plus `reference`, the payload the vectors were taken with
cli/          prodrome-cli: the `prodrome` binary — the verbs, over that payload
              and the reference policy, and the only clock in the workspace
wasm/         prodrome-wasm: the core compiled for the browser, built with that
              payload, plus `json`, the term codec the JavaScript side reads
conformance/  the vectors, as literals of the grammar in SPEC §2
roadmap/      this crate's own todos, as a store the binary reads
SPEC.md       the specification
```

## Contributing

Issues and pull requests are welcome. Changes to semantics need a change to
`SPEC.md` in the same pull request, and a law or a vector that pins them.

**The roadmap.** `roadmap/` is appended to the way the code is changed: by a
pull request. There is no allowlist in the store and none in CI, because the
store holds no policy — standing is the host's (§5), and a directory of
objects is not a host. What there is instead is content addressing: every
object is named by the hash of its bytes, nothing is ever rewritten, and a
pull request's diff is exactly the objects it adds. So the objects are read
like code, and merging one is a maintainer's act. A reader who wants to see
what a particular writer's events would say without folding them passes
`--untrusted NAME`, and gets the confirmed reading and the claimed one side by
side; that is the reader's policy, and it is not stored anywhere either.

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
