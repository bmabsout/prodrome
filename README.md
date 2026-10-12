# prodrome

A temporal, content-addressed event database with fulfillment-priority
semantics.

[![CI](https://github.com/bmabsout/prodrome/actions/workflows/ci.yml/badge.svg)](https://github.com/bmabsout/prodrome/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

A Prodrome store is a history of events about todos. The history never
changes: a write only adds an object. You read the store at an instant, and
the reading is what the history believed then: which todos were open, and how
well each one was fulfilled. Each todo carries a spec, a term of FPL, a small
fuzzy temporal logic. A spec is a function of time with values in `[0, 1]`,
and a low value means the todo is urgent. So a list sorted by value puts the
most urgent todo first.

- **Fulfillment is a function of time.** A deadline decays. A conjunction is
  as good as its weakest member. A term can wait on another todo.
- **Readings at any time.** Ask for the open todos at an instant `t`, and you
  get what the store believed at `t`.
- **A CRDT.** Every store is a full replica. Write offline, exchange objects
  with any other store, and the stores converge without a shared clock. Two
  concurrent writes to one field stay visible as a conflict, for a person to
  settle.
- **Content-addressed.** Each object is one file, named by the SHA-256 of its
  bytes. You can verify a store from its files alone.
- **Standing belongs to the host.** The database asks your policy whether an
  event binds or only claims, and it keeps both readings.
- **Runs in the browser.** The same core compiles to WebAssembly.

## Installation

The crates are not on crates.io yet. Depend on the library by git revision:

```toml
[dependencies]
prodrome = { package = "prodrome-core", git = "https://github.com/bmabsout/prodrome", rev = "<commit>" }
```

The library requires Rust 1.85 or later. The command line tool, `prodrome`, is
the crate `prodrome-cli`:

```console
$ cargo install --git https://github.com/bmabsout/prodrome prodrome-cli
$ nix build github:bmabsout/prodrome#prodrome-cli    # or with nix
```

## Example

A store is a directory. This example makes one, adds two todos, and reads the
store at three instants. Each write prints the names of the objects it
adds; the listing below leaves them out.

```console
$ export PRODROME_ACTOR=ada
$ prodrome init work && cd work
$ prodrome add write-the-report --body "write the report" \
    --deadline 2026-09-15 --at 2026-09-08T09:00:00
$ prodrome add water-the-plants --body "water the plants" \
    --priority 80 --at 2026-09-08T09:00:00
$ prodrome list --at 2026-09-14
2 open at 2026-09-14T00:00:00 — standing behind every writer
0.335  write-the-report  write the report
0.800  water-the-plants  water the plants
$ prodrome done water-the-plants --at 2026-09-14T10:00:00
$ prodrome list --at 2026-09-14T12:00:00
1 open at 2026-09-14T12:00:00 — standing behind every writer
0.251  write-the-report  write the report
$ prodrome list --at 2026-09-13       # the past does not change
2 open at 2026-09-13T00:00:00 — standing behind every writer
0.501  write-the-report  write the report
0.800  water-the-plants  water the plants
$ prodrome verify
ok: 6 objects, 2 heads
```

The report's value falls as its deadline comes near, so it moves to the top
of the list. The same example in Rust is the crate's
[quick start](core/src/lib.rs), which `cargo test` runs.

## Where to go next

- [The guide](docs/guide.md): the model first, then the Rust API, the store
  on disk and WebAssembly.
- [`SPEC.md`](SPEC.md): the contract. The laws are in §9.
- [`CHANGELOG.md`](CHANGELOG.md): what changed, and what the stored bytes
  promise.
- [The documentation index](docs/README.md): design notes, development and
  CI, and the issue mirror.
- [The roadmap](docs/roadmap.md): this crate's own todos, as a Prodrome
  store on the [`roadmap-data`](https://github.com/bmabsout/prodrome/tree/roadmap-data) branch.
- [The viewer](docs/viewer.md): the roadmap in the browser, on
  [GitHub Pages](https://bmabsout.github.io/prodrome/).
- `cargo doc --open`: the API.

## Contributing

Issues and pull requests are welcome. A change to semantics needs a change to
`SPEC.md` in the same pull request, and a law or a vector that pins it.
[`docs/development.md`](docs/development.md) explains the tests, the CI and
the repository layout.

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
