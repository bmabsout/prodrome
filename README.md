# Prodrome

**A mergeable store of events.**

[![CI](https://github.com/bmabsout/prodrome/actions/workflows/ci.yml/badge.svg)](https://github.com/bmabsout/prodrome/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Prodrome is a mergeable store of events. A schema says what the events are, and
Prodrome computes the state from them.

- **It is a CRDT** (a conflict-free replicated data type). Any two copies of a
  store merge, in any order, and end in the same state. So you can edit copies
  offline on different machines and sync them later, with no server to
  coordinate them. If both sides changed the same field, the field keeps both
  values until someone picks one.
- **It is immutable.** No event is ever changed or deleted, and each object is
  indexed by the hash of its contents. So you can read the state at any past
  moment, check a store for damage, and cache a result for as long as you
  like, because the objects it was computed from never change.
- **It runs anywhere.** The same Rust core works on a directory of files, in
  memory, and in the browser through WebAssembly, and the copies sync with
  each other.
- **Its laws make every merge succeed.** [`SPEC.md`](SPEC.md) §9 states the
  laws that every implementation must pass, and the tests check them. They
  guarantee that a merge never fails and that any number of copies converge.

What you keep in it is up to you: an application's settings or messages, a
list of tasks, a record of decisions. A schema can also give each entity a
value that changes over time, written in FPL, a small temporal logic.

## Try it

The `prodrome` command uses the reference schema in this repository, a todo
list in which a todo's urgency rises as its deadline comes closer.

```console
$ export PRODROME_ACTOR=ada
$ prodrome init work && cd work
$ prodrome add write-the-report --body "write the report" --deadline 2026-09-15 --at 2026-09-08T09:00:00
$ prodrome add water-the-plants --body "water the plants" --priority 80 --at 2026-09-08T09:00:00

$ prodrome list --at 2026-09-10      # five days before the deadline
0.800  water-the-plants  water the plants
0.980  write-the-report  write the report

$ prodrome list --at 2026-09-14      # one day before: the report is now the most urgent
0.335  write-the-report  write the report
0.800  water-the-plants  water the plants

$ prodrome done water-the-plants --at 2026-09-14T10:00:00
$ prodrome list --at 2026-09-13      # the past still reads the same
0.501  write-the-report  write the report
0.800  water-the-plants  water the plants
```

Each number is the todo's fulfillment, and lower numbers are more urgent. The
output above shows only the list rows.
The same example in Rust is the [quick start](core/src/lib.rs) in the crate
documentation.

## Also

- **Values that change with time, if you want them.** With FPL, a schema
  combines deadlines, priorities and dependencies into one number per entity.
- **You decide whom to trust.** Your program's policy says whose events
  count. The others are kept as suggestions.

## Install

The crates are not on crates.io yet. Use the library as a git dependency at a
fixed commit (Rust 1.85 or later):

```toml
[dependencies]
prodrome = { package = "prodrome-core", git = "https://github.com/bmabsout/prodrome", rev = "<commit>" }
```

Install the `prodrome` command with cargo or nix:

```console
$ cargo install --git https://github.com/bmabsout/prodrome prodrome-cli
$ nix build github:bmabsout/prodrome#prodrome-cli
```

## Learn more

- [The guide](docs/guide.md): the model first, then the Rust API, the storage
  format and WebAssembly.
- [`SPEC.md`](SPEC.md): the specification. Section 9 lists the laws every
  implementation must pass.
- [The roadmap](docs/roadmap.md): the work that is left. It is itself a
  Prodrome store, and you can [browse it online](https://bmabsout.github.io/prodrome/).
- [`CHANGELOG.md`](CHANGELOG.md), [all documentation](docs/README.md), and
  `cargo doc --open` for the API.

## Contributing

Issues and pull requests are welcome. See [`CONTRIBUTING.md`](CONTRIBUTING.md).

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
