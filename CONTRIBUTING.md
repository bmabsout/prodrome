# Contributing to Prodrome

Thank you for your interest in Prodrome. Issues and pull requests are
welcome. This page explains how to set up the project, how to test a
change, how maintainers review it, and the rules for code. [`docs/development.md`](docs/development.md) gives more detail
about the tests, the CI, and the layout of the repository.

## Set up

The project uses a nix flake. The flake gives the Rust toolchain and all
other tools that the CI uses:

```console
$ git clone https://github.com/bmabsout/prodrome
$ cd prodrome
$ nix develop
```

If you do not use nix, install Rust 1.85 or later with rustup. You can then
build and test the Rust crates with cargo. You need nix to build the wasm
package, the viewer, and the Typst package.

## Test your change

Run these checks before you push. The CI runs the same checks, so a change
that passes them locally usually passes in the CI:

```console
$ nix develop -c cargo fmt --all --check
$ nix develop -c cargo clippy --locked --workspace --all-targets -- -D warnings
$ PRODROME_MAX_CASES=32 nix develop -c cargo test --locked --workspace
```

The variable `PRODROME_MAX_CASES=32` limits each property test to 32
generated cases, so the tests finish in about a minute. Before you ask for
a review, run the full gate. It runs every test at its full number of cases
and builds every package:

```console
$ nix flake check
```

## Open a pull request

Put one change in each pull request. A small pull request is easier to
review, and if it breaks something, it is easier to find and revert. If
your change needs a preparatory refactor, open the refactor as its own pull
request first.

Describe the change in two or three plain sentences, as you would explain
it to a colleague. Add one line that says how you tested it. The template
`.github/pull_request_template.md` gives this form. The commits give the
details.

## A change to semantics needs the SPEC and a test

[`SPEC.md`](SPEC.md) is the specification, and this crate is its reference
implementation. Other implementations depend on the SPEC, so the SPEC must
always describe what the code does. If your change alters what a program
can observe, such as a value, a stored byte, or a result of `verify`, do
these things in the same pull request:

1. Change `SPEC.md` to describe the new behaviour.
2. Add a law in SPEC section 9 and a property test that checks it, or add a
   vector in `conformance/`. The test shows that the code follows the new
   text.
3. Add an entry to [`CHANGELOG.md`](CHANGELOG.md). If the change affects the
   stored format, say so.

Do not change the fields of a stored constructor that a release contains.
Stores that exist today contain those bytes, and the name of each object is
the hash of its bytes. To store new data, add a new constructor.

## How maintainers review and merge

The CI must pass on your pull request: the `cargo (lint)`, `cargo (test)`
and `nix` checks. A maintainer then reads the change. The maintainer checks
that the change does one thing, that the SPEC and the tests match the code,
and that the code follows the style below. When the maintainer agrees with
the change, the maintainer merges it with a merge commit.

## Roadmap items arrive by pull request

The roadmap is a Prodrome store on the branch `roadmap-data`.
[`docs/roadmap.md`](docs/roadmap.md) explains it. To add or change an item,
write the objects with the `prodrome` CLI, and open a pull request against
`roadmap-data`. Do not edit or delete an existing object. Each object is
named by the hash of its bytes, so the diff of the pull request contains
exactly the objects that it adds, and a reviewer can read each one. If you
mark an item as done, name the evidence in the note: a merged pull request,
or the code that does the work.

## Code style

These rules keep the code correct by construction, so that the compiler
finds mistakes before a test or a user does.

- **Make invalid states unrepresentable.** Use an enum for a value that has
  several states, not a `bool` or an `Option`. Then no code can build a
  state that has no meaning.
- **Parse input once, at the edge.** Check input when it enters the program,
  and return a type that records the check, such as `TodoId`. Then later
  code does not need to check again.
- **Write total functions.** Avoid `unwrap`, `expect` and `panic!`. If a
  state is impossible, change the types so that it cannot occur.
- **Match exhaustively.** Avoid a `_ =>` arm in a match on our own enums.
  Then a new variant makes the compiler show every place that must handle
  it.
- **Use data for closed sets of behaviour.** Write a closed set of
  behaviours as an enum that one function interprets, as `Term` does. Data
  is easier to test, print and compare than a closure.
- **Keep the core pure.** The core does not read the clock. Only the
  command-line tool reads it. A pure function gives the same answer for the
  same input, so a test can check it.
- **Write one combinator with laws.** Do not copy a function for each type.
  Make it generic, and state its laws.
- **Test each law.** State each law in SPEC section 9, and name it in the
  doc comment of the test that checks it.
- **Put one concept in each file,** and name the file for the concept.
- **Do not add checks for states that the types exclude,** and do not add an
  abstraction that no code uses.
- **Write comments about the code, not its history.** The commit message
  records the history.

Some existing code does not follow every rule yet. The maintainers fix
these places over time. Do not add new ones.

## This repository is public

Everything that you write here is public. In examples, tests and roadmap
items, use only neutral names and data, such as the actor `ada` and the
todo "write the report". Do not write about a real person, account,
deployment, place or private project.

## License

Unless you explicitly state otherwise, any contribution that you
intentionally submit for inclusion in the work, as defined in the
Apache-2.0 license, is dual licensed under MIT or Apache-2.0, without any
additional terms or conditions. The License section of the README gives the
full text.
