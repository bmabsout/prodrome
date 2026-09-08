# prodrome

**A temporal, content-addressed event database with fulfillment-priority
semantics: a local database of plain files, amenable to a git backing, with no
git dependency.**

Everything a Prodrome store holds is an *object*: one file, holding one
expression of a tiny printable grammar, named by the SHA-256 of exactly those
bytes. An object names its parents, so a store's history is a DAG rather than a
log, and a reader re-hashes the bytes before it parses them — tamper-evidence
on read rather than a promise about who had write access. Order is CAUSAL: an
object comes after the objects it descends from and is otherwise incomparable,
and the `at` a writer stamped on an event is *data*, read by the folds and
never by the merge. No clock decides who wins, because a clock that decides is
a clock every replica has to agree with.

What you get out of a store is a *fold*: the events in causal order, up to a
moment you name, turned into what is believed at that moment — which todos are
resolved and when, which specification is in force, which record is current.
The same DAG read as *registers* answers the question a fold cannot: where two
replicas wrote concurrently and nothing since descends from both, the register
holds a frontier rather than a value, and a conflict is something the reader is
SHOWN instead of something a merge rule quietly resolved. Both readings agree
wherever there is nothing to disagree about — that is a law, not a convention
(SPEC §9.6) — and a trust rule sits over them: an actor the deployment does not
trust can *claim* that a todo is finished, and the claim is stored, shown and
never folded.

The third layer is FPL, a small fuzzy temporal logic in which a todo's spec is
a function of time into `[0, 1]` — a deadline that decays, a conjunction that
is only as good as its weakest member (a power mean, so a low member dominates
without a hard `min`), a window sampled over the days ahead, a term anchored to
another todo's completion. Evaluating it at an instant, against the environment
the fold produced, is what makes a *fulfillment*: a number that says how well a
demand is being met right now, and whose complement is urgency. It is one
evaluator — every consumer reads numbers this crate produced — and a decorated
evaluation (`explain`) carries the same shape with a value at every node, so an
explanation is an annotation of the computation rather than a second reading of
it. Replicas exchange objects, not deltas: `adopt` takes another store's bytes,
re-verifies them, and either fast-forwards or grows a second head that a
`Woven` merge settles later.

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

That block is `prodrome-core`'s crate documentation, so `cargo test` runs it:
if a signature here drifts from the crate, the suite says so.

The surface it names, module by module:

| module      | what a caller reaches for |
| ----------- | ------------------------- |
| `store`     | `EventStore::new`, `append`, `merge`, `adopt` / `adopt_objects`, `load`, `read_dag` / `read_dag_named` / `read_dag_at`, `read_chain`, `events`, `tip` / `tips`, `ancestors`, `concurrent`, `verify`, and `linearise` over objects already in memory |
| `event`     | `mk_created`, `mk_completed`, `mk_cancelled`, `mk_reopened`, `mk_spec_revised`, `mk_authored` and the records they hold; `mk_sealed` / `mk_woven`; `parse_envelope`, `canonical_envelope`, `seal_hash`, `parents_of`, `binds`; the identifier types `Hash`, `TodoId`, `Actor` |
| `literal`   | `Datetime::new`, `parse_literal`, `print_literal`, the `Vocabulary` a parse is checked against (`Open`, `Table`), `ProdromeError` |
| `fold`      | `env_at`, `specs_at`, `authored_at`, `flatten`, `history`, `evaluation_env`, `chronological`, `Untrusted` |
| `registers` | `nodes_of`, `fold`, `extend`, `since`, `env_of` / `specs_of` / `content_of` / `chosen_of`, `conflicts_of` |
| `fpl`       | `mk_flat`, `mk_decay`, `mk_curve`, `mk_conj`, `mk_offset`, `mk_gate`, `mk_shift`, `mk_within`, `mk_importance`, `mk_after`, `mk_offset_by`, `mk_piecewise`, `checklist`; `fulfillment`, `explained` / `explain`, `normalize`, `print_term` / `parse_term`, `to_json` / `from_json`, `instant_of` / `datetime_of` |
| `breaks`    | `breakpoints`, `series_knots`, `constant` — the curve as knots, for a plot |
| `view`      | `entries`, and the `Entry` it hands back (§6.7) |

## The store on disk

```
<root>/
  objects/<sha256>.py     one object, holding exactly its canonical print
  HEAD                    the single tip
  refs/<sha256>           one file per head, only while there is more than one
  .lock                   an flock an append holds; not part of the data
```

Nothing is encoded, compressed or packed. A store is greppable, diffable and
readable by any program that can read a text file, and an object's file name is
a checksum of its contents, so corruption is detectable without a second copy.

That shape is friendly to a git backing — objects are append-only and never
rewritten, so a commit is always a fast-forward of files that only ever
appeared — and it needs no git at all: **content addressing is the
immutability, and replica sync is the distribution**. Backing a store with git
is a backup habit somebody may adopt; it is not part of the design, and nothing
in this crate shells out to git or requires it to be installed.

`.py` is the file extension because an object's print is a Python-literal
expression: a closed grammar of `None`/`True`/`False`, integers, floats printed
by CPython's `repr` rules, strings quoted by CPython's rules, `datetime` and
`timedelta`, tuples, and constructor calls with every field in declared keyword
form (SPEC §2). Nothing evaluates it — the parser admits that grammar and
nothing else, with no names, operators, comprehensions or attribute access to
reach through — but the print is a text a human can read, and `python3 -c` can
too.

## The browser runs the same evaluator

```console
$ nix build .#prodrome-wasm
```

`wasm/` is `core/` compiled to WebAssembly: a dozen exports that parse their
arguments, call one thing in the core, and print the answer — no arithmetic, no
policy, and no clock, because every moment is an argument. The build is pure
(dependencies vendored from `Cargo.lock`, nothing reaches the network) and
gives two glues over one `.wasm`: `$out/web/` for a bundler and `$out/nodejs/`
for a script, so the module a browser fetches is the module a node test can
drive.

| export           | asks                                                          |
| ---------------- | ------------------------------------------------------------- |
| `verify_objects` | §3: do these bytes hash to these names, and do they form one DAG under these heads |
| `fold`           | §6.1–6.5: what does the chain believe at an instant            |
| `registers`      | §6.6: which registers have more than one live write            |
| `entries`        | §6.7: every todo as the folds see it — the composition, once   |
| `fulfillment`    | §7: what is this term worth now                                |
| `explain`        | §7: what is that number made of                                |
| `series_knots`   | §7 knots: what is that term's curve over a window              |
| `term_json`      | §2 → §7: a stored term's canonical print, as the JSON shape    |
| `lifecycle`, `seal`, `merge_object` | §3–§4 from a replica: build a lifecycle event, seal it onto a parent, join two heads |

## The conformance vectors

`conformance/*.json` are FROZEN. They are what an independent implementation —
a Python reference, retired in 2026 — produced while it was still an
independent reading of `SPEC.md`: prints and hashes exactly, linearisations and
frontiers exactly, fulfillments to 1e-9. There is no generator any more, and
that is deliberate: once the generator called this crate, a regeneration would
have RE-STATED the vectors rather than re-derived them, which is a comparison
nobody made.

So these files are **evidence, not output**. A diff in one is not a vector to
refresh; it is this crate disagreeing with the last independent reading of the
spec, and the question it asks is which of the two is wrong. Adding a NEW
vector by hand is fine, and says so in its commit message.

| file           | what it pins |
| -------------- | ------------ |
| `dag.json`     | 40 DAGs: each linearisation, tips, parents and `verify` finding |
| `folds.json`   | 120 logs: `env` and `history_at` by kind and instant, `specs`, `content` and `flatten` by canonical print |
| `fpl.json`     | 150 terms: prints and JSON byte-identical, 900 fulfillment samples, every `normalized` print and every `explain` tree |
| `series.json`  | 60 windows, 1429 knots: instants and `exact` identical |

## The laws

`SPEC.md` §9 is the contract's teeth, and each law is a test rather than a
paragraph:

1. `print ∘ parse` is the identity on every stored object, and `hash(print)` is
   its name — `core/tests/literals.rs`, `core/tests/events.rs`.
2. **Prefix**: appending a later event changes no earlier moment's reading.
3. Independent events commute; a uniform shift of every `at` changes no winner.
4. `history.at(t) == env_at(t)`.
5. `mk_piecewise` is a normal form (unit, join, idempotent, no adjacent
   repeats) and `normalize` preserves every reading — `core/tests/fpl_laws.rs`.
6. **Registers equal the folds** on any DAG, conflicts are exactly what both
   branches wrote, a merge settles nothing and a descending write settles.
7. Interpolation between series knots equals evaluation on the exact fragment.
8. Every vector above, to the tolerance it was taken at.
9. **The view is the composition it names**: every field of every entry equals
   the fold §6.7 names it by — `core/tests/fold_laws.rs`.

Laws 2, 3, 4, 6 and 9 are properties over generated logs and generated
two-replica DAGs (`core/tests/fold_laws.rs`), built as REAL stores in temp
directories and driven the way a second replica would: append, adopt, write
concurrently, merge. Nothing about a frontier is allowed to depend on this side
having constructed the graph in memory.

```console
$ nix flake check                 # the suites, plus clippy at -D warnings
$ nix develop -c cargo test       # the same suites, with a toolchain in hand
$ nix build .#prodrome-wasm       # the browser's build
```

## The crates

```
core/    prodrome-core — the database. literal (§2), event (§4), store (§3),
         fpl (§7), fold (§6.1–6.5), registers (§6.6), breaks (§7 knots),
         view (§6.7). No I/O beyond a store's own directory, and no clock.
wasm/    prodrome-wasm — that core compiled for the browser, and nothing else:
         each export parses, calls one thing, and prints the answer.
conformance/  the frozen vectors.
SPEC.md       the contract. It is the document; this crate is its implementation.
```

Dependencies are few and each is named in `core/Cargo.toml` with the reason it
is there: `chrono` for the arithmetic evaluation needs, `sha2` for the names,
`serde`/`serde_json` for the JSON shapes, `thiserror` for typed refusals,
`unicode-general-category` for the printer's `isprintable` tables, and `rustix`
on unix for the append lock.

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
