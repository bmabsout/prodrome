# Changelog

All notable changes to this project are documented here. The format is
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html) for its
API. THE STORED BYTES ARE A SEPARATE PROMISE and a stricter one: a shipped
constructor's fields never change, in any release.

## [Unreleased]

## [0.5.0] — 2026-09-10

A VERSION AND NOT AN `Unreleased`: a new crate ships in the workspace and
`packages.default` changes, which are things a reader can depend on, and both
belong under a number rather than under a heading that never settles. Nothing
about the format moved.

### Added

- **`prodrome-cli`**, the `prodrome` binary, over `prodrome-core` with the
  reference payload (`reference::Todo`) and the reference policy
  (`policy::Untrusted`). The verbs: `init`, `add`, `done`, `cancel`, `reopen`,
  `revise`, `list`, `show`, `verify`, `weave`. `--store DIR` defaults to
  `./roadmap` when that directory exists and to `.` otherwise; `--actor`
  defaults to `$PRODROME_ACTOR`, then `$USER`; `--untrusted NAME,…` (or
  `$PRODROME_UNTRUSTED`) is the READER's standing policy, so one invocation
  shows the confirmed reading and the claimed one at once.
- IT IS A HOST, AND IT IS WHERE THE CLOCK IS. The core has none — an event's
  `at` is data (§4) and a fold is a query at an instant the caller names (§6) —
  so `--at` names one and the machine's clock is only what a caller who did not
  falls back to. On a write `--at` is the instant STAMPED ON the event, which
  is how a todo that became true yesterday says so.
- A price is stated as `--priority N`, a constant, or as `--deadline
  YYYY-MM-DD` with `--start`, `--end` and `--lead-up-days`, a `Decay` onto
  17:00 that day. The scale is fulfillment as a percentage, so LOW IS URGENT
  and there is no second scale to invert.
- `weave` is `EventStore::merge(None, None)`: every head into one `Woven`
  carrying no event, which is what settles a store two branches both appended
  to. No actor and no note, because a merge asserts structure and not a fact
  about a todo (§3).
- **`roadmap/`**: this crate's own todos, as a store of objects, seeded by the
  verbs above and committed like any other directory. The historical items
  carry the instants their releases were actually cut, so `prodrome list --at`
  answers about the past with what was believed then. It is the one example
  that cannot go stale: CI runs `prodrome verify --store roadmap` and
  `prodrome list --store roadmap`, so a pull request that appends to it proves
  its objects rehash and that the store still folds.
- THE STORE HOLDS NO POLICY and CI names no allowlist — 0.3's point, applied to
  this repository. Objects are content-addressed and never rewritten, so a pull
  request's diff is exactly the objects it adds, the objects are reviewed like
  code, and merging one is a maintainer's act. README's *Contributing* says it
  in those words.
- `nix build .#prodrome-cli` builds and tests the binary, and it is a
  `nix flake check` check. `packages.default` is now the binary rather than the
  wasm; `nix build .#prodrome-wasm` is unchanged. `roadmap/` is deliberately
  NOT in the flake's fileset: it is data the binary reads, not source it is
  built from, so appending a todo rebuilds nothing.

### Unchanged

- **The stored bytes.** No constructor, field or print changed; every object
  written by 0.1.0 through 0.4.0 parses, prints back byte for byte and hashes
  to the same name. `roadmap/` is written by the same printer that wrote the
  vectors.
- **The wasm wire**, and every conformance vector.
- `prodrome-core`'s API. The binary is a consumer of it and added nothing to
  it: `EventStore`, the folds, `view::entries` and `policy::Untrusted` are what
  the verbs are made of.

## [0.4.0] — 2026-09-10

### Changed

- **The vectors are literals** (SPEC §2). `conformance/` held 1.9 MB of JSON
  in a crate whose claim is that there is ONE reading of its data: one closed
  grammar, one printer, one parser. Evidence written in a second format was a
  second format to keep in step. `conformance/*.py` is that evidence in the
  crate's own grammar, read by the crate's own parser against the VECTOR
  VOCABULARY — a second closed whitelist, disjoint from a store's, so a vector
  name is refused in an object and an object's kind is refused in a vector.
  Every stored object, event and term inside one is a STRING holding its
  canonical print, because those exact bytes are what a vector is evidence of
  (§9.1); instants are `datetime(...)`, and the mappings the JSON keyed by
  todo id or object name are tuples of a named constructor.
- A CHECKED MIGRATION, not a refresh: the vectors are frozen and have no
  generator left to re-run, so the converting commit read the JSON, built the
  literal, printed it, parsed it back, asserted the two values equal, mapped
  the literal back to JSON and asserted it equalled what it started from —
  exactly, no tolerance, every key and every float — for all 400 vectors, and
  ran that under `cargo test`. The counts are unchanged: 120 logs, 40 DAGs,
  150 terms, 60 windows, 30 + 30 view cases.
- **The JSON codec leaves the core.** `fpl::to_json`, `fpl::from_json`,
  `fpl::explanation_json` and `fpl::explain` are `prodrome-wasm`'s
  `json` module now — JSON is JavaScript's literal grammar and the wasm crate
  is the JavaScript boundary. The core keeps `fpl::explained`: the decoration
  is §7's, only its JSON was not. SPEC §7 no longer names `to_json` as part of
  the contract.
- `prodrome-core` depends on neither `serde` nor `serde_json`, as a dependency
  or a dev-dependency. Nothing in the crate parses a second format.
- `core/examples/generate_view_vectors.rs` emits literals, laid out one case
  per line so a diff is readable.

### Added

- `wasm/conformance/term-json.json`: the `json` and `explain` halves of what
  was `conformance/fpl.json`, unchanged, for the same 150 terms — the JSON
  boundary's evidence, in the crate where the JSON is produced.
  `wasm/src/json.rs`'s test replays all of them, and `wasm/src/wire.rs` gains
  the first test of `json_entry`'s ten keys.
- `core/tests/common/vectors.rs`: the vector vocabulary and the accessors a
  suite reads a case with. `common::entry_value` (an `Entry` as the vector
  grammar's `Row`) and `common::explanation_value` (an explanation's
  DECORATION — kind, value, notes — beside the term's own print) replace the
  copy of `json_entry` the core kept in step with the wasm crate by hand.

### Unchanged

- **The stored bytes.** No constructor, field or print changed; every object
  written by 0.1.0, 0.2.0 and 0.3.0 parses, prints back byte for byte and
  hashes to the same name. The format of the EVIDENCE changed; the format of
  the DATA did not.
- **The wasm wire.** Every exported function's arguments and answers are the
  shapes they were — `term_json`, `explain`, `series_knots`, `entries`,
  `fold`, `registers` included, down to the lowercase kind tags, the ISO
  instants and the spans in hours.
- **Every expected answer**, to the digit: what `conformance/*.py` says is
  what `conformance/*.json` said.


## [0.3.0] — 2026-09-09

### Changed

- **Standing is the host's** (SPEC §5, rewritten). The core carried one
  deployment's trust policy: a set of untrusted actor names, plus a
  kind-dependent rule (their lifecycle events did not bind, their content
  records did), plus a `verify` clock rule keyed on the same set. None of that
  is a fact about a DAG of events — it is the same move 0.2 made for the
  record. The database now asks the HOST, one event at a time, and gets a
  `policy::Standing`: `Binds` (the folds take it) or `Claims` (stored, shown,
  never folded). Nothing in the core reads an actor name to decide anything.
- `policy::Policy<P>` is that question as a trait. One required method,
  `standing(&self, event: &TodoEvent<P>) -> Standing`; one provided method,
  `confirms(&self, event) -> bool` ("is this the host's own word"), whose
  default is `standing(event) == Binds`. Not sealed: a host implements it.
- `fold::Untrusted` is now `policy::Untrusted`, the REFERENCE policy — the
  rule `conformance/` was taken under, in the core rather than behind the
  `reference` feature, because it is a policy over the core's own event kinds
  and needs no payload. `Untrusted::none()`, `::of` and `::actors` are
  unchanged; `Untrusted::binds` is gone (ask `standing`). `policy::Everything`
  is new: the policy under which every event binds, which is what §6.7's
  CLAIMED reading is taken under.
- Every fold takes `&impl Policy<P>` where it took `&Untrusted`:
  `fold::{env_at, specs_at, flatten, history}`, `registers::{extend, fold}`,
  `view::entries`. `EventStore<P>` is now `EventStore<P, Pol = Untrusted>` and
  holds the policy by value, so `EventStore::<Todo>::new(root, policy)` takes
  an `Untrusted` where it took a `BTreeSet<Actor>`; `EventStore::policy()`
  hands it back.
- `event::binds(event, &BTreeSet<Actor>)` is gone. It was §5 hardcoded; the
  rule is `Untrusted`'s `Policy` impl now.
- `view::Standing` is `view::Confidence` and `Entry::standing` is
  `Entry::confidence` — the name went to the event-level sum, which is what
  §5 is about. `Provisional` and `Confidence::{of, is_provisional}` are
  unchanged, and `Provisional::Content` now means "the winning content record
  is one the policy does not `confirm`" rather than "its actor is untrusted".
- §3's `verify` states its clock rule through the policy: an event the policy
  does not `confirm`, dated behind any of its ancestors. Same rule, same
  findings, same wording — the reference policy `confirms` exactly the actors
  off its roster, whatever the kind, which is why it overrides the default.
- SPEC §5 is rewritten as STANDING; §3, §6.1–6.5, §6.7 and §8 quantify over
  the policy.

### Added

- SPEC §9.10–9.12, the laws that make "the host decides" honest, as proptests
  over generated logs in `core/tests/fold_laws.rs`:
  - **9.10** under the policy that binds everything, the claimed and confirmed
    readings are equal — every fold answers alike, no entry carries a claim,
    no entry is provisional (`the_two_readings_agree_under_a_policy_that_binds_everything`);
  - **9.11** a claiming event never moves the confirmed reading — append one
    and every confirmed answer at every moment is the answer it was
    (`a_claiming_event_never_moves_the_confirmed_reading`);
  - **9.12** standing selects events, not positions — folding under a policy
    is folding the sub-log of the events it binds, under any permutation of
    the log (`standing_selects_events_and_not_positions`).
- `conformance/view/*.json` (SPEC §6.7): `view::entries` had no frozen
  vectors — they were scrubbed when this crate went public, taken as they
  were on a private chain. SEEDED instead: random logs from this crate's own
  generator (`core/tests/common/mod.rs`'s `a_log`, the same one
  `core/tests/fold_laws.rs`'s properties draw from), under a fixed seed,
  folded at several instants under two reference policies (rostering
  `"triage"` and, inverted, `"bassel"`). `core/tests/conformance_view.rs`
  replays them; `core/examples/generate_view_vectors.rs` is the only thing
  that regenerates them, run by hand, never by `cargo test` or CI.

### Unchanged

- **The stored bytes.** No constructor, field or print changed; every object
  written by 0.1.0 and 0.2.0 parses, prints back byte for byte and hashes to
  the same name.
- **The wasm wire.** `fold`, `registers` and `entries` still take `untrusted`
  as a JSON array of actor names; `wire::parse_untrusted` turns that array
  into the reference policy, which is the rule the argument always meant.
  Every exported function's arguments and answers are the shapes they were,
  `entries`' `unconfirmed` included.
- **Every conformance vector**, byte for byte. `conformance/*.json`'s
  `untrusted` fields are read through the reference policy; the prints,
  hashes, linearisations, frontiers, fulfillments, entries and `verify`
  findings are the ones the vectors held.

## [0.2.0] — 2026-09-09

### Changed

- **A record's fields are the host's** (SPEC §4). `Authored` used to name one
  deployment's schema — `kind`, `created`, `body`, `spec`, `rationale`,
  `category`, `waiting_on`, `detail`, `source`, `subtodos`, `notes`, `note` —
  inside the database. The record kind is now `KIND(todo, at, actor, <the
  host's fields>)`, where a `payload::Payload` supplies the constructor name,
  the fields and their order, the vocabulary those fields nest, their parse and
  print, and the only two readings the folds take: the spec a record carries
  and its checklist length.
- Every type that carries an event is generic over that payload:
  `Authored<P>`, `TodoEvent<P>`, `Envelope<P>`, `EventStore<P>`,
  `registers::{Node, Write, Frontier, Folded}<P>`, and the folds, registers and
  `view::entries` that read them. A type parameter and never a `dyn`: one store
  is parsed against one closed vocabulary, and that vocabulary is the core's
  names union the payload's.
- `literal::Signature` borrows its field order from the vocabulary that
  answered (`Signature<'a>`), so a vocabulary assembled at runtime can lend
  one. `Table::find` keeps the `'static` a static table has.
- `wasm`: a record crosses the boundary as its payload's own `fields()`, keyed
  by the names it stores them under, each literal mapped to JSON by the obvious
  rules. Two keys inside `content`/`records` changed shape: `created` and
  `source.date` are full ISO instants rather than `%Y-%m-%d`, and a record's
  `spec` arrives in the literal shape rather than `fpl::to_json`'s. `todo`,
  `notes`, and a `kind` tag on nested calls are new.

### Added

- `payload::Payload`, and the field readers a host's `from_fields` needs
  (`string_field`, `datetime_field`, `tuple_or_empty`, `spec_field`, …), which
  are the ones the core's own kinds are parsed with.
- `prodrome::reference`, behind the default `reference` feature: the payload
  `conformance/*.json` was taken with, `Todo`, with `Source`, `SubTodo`,
  `Note`, `NoteSite`, `MarkupSource` and `StringSource`. A worked example of
  the trait, and the fixture the conformance suites drive. Turn it off with
  `default-features = false`.
- SPEC §9.1 now says the round trip explicitly for the generic path, and
  `tests/folds.rs` checks it: every `Authored(...)` print in the vectors parses
  under the reference payload and prints back byte for byte.

### Removed

- From `prodrome::event`: `MarkupSource`, `StringSource`, `Source`, `SubTodo`,
  `Note`, `NoteSite`, `mk_authored`, `mk_source`, `mk_note`, `mk_subtodo`, and
  the record's entry in `EVENT_SIGNATURES`. All of it is in
  `prodrome::reference`, unchanged. `mk_record` is the core's constructor for a
  record now: the three fields it owns, and a payload.

### Unchanged

- The stored bytes. Every object written by 0.1.0 parses, prints back byte for
  byte and hashes to the same name under the reference payload.
- The literal grammar, `Hash`, `TodoId`, `Actor`, `Name`, the lifecycle kinds,
  `SpecRevised`, FPL, the folds' semantics, the registers' semantics, and every
  conformance vector.

## [0.1.0]

- The first public release: the literal grammar (§2), objects and the DAG (§3),
  events (§4), trust (§5), the folds and registers (§6), FPL (§7), and the
  conformance vectors the laws in §9 are checked against.
