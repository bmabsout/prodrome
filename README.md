# prodrome

**A temporal, content-addressed event database with fulfillment-priority
semantics. Plain files, amenable to a git backing, no git dependency.**

A store is a directory of *objects*: one file each, holding one expression of
a small printable grammar, named by the SHA-256 of exactly those bytes. Each
object names its parents, so the history is a DAG, and every read re-hashes
the bytes before parsing them. Order is causal: an object comes after what it
descends from and is otherwise incomparable. The `at` a writer stamps on an
event is data for the folds, never an input to the merge. No clock decides who
wins.

A *fold* turns the events up to a moment into what is believed at that moment:
which todos are resolved, which specification is in force, which record is
current. *Registers* read the same DAG and answer what a fold cannot: where
two replicas wrote concurrently, a register holds both writes and the reader is
shown a conflict rather than a quietly chosen winner. The two readings agree
wherever there is nothing to disagree about; that is a law, not a convention.
A trust policy sits over both: an untrusted actor's claim that a todo is done
is stored and shown, and never folded.

FPL is the third layer: a small fuzzy temporal logic in which a todo's spec is
a function from time into `[0, 1]`. A deadline decays, a conjunction is as
good as its weakest member, a window is sampled over the days ahead, a term can
anchor to another todo's completion. Evaluating a spec at an instant against
the fold's environment gives a *fulfillment*; its complement is urgency. There
is one evaluator, and `explain` decorates its computation with a value at every
node instead of re-reading the semantics. Replicas exchange objects, not
deltas: `adopt` re-verifies another store's bytes and either fast-forwards or
grows a second head that a later merge settles.

## Quick start

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

This block is the crate's documentation, so `cargo test` compiles and runs it.

| module      | what a caller reaches for |
| ----------- | ------------------------- |
| `store`     | `EventStore::new`, `append`, `merge`, `adopt` / `adopt_objects`, `load`, `read_dag` / `read_dag_named` / `read_dag_at`, `read_chain`, `events`, `tip` / `tips`, `ancestors`, `concurrent`, `verify`, `linearise` |
| `event`     | `mk_created`, `mk_completed`, `mk_cancelled`, `mk_reopened`, `mk_spec_revised`, `mk_authored`; `mk_sealed` / `mk_woven`; `parse_envelope`, `canonical_envelope`, `seal_hash`, `parents_of`, `binds`; `Hash`, `TodoId`, `Actor` |
| `literal`   | `Datetime::new`, `parse_literal`, `print_literal`, `Open` / `Table`, `ProdromeError` |
| `fold`      | `env_at`, `specs_at`, `authored_at`, `flatten`, `history`, `evaluation_env`, `chronological`, `Untrusted` |
| `registers` | `nodes_of`, `fold`, `extend`, `since`, `env_of` / `specs_of` / `content_of` / `chosen_of`, `conflicts_of` |
| `fpl`       | the `mk_*` constructors, `checklist`; `fulfillment`, `explained` / `explain`, `normalize`, `print_term` / `parse_term`, `to_json` / `from_json`, `instant_of` / `datetime_of` |
| `breaks`    | `breakpoints`, `series_knots`, `constant` |
| `view`      | `entries` and `Entry` (§6.7) |

## The store on disk

```
<root>/
  objects/<sha256>.py     one object, holding exactly its canonical print
  HEAD                    the single tip
  refs/<sha256>           one file per head, only while there is more than one
  .lock                   held by an append; not data
```

Nothing is encoded, compressed or packed. A store is greppable and diffable,
and a file's name is a checksum of its contents, so corruption is detectable
without a second copy. Objects are append-only and never rewritten, which is
why a git backing works as a backup habit: every commit is a fast-forward of
files that only ever appeared. Nothing in this crate shells out to git or needs
it installed. Content addressing is the immutability; replica sync is the
distribution.

The `.py` extension is honest: an object's print is a Python-literal expression
from a closed grammar (SPEC §2), readable by a person or by `python3 -c`.
Nothing evaluates it. The parser admits that grammar and nothing else: no
names, operators, comprehensions or attribute access.

## The browser runs the same evaluator

```console
$ nix build .#prodrome-wasm
```

`wasm/` is `core/` compiled to WebAssembly. Each export parses its arguments,
calls one thing in the core, and prints the answer: no arithmetic, no policy,
no clock, because every moment is an argument. The build is pure and yields two
glues over one `.wasm`, `$out/web/` for a bundler and `$out/nodejs/` for a
script.

| export           | asks |
| ---------------- | ---- |
| `verify_objects` | §3: do these bytes hash to these names and form one DAG under these heads |
| `fold`           | §6.1–6.5: what the chain believes at an instant |
| `registers`      | §6.6: which registers hold more than one live write |
| `entries`        | §6.7: every todo as the folds see it |
| `fulfillment`, `explain` | §7: what a term is worth now, and what that number is made of |
| `series_knots`   | §7: a term's curve over a window, as knots |
| `term_json`      | a stored term's print as the JSON shape |
| `lifecycle`, `seal`, `merge_object` | §3–§4 from a replica: build an event, seal it onto a parent, join heads |

## Conformance vectors

`conformance/*.json` are frozen. An earlier, independent implementation
produced them while it was still an independent reading of `SPEC.md`; there is
no generator any more, on purpose. A diff in one is not a vector to refresh, it
is this crate disagreeing with the last independent reading, and the question
is which of the two is wrong. New vectors may be added by hand.

| file          | pins |
| ------------- | ---- |
| `dag.json`    | 40 DAGs: linearisation, tips, parents, `verify` findings |
| `folds.json`  | 120 logs: environment and history by instant; specs, content and `flatten` by print |
| `fpl.json`    | 150 terms: prints, JSON, 900 fulfillment samples, `normalize`, `explain` |
| `series.json` | 60 windows, 1429 knots: instants and exactness |

## Laws

SPEC §9, each a test rather than a paragraph:

1. `print ∘ parse` is the identity on every object, and `hash(print)` is its name.
2. Prefix: a later event changes no earlier moment's reading.
3. Independent events commute; a uniform shift of every `at` changes no winner.
4. `history.at(t) == env_at(t)`.
5. `mk_piecewise` is a normal form and `normalize` preserves every reading.
6. Registers equal the folds on any DAG; conflicts are exactly what both branches wrote; a merge settles nothing; a descending write settles.
7. Interpolation between series knots equals evaluation on the exact fragment.
8. Every vector, to the tolerance it was taken at.
9. The view is the composition it names: every field of every entry equals the fold §6.7 names it by.

Laws 2, 3, 4, 6 and 9 are properties over generated logs and generated
two-replica DAGs, built as real stores in temporary directories and driven the
way a second replica would be.

```console
$ nix flake check              # the suites, plus clippy at -D warnings
$ nix develop -c cargo test    # the same, with a toolchain in hand
$ nix build .#prodrome-wasm    # the browser's build
```

## Layout

```
core/         prodrome-core: literal (§2), event (§4), store (§3), fpl (§7),
              fold (§6.1–6.5), registers (§6.6), breaks (§7 knots), view (§6.7).
              No I/O beyond a store's own directory, and no clock.
wasm/         prodrome-wasm: the core compiled for the browser, nothing else.
conformance/  the frozen vectors.
SPEC.md       the contract; this crate is its implementation.
```

Dependencies are few and each is named in `core/Cargo.toml` with its reason:
`chrono`, `sha2`, `serde` / `serde_json`, `thiserror`,
`unicode-general-category`, and `rustix` on unix for the append lock.

## Licence

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 licence, shall
be dual licensed as above, without any additional terms or conditions.
