# Prodrome

**An event database that keeps every change, answers for any moment, and merges copies without a server.**

[![CI](https://github.com/bmabsout/prodrome/actions/workflows/ci.yml/badge.svg)](https://github.com/bmabsout/prodrome/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

You define the kinds of events your application records: its schema. Prodrome
stores each event as a file and never changes it. To read the data, you pick a
moment, and Prodrome computes the state of everything at that moment from the
events. Copies edited on different machines merge by exchanging files.

A schema can also give each entity a value that changes over time, written in
FPL, a small temporal logic. The reference schema in this repository is a todo
list in which each todo's urgency rises as its deadline comes closer. The
`prodrome` command uses it.

## Example: the todo list

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

## Why Prodrome

- **Any schema.** The store is generic over the events you define. Todos are
  one example, not the limit.
- **Nothing is overwritten.** Every change is kept, so any past moment can be
  read again.
- **Values that change with time, if you want them.** With FPL, a schema
  combines deadlines, priorities and dependencies into one number per entity.
- **Copies merge without a server.** Edit offline on several machines and
  exchange files; every copy then gives the same answers. Conflicting edits
  are kept side by side for a person to settle.
- **Storage you can check.** Each event is a file named by the SHA-256 hash of
  its contents, and `prodrome verify` checks a whole store.
- **You decide whom to trust.** Your program's policy says whose events
  count. The others are kept as suggestions.
- **Runs in the browser.** The same Rust core compiles to WebAssembly.

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
