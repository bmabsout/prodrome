# Prodrome

**A mergeable store of events.**

[![CI](https://github.com/bmabsout/prodrome/actions/workflows/ci.yml/badge.svg)](https://github.com/bmabsout/prodrome/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Prodrome stores events, and your schema says what they are.

- **It's a CRDT,** so any two copies merge, in any order, into the same state.
  Edit copies offline on different machines and sync them later, with no
  server.
- **It's immutable,** so you can read the state at any past moment, check a
  store for damage, and cache results for good.
- **It runs anywhere:** on disk, in memory, or in the browser through
  WebAssembly, and all of them sync with each other.
- **Its laws guarantee every merge,** so distributed syncs always work. The
  laws are in [`SPEC.md`](SPEC.md) §9, and the tests check them.

Use it for anything made of events: an app's settings or messages, a list of
tasks, a record of decisions. A schema can also give each item a value that
changes over time, with FPL, a small temporal logic.

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
