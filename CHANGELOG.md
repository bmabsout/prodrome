# Changelog

All notable changes to this project are documented here. The format is
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html) for its
API. THE STORED BYTES ARE A SEPARATE PROMISE and a stricter one: a shipped
constructor's fields never change, in any release.

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
