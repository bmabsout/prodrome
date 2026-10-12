# The Prodrome guide

This guide teaches Prodrome in the order a reader needs it. Part I is the
model: what a store means. Part II is how to read and price todos. Part III
is the Rust API, the files on disk and WebAssembly. [`SPEC.md`](../SPEC.md)
is the contract; where this guide and the SPEC differ, the SPEC is right.
Each section names the SPEC section it follows.

## Part I. The model

### 1. A store is a history, and a reading is that history at an instant

A store holds a HISTORY: a set of objects, each an event or a structural
record, linked by the parents each object names. A write adds an object. A
write never changes or removes one. So a history only grows, and two
histories merge by set union.

A READING takes three things and gives one answer:

```
reading : History × Instant × Policy → Belief
```

- The history is the set of objects the store holds.
- The instant `t` is the moment you ask about. Each event carries an `at`,
  the instant its writer stamped on it, and a reading at `t` uses only the
  events with `at ≤ t`.
- The policy says which events bind and which only claim (§4 below).
- The belief is, for each todo: its state (open, completed, cancelled), its
  content, and its spec.

A spec is a term of FPL. It denotes a function of time:

```
⟦spec⟧ : Instant → [0, 1] ∪ {∅}
```

The value is the todo's FULFILLMENT: how well things go if the todo stays as
it is. Its complement is the todo's urgency. `∅` means "no value": a note or
a proposal with no claim on attention.

Everything else in Prodrome is an operation on these two meanings. Three
consequences follow from them, and the SPEC states each one as a law (§9):

- **The past does not change.** An event dated after `t` cannot change the
  reading at `t` (law 2, *Prefix*). So you can ask what the store believed
  last week, and the answer is the one it gave last week.
- **Order is causal; time is data.** The DAG of parents decides which write
  supersedes which. The `at` stamps never decide a merge (§1, law 25).
- **Replicas converge.** A reading depends only on the set of objects, so
  two replicas with the same objects give the same reading (laws 17, 45).

### 2. Objects and the DAG

*SPEC §3.*

A prodrome begins with a `Genesis(label, nonce)`. An event is written as a
`Change(genesis, deps, event)`. Its `deps` are what its writer saw of its
todo: the todo's heads, every write to it that no other write descends from.
So `deps` never leave their todo, and one writer's own writes are ordered by
causality, never by their instants. A write supersedes only the writes to its
own registers that it descends from.

Appending an event that the prodrome already holds writes nothing. A
`Snapshot(genesis, tips, previous)` attests everything written so far.
Stores written before 0.9 hold `Sealed` and `Woven` objects, and they are
read forever.

An object's name is the hash of its canonical print. So a change's name is a
function of its genesis, its event and what its writer held of its todo,
wherever it was written. `verify` reports any object that does not hash to
its name, rests on a missing parent, forms a cycle, or crosses from one
genesis to another.

### 3. Registers and readings

*SPEC §6.*

The DAG is read per `(kind, todo)`. A REGISTER holds its FRONTIER: the writes
to it that no later write descends from. It reads as its CANDIDATES, the
distinct events of the frontier. One candidate is a value. More than one is
a conflict. The caller sees the conflict, and the next write that descends
from it settles it.

Every fold projects the registers at an instant:

- `env` gives each todo's candidate outcomes, and every instant it was
  tended (a grow-only set, merged by union);
- `specs` gives its candidate specs;
- `content` gives its candidate records;
- `flatten` gives its whole history as one function of time. A conflict is
  priced as its most urgent candidate, with `Least`.

### 4. Standing: binds or claims

*SPEC §5.*

The database does not decide whom to believe. It asks the host's `Policy`
one question per event: does this event BIND, or does it only CLAIM? It
keeps both readings. The CONFIRMED reading uses the policy. The CLAIMED
reading uses the policy where every event binds. A claim is stored and shown,
and it is never folded into the confirmed reading.

`Untrusted`, the reference policy, is a set of actor names whose lifecycle
events only claim. `Proven` binds an event only when a signature proves
who wrote it. A host with a different
rule implements the trait.

### 5. Records, payloads and schemas

*SPEC §4, §6.*

A store is opened at a SCHEMA: the type of its events and the registers each
event writes. The todo schema is the reference one. Six of its event kinds
belong to the database, and their fields are its semantics: `Created`;
`Completed`, `Cancelled` and `Reopened`; `Tended`, a pass at a recurring
todo, which leaves its state alone; and `SpecRevised`.

The seventh kind is the host's. A record is `KIND(todo, at, actor, <your
fields>)`. Your `Payload` supplies the name, the fields, their parse and
print, and the two things the folds read out of a record: the spec it
carries, and how many checklist items it has. The stored bytes are frozen
when you ship them, as the database's own are.

## Part II. Pricing work

### 6. FPL terms

*SPEC §7.*

A spec is built from these terms:

| Term | Meaning |
|---|---|
| `Flat` | one value at every instant |
| `Decay` | a value that falls toward a deadline |
| `Conj` | a power mean of its members, so the weakest member dominates |
| `Within` | a term sampled over a window of days ahead |
| `After` | a term anchored to another todo's completion |
| `Recur` | a term anchored to a todo's last tending |
| `Periodic` | a term repeated on the calendar |
| `Piecewise` | a different term from each instant on |
| `Ref` | another todo's fulfillment, for subtodos and groups |
| `Absent` | no value, `∅`; it neither raises nor lowers what it is composed with |

A term that holds a `Ref` is OPEN. `link` binds every reference to that
todo's function, and to `Absent` for a known todo with none. It returns a
`Closed` term. It refuses a todo the store has never seen, and a loop.

Four operations read a closed term:

- `fulfillment` evaluates it at an instant;
- `explain` returns the same computation, with a value at every node;
- `series_knots` gives its curve over a window;
- `observe::next_change` gives the next instant at which the value, as a
  view shows it (a whole percent, for example), changes. A client redraws
  then, not on a ticking clock.

### 7. Pricing your own work

A price answers "how well do things go if this stays as it is?", not "how
important is this?". Low is urgent. The command line tool prices a todo in
one of two ways:

- `--priority N`, a constant from 0 to 100: 10 is critical, 35 to 45 is a
  real consequence within weeks, 70 to 85 is a nice-to-have, and 90 or more
  is parked;
- `--deadline YYYY-MM-DD`, a `Decay` from `--start` (default 55) to `--end`
  (default 5), which begins `--lead-up-days` (default 3) before 17:00 on that
  day.

Some rules of thumb:

- A deadline is a decay toward its date, not a constant low number. The todo
  becomes urgent when it should, and not before.
- A todo that waits on another is a `Ref` to it, so it is exactly as urgent
  as the thing it waits on.
- A group of todos is a `Conj` of `Ref`s: its weakest member dominates the
  group's value.
- A recurring chore is a `Recur`: its term restarts at each `Tended`, so its
  value falls with the time since the last pass.
- `revise` writes a new spec from its instant on. The old spec still prices
  every earlier instant.

The roadmap's [`PRICING.md`](https://github.com/bmabsout/prodrome/blob/roadmap-data/PRICING.md) is one
project's scale, with anchors from 10% to 90%.

### 8. The chain compiler

*SPEC §7.1.*

`After` and `Recur` are the terms that read history. `chain::compile(term,
env)` resolves each of them against a snapshot. It returns a term with the
same reading and no lookup left in it: each link becomes one graded offset,
a chain becomes one schedule, and a recurrence becomes one piece per pass. A
cancelled upstream compiles to the moot value 1.0, and the compiler REPORTS
it beside the term, because nothing in the number says so. The compiler is
an optimisation with an equivalence law (law 13), not a second semantics:
the interpreted path is unchanged, and compiling is opt-in.

### 9. The view

*SPEC §6.7.*

`entries` composes the folds into one row per todo: its outcome, its claim,
its function, its value (a number, `absent`, or why it does not link), its
current content, its conflicts, and the objects that mention it.
`list_order` is the one order a list shows: most urgent first, then every row
with no number.

## Part III. Using the crate

### 10. A host: payload and policy

A host is a program over the core. It supplies two things:

- **A payload**, `payload::Payload`: the constructor name its records are
  stored under, their fields in order, the vocabulary those fields nest, a
  parse and a print, and the two readings the folds take (the spec a record
  carries, and its checklist length). The default `reference` feature ships
  `prodrome::reference::Todo`, the payload that `conformance/` was taken
  with. A host with its own payload can turn the feature off with
  `default-features = false`.
- **A policy**, `policy::Policy`: one function from an event to `Binds` or
  `Claims`, which is the whole of SPEC §5. `policy::Untrusted` is the
  reference policy, and `Untrusted::none()` stands behind every writer.

The `prodrome` command line tool is a host like any other: the reference
payload, the reference policy, and a clock, over the core. It is the only
code in the workspace that reads the machine's clock.

### 11. The Rust quick start

The crate's [quick start](../core/src/lib.rs) is a doctest, so `cargo test`
runs it. It does these steps:

1. Open a store, `EventStore::<TodoEvent<Todo>>::new(&dir,
   Untrusted::none())`, and `init` its genesis.
2. `append` a `Created` and a `SpecRevised` whose spec is a `Decay`. Each
   event carries its own `at`; the store has no clock.
3. Read the DAG back with `store.dag()`. Every object is rehashed on the
   way in.
4. Fold at an instant: `registers::fold` the nodes, then take `env` and
   `flatten` of the one prodrome.
5. `link` the todo's function, and evaluate it with `fulfillment`. A value is
   `Option<f64>`; `None` is `∅`.
6. Or call `view::entries` for the whole composition: one row per todo.

`cargo doc --open` documents every module. The module list in
`core/src/lib.rs` maps each module to its SPEC section.

### 12. The store on disk

*SPEC §3 states every rule in this section.*

```
<root>/
  objects/<sha256>.py     one object, its canonical print
  .gitattributes          written by the store where there is none
  .lock                   held during an append
  quarantine/<sha256>.py  a file set aside by fsck for failing its hash
```

A print is a Python-literal expression from a closed grammar (SPEC §2). The
parser accepts that grammar and nothing else, and it evaluates nothing.

No file names the heads. A store's tips are derived: they are the objects
that no object names as a parent. So a store under git merges by union. Two
clones that each appended only added files, and `git merge` of the two is
the Prodrome's own merge: nothing can conflict. A store from before 0.9 has
a `HEAD`, and `refs/` if it had several heads; `prodrome verify` asks you to
delete them.

A crash leaves at most a temp file, and `prodrome verify` reports it. A file
that does not hash to its name, or does not parse, is left out of every
read with everything that rests on it, and the read says so. Every write
refuses while such a file is there. `prodrome fsck` moves each such file to
`quarantine/`, and the store writes again. `prodrome verify` checks the same
things and changes nothing.

**A store under git needs `objects/** -text -diff` in its
`.gitattributes`.** The first write to a store with no `.gitattributes`
writes one, so commit it. Without it, a checkout that converts line endings
leaves files that no longer hash to their names.

### 13. WebAssembly

```console
$ nix build .#prodrome-wasm
```

This produces `result/web/` for a bundler and `result/nodejs/` for a script.
Each export (`verify_objects`, `fold`, `registers`, `entries`,
`fulfillment`, `explain`, `compile`, `link`, `series_knots`, `next_change`,
`term_json`, `lifecycle`, `seal` and `merge_object`) parses its arguments,
calls the core, and returns JSON.

The JSON shape of a term is defined in `wasm/exports/src/json.rs`. JSON is
JavaScript's literal grammar, and the core has its own. A record crosses as
its payload's own field names. Beside the exports is `Todos`, the same
exports generic over a schema. A host with its own schemas builds one module
of them all, each a class of its own, with `prodrome_wasm_exports::schema!`;
the header of `wasm/exports/src/lib.rs` explains how.

## What this guide does not cover yet

- Worked FPL examples with plotted curves: `Within`, `After`, `Recur` and
  `Periodic`, each with its curve over a month.
- Sync between replicas, the overlay and a `Decision` (design §6.1).
- Signatures and keys (SPEC §3, §5).
- A declared schema, written as text (SPEC §6).
- Nested histories and sets of prodromes (design §6.3).
- The digest, a summary tree over a log (design §6.5).

Until then, [`SPEC.md`](../SPEC.md) and the design notes in this directory
cover these topics.
