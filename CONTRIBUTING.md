# Contributing to Prodrome

Issues and pull requests are welcome. This page explains the workflow for a
change. [`AGENTS.md`](AGENTS.md) gives the rules for code style, for people
and for coding agents. [`docs/development.md`](docs/development.md)
explains the tests, the CI, and the layout of the repository.

## Before you push

Run these checks. The CI runs the same checks.

```console
$ nix develop -c cargo fmt --all --check
$ nix develop -c cargo clippy --locked --workspace --all-targets -- -D warnings
$ PRODROME_MAX_CASES=32 nix develop -c cargo test --locked --workspace
```

Before you ask for a review, also run the full gate:

```console
$ nix flake check
```

## One change in each pull request

Put one change in each pull request. If a change needs a preparatory
refactor, open the refactor as its own pull request first. Describe the
change in two or three plain sentences, and add one line that says how you
tested it. The file `.github/pull_request_template.md` gives this form.

## A change to semantics needs the SPEC, and a test

[`SPEC.md`](SPEC.md) is the specification, and this crate is its reference
implementation. If your change alters what a program can observe, such as a
value, a stored byte, or a result of `verify`, do these things in the same
pull request:

1. Change `SPEC.md` to describe the new behaviour.
2. Add a law in SPEC section 9 and a property test that checks it, or add a
   vector in `conformance/`.
3. Add an entry to [`CHANGELOG.md`](CHANGELOG.md). If the entry changes the
   stored format, say so.

Never change the fields of a stored constructor that a release contains.
To store new data, add a new constructor.

## Roadmap items arrive by pull request

The roadmap is a Prodrome store on the branch `roadmap-data`. To add or
change an item, write the objects with the `prodrome` CLI, and open a pull
request against `roadmap-data`. Do not edit or delete an existing object.
The diff of the pull request must contain only new files in
`roadmap/objects/`. A verdict on an item, such as "done", must name its
evidence: a merged pull request, or the code that does the work.
[`docs/roadmap.md`](docs/roadmap.md) explains the roadmap.

## Write in plain technical English

Write all prose in plain technical English, close to ASD-STE100. This rule
covers documentation, comments, CHANGELOG entries, commit messages, pull
request descriptions, and the text of roadmap items. A new reader must
understand each sentence on the first read.

1. Give each sentence a concrete subject, a plain verb, and an explicit
   object. Say who does what to what.
2. Only people and programs act. Do not write that data, bytes, documents
   or notes "promise", "say", "plan", "name" or "keep" something.
3. Do not use riddles, wordplay or slogans. State the fact plainly.
4. Use common words. Define each term of art in one plain sentence where it
   first appears, or link to its definition. Do not put two terms of art in
   one phrase.
5. Put one idea in each sentence. Write instructions in the imperative.
   Put a condition first.
6. Keep the words "the" and "a". Do not drop words to save space.
7. Use the active voice and the simple present tense.
8. Write descriptions as explanations for a stranger, not as notes to
   yourself.

For example, do not write "the history is free, and a schema is its
reading". Write "Events are stored without a schema, and a program applies
a schema when it reads the events."

## Public repository

This repository is public. In examples, tests and roadmap items, use only
neutral names and data, such as the actor `ada` and the todo "write the
report". Do not write about a real person, account, deployment, place or
private project.

## License

Unless you explicitly state otherwise, any contribution that you
intentionally submit for inclusion in the work, as defined in the
Apache-2.0 license, is dual licensed under MIT or Apache-2.0, without any
additional terms or conditions. The License section of the README gives the
full text.
