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

The roadmap lives on its own branch, [`roadmap-data`](../../tree/roadmap-data),
apart from the code: a store of objects like any other, holding what this
crate has left to do. Check that branch out and `prodrome list` folds it at
whatever instant you ask for. The list is ascending in fulfillment, so the top
of it is the most urgent thing. CI on `main` checks the branch out and runs
`verify` and `list` over it on every change, so the code can never stop
reading its own data.

**Issues are items.** An issue opened here becomes item `gh-<number>` on that
branch, at a neutral 80%, and its page is
`https://bmabsout.github.io/prodrome/#/todo/gh-<number>`. Closing the issue
completes the item and reopening it reopens it. A maintainer prices it with a
comment — `/price 40`, `/price 30 --deadline 2026-10-15`, `/price ref gh-7` —
on the scale in the branch's `PRICING.md`. A pricing bot (Claude) suggests a
price on every new issue; its suggestion is stored as a CLAIM by
`pricing-bot`, shown beside the item and binding nothing until a maintainer
replies `/price accept`. The workflows are `.github/workflows/mirror.yml` and
`price.yml`, over one deterministic, offline binary, `prodrome-github`. It lives in its
own crate, `github/`: an optional integration, like the viewer, and nothing
in `core/` or `cli/` knows GitHub.
[`docs/github-agent.md`](docs/github-agent.md) explains the flow and where
trust stops.

```console
$ git switch roadmap-data
$ prodrome list
$ prodrome list --at 2026-09-09
$ prodrome list --untrusted pricing-bot   # the bot's suggestions as claims
```

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
use prodrome::fold::{env_at, evaluation_env, flatten, link_specs};
use prodrome::fpl::{delta_from_hours, fulfillment, instant_of, link, mk_decay};
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
// A spec may name other todos with `Ref`, so evaluation takes a CLOSED term:
// `link` binds every reference to that todo's own function, and to
// `Absent` for a todo the store knows that has none.
let closed = link(&functions[&todo], &link_specs(&functions, [&todo]))?;
// A value is `Option<f64>`: `None` is `∅`, the reading of `Absent`.
let value = fulfillment(&closed, instant_of(now), &evaluation_env(&env));
assert!(value.is_some_and(|value| (0.0..=1.0).contains(&value)));

// Or the whole composition at once: one row per todo the chain mentions,
// each with its outcome, its function, its price and its conflicts (§6.7).
let rows = entries(&nodes_of(&objects), now, policy)?;
assert_eq!(rows.len(), 1);
assert_eq!(rows[0].state(), "open");
assert_eq!(rows[0].value(), Ok(value));
```

This example is the crate's doctest; `cargo test` compiles and runs it.

## Concepts

**Objects and the DAG.** A prodrome begins with a `Genesis(label, nonce)`.
An event is written as a `Change(genesis, deps, event)`, whose `deps` are
what its writer saw of its todo: the todo's heads, every write to it that
no other descends from, so they never leave its todo and a writer's own
writes are ordered by causality, never by their instants. A write still
supersedes only the writes to its own registers it descends from.
Appending an event the prodrome already holds writes nothing. A
`Snapshot(genesis, tips, previous)` attests everything written so far.
Stores written earlier hold `Sealed` and `Woven` objects, read forever. An
object's name is the hash of its canonical print, so a change's name is its
genesis, its event and what its writer held of its todo, wherever it was
written.
`verify` reports anything that does not hash to its name, rests on a missing
parent, forms a cycle, or crosses from one genesis to another.

**Records and payloads.** Six event kinds are the database's — `Created`,
`Completed`/`Cancelled`/`Reopened`, `Tended` (a pass at a recurring todo,
which leaves its state alone), `SpecRevised` — and their fields are its
semantics. The seventh is yours: a record is `KIND(todo, at, actor, <your
fields>)`, where your `Payload` supplies the name, the fields, their parse and
print, and the only two things the folds read out of one (the spec it carries
and how many checklist items it has). The stored bytes are frozen the moment
you ship them, exactly as the database's own are.

**Registers and folds.** The DAG is read per `(kind, todo)`: a register
holds the frontier of writes nothing later descends from, and reads as its
candidates, the distinct events there. One is a value; more is a conflict,
shown to the caller and settled by the next write that descends from it.
Every fold projects the registers at an instant: `env` gives each todo's
candidate outcomes and every pass it was tended (a grow-only set, merged by
union), `specs` its candidate specs, `content` its candidate records, and
`flatten` its whole history as one function of time, a conflict priced as
its most urgent candidate with `Least`.

**Standing.** The database does not decide whom to believe. It asks the host's
`Policy` one question per event — does this BIND, or does it only CLAIM — and
keeps both readings: the confirmed one, under that policy, and the claimed one,
under the policy where everything binds. A claim is stored and shown and never
folded. `Untrusted`, the reference policy, is a set of actor names whose
lifecycle events claim; a host with a different rule writes six lines of its
own.

**FPL.** Terms such as `Flat`, `Decay`, `Conj` (a power mean, so the weakest
member dominates), `Within` (sampled over a window), `After` (anchored to
another todo's completion), `Recur` (anchored to a todo's last tending),
`Periodic` (repeated on the calendar), `Piecewise`, `Ref` (another todo's
fulfillment, for subtodos and groups) and `Absent` (no value at all, `∅`: a
note or a proposal with no claim on attention, which neither raises nor
lowers what it is composed with). A term holding a `Ref` is open; `link`
binds every reference to that todo's function, `Absent` for a known todo
with none, and answers a `Closed` term, refusing a todo the store has never
seen or a loop as a value. `fulfillment` evaluates a
closed term at an instant; `explain` returns the same computation with a value
at every node; `series_knots` gives a term's curve over a window; and
`observe::next_change` answers when its value, as a view shows it (an
`Observation`, a whole percent say), next changes, so a client redraws then
and not on a ticking clock.

**The chain compiler.** `After` and `Recur` are the terms that read history,
so `chain::compile(term, env)` resolves every one of them against a snapshot
and hands back a term with the same reading and no lookup left in it — each
link as one graded offset, a chain as one schedule, a recurrence as a piece
per pass. A cancelled upstream compiles to
the moot 1.0 and is REPORTED beside it, because nothing in the number says so.
An optimisation with an equivalence law (SPEC §9.13), not a second semantics:
the interpreted path is unchanged and compiling is opt-in.

**View.** `entries` composes the above into one row per todo: outcome, claim,
function, value (a number, `absent`, or why it does not link), current
content, conflicts, and the objects that mention it. `list_order` is the one
order a list shows them in: most urgent first, then every row with no number.

The precise semantics are in [`SPEC.md`](SPEC.md).

## Storage format

```
<root>/
  objects/<sha256>.py     one object, its canonical print
  .gitattributes          written by the store where there is none
  .lock                   held during an append
  quarantine/<sha256>.py  a file set aside by fsck for failing its hash
```

Objects are append-only and never rewritten. A print is a Python-literal
expression from a closed grammar (SPEC §2); the parser admits that grammar and
nothing else, and nothing is evaluated.

Nothing names the heads. A store's tips are DERIVED: the objects no object
names as a parent. So a store under git merges by union: two clones that each
appended only added files, and `git merge` of the two is the Prodrome's own
merge, with nothing that can conflict, and nothing has to join it: the next
append names only the writes it supersedes. (A store from before 0.9 has a `HEAD`,
and `refs/` if it had several heads; `prodrome verify` asks for them to be
deleted, and the tips it derives are the ones they named.)

An object is synced to disk before it appears under its name, so a crash
leaves at most a temp file, which `prodrome verify` reports with anything
else in `objects/` that is not an object. A file that no longer hashes to its
name makes every read refuse, since nothing can say what it held;
`prodrome fsck` moves every such file to `quarantine/` (never overwriting
or deleting one), the store answers again, reading its history without the
object and without what rests on it, and the report keeps a receipt for it
until the object is restored from a replica. `prodrome verify` checks the
same and changes nothing.

**A store under git needs `objects/** -text -diff` in its `.gitattributes`**,
and writes it: the first write to a store with no `.gitattributes` beside
`objects/` puts one there (`objects/** -text -diff` and `quarantine/** -text
-diff`), and one already there is never rewritten. An object's name is the
hash of its bytes, and a checkout that converts line endings, or a merge
driver that rewrites text, leaves a file that no longer hashes to its name;
the attribute tells git the bytes are not its to touch. A store that has
not been written since it was cloned has one only if it was committed, so
commit it.

## WebAssembly

```console
$ nix build .#prodrome-wasm
```

Produces `result/web/` for a bundler and `result/nodejs/` for a script. The
exports (`verify_objects`, `fold`, `registers`, `entries`, `fulfillment`,
`explain`, `compile`, `link`, `series_knots`, `next_change`, `term_json`,
`lifecycle`, `seal` and `merge_object`) each parse their arguments, call the core, and
return JSON.
The JSON shape of a term is this crate's — `wasm/exports/src/json.rs`, since 0.4 —
because JSON is JavaScript's literal grammar and the core has its own. A
record crosses as its payload's own field names. Beside them is `Todos`, the
same exports generic over a schema: a host with its own schemas builds one
module of them all, each a class of its own, with
`prodrome_wasm_exports::schema!`, as `wasm/exports/src/lib.rs`'s header
describes.

## The viewer: the Prodrome in the browser

<https://bmabsout.github.io/prodrome/> shows this crate's roadmap, read-only.
It is a static site: the page fetches the objects of the `roadmap-data`
branch as they are on disk, folds and prices them with the core compiled to
WebAssembly, and typesets the result with Typst compiled to WebAssembly.
Nothing is computed on a server. `#/` is the list, most urgent first, and
`#/todo/<id>` one item: its body, price, explanation and history. A list row
draws its price as a small pie, and an item its thirty days, fifteen back and
fifteen ahead, as one line; both are coloured on one continuous scale, red at
0 through orange and yellow to green at 1. The item page also has a
small Typst editor with highlighting and completion, as a demo; it saves
nothing.

**It is an example, and it is optional.** `core/` and `cli/` never depend on
Typst, and nothing in them assumes a store's text is Typst. Typst lives in
three directories of its own, for a host whose items hold Typst:

```
typst-wasm/   Typst 0.15.1 for the browser: HTML export, highlighting, completion,
              and a world kept between compilations for a host's views
              (its own cargo workspace and lock file, outside the core's)
typst/        prodrome-typst, a Typst package: the list, an item, the two marks
viewer/       the static web app, in TypeScript, built by nix with tsc and esbuild
```

The command line's `list` and `show` stay plain text: making them typeset
would make `cli/` depend on Typst. A reader who wants a PDF compiles the
package's layouts with the `typst` binary (see `typst/README.md`).

To run it locally against a checkout of `roadmap-data`:

```console
$ nix build .#prodrome-viewer -o result-viewer
$ git worktree add ../roadmap-data roadmap-data
$ sh viewer/assemble.sh result-viewer ../roadmap-data/roadmap site
$ python3 -m http.server -d site 8000     # then open http://localhost:8000/
```

`assemble.sh` works on any store, so the same three commands show your own.
`node viewer/test/smoke.mjs site <todo>` drives the list and one item in
headless Chromium, if Playwright is installed. The Pages workflow
(`.github/workflows/pages.yml`) runs the same build and assembly on every
push to `main` and after every verified push to `roadmap-data`.

## Documentation

- [`SPEC.md`](SPEC.md): the contract, with the laws in §9.
- [`CHANGELOG.md`](CHANGELOG.md): what changed, and what the stored bytes promise.
- [`docs/github-agent.md`](docs/github-agent.md): the issue mirror and the
  pricing bot, and their trust boundaries.
- `cargo doc --open`: the API.

## Testing

```console
$ nix flake check              # fmt, tests and clippy, in the sandbox
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

`conformance/absent/*.py` (§9.18) is seeded the same way, for `Absent`:
terms, exact series and logs drawn from the generators the laws of `∅` run
over (`core/tests/common/terms.rs`, and `a_log_with_absence`), frozen, and
regenerated only by hand with
`cargo run --example generate_absent_vectors -p prodrome-core`.

`conformance/link.py` (§7.2) is the other exception: it is written BY HAND
from `link`'s laws — a few specs, the prints they link to, their values at a
few instants and one loop refused — because a law's own consequences are the
evidence there, and every number in it is arithmetic a reader can redo.
`conformance/recur.py` (§7.3) is written the same way: a toothbrush tended
twice and claimed once, a rent that repeats, and their readings.

`wasm/conformance/` holds the JSON boundary's own evidence: the `to_json` and
`explain` shapes for the same 150 terms, frozen when the codec moved there.

## Repository layout

```
core/         prodrome-core: literal, payload, policy, event, store, fpl, chain, fold, registers, breaks, view
              plus `reference`, the payload the vectors were taken with
cli/          prodrome-cli: the `prodrome` binary — the verbs, over that payload
              and the reference policy, and the only clock in the workspace
wasm/         prodrome-wasm: the core compiled for the browser, built with that
              payload; `wasm/exports/` holds its exports generic over a schema
              and `json`, the term codec the JavaScript side reads
conformance/  the vectors, as literals of the grammar in SPEC §2
docs/         how the repository runs its own roadmap: the issue mirror
.github/      CI, and the two workflows that mirror and price issues
SPEC.md       the specification

              OPTIONAL EXTRAS; nothing above depends on them:
github/       prodrome-github, the binary that mirrors GitHub issues into a store
typst-wasm/   Typst compiled for the browser, its own workspace
typst/        prodrome-typst, the Typst package of layouts
viewer/       the static web app published to GitHub Pages
```

**The core stays free of any markup language.** `core/` and `cli/` store a
record's text as opaque strings and never read it as Typst, Markdown or
anything else; the issue mirror writes titles as plain text. A viewer that
renders a store whose items hold Typst is an optional extra built on top of
the core, and an example of using it, not a part of it.

## Contributing

Issues and pull requests are welcome. Changes to semantics need a change to
`SPEC.md` in the same pull request, and a law or a vector that pins them.

**The roadmap.** The `roadmap-data` branch is appended to the way the code is
changed: by a pull request, against that branch. The store holds no policy,
because standing is the host's (§5), and a directory of objects is not a host.
What there is instead is content addressing: every object is named by the hash
of its bytes, nothing is ever rewritten, and a pull request's diff is exactly
the objects it adds. So the objects are read like code, and merging one is a
maintainer's act. A reader who wants to see what a particular writer's events
would say without folding them passes `--untrusted NAME`, and gets the
confirmed reading and the claimed one side by side.

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
