# Documentation

Start with the [guide](guide.md). [`SPEC.md`](../SPEC.md) is the contract.

## For users

- [`guide.md`](guide.md): the model, how to price work, the Rust API, the
  store on disk and WebAssembly.
- [`../SPEC.md`](../SPEC.md): the specification. The laws are in §9.
- [`../CHANGELOG.md`](../CHANGELOG.md): what changed, and what the stored
  bytes promise.

## For contributors

- [`development.md`](development.md): tests, CI, the conformance vectors and
  the repository layout.
- [`roadmap.md`](roadmap.md): the roadmap branch, issues as items, and how to
  add to it.
- [`viewer.md`](viewer.md): the browser viewer and GitHub Pages.
- [`github-agent.md`](github-agent.md): the issue mirror and the pricing
  bot, and their trust boundaries.

## Design notes

Each note proposed a change before it was made. The SPEC and the CHANGELOG
say what landed.

- [`design-empty-term.md`](design-empty-term.md): `Absent`, the empty term,
  and the store's two layers.
- [`design-change-identity.md`](design-change-identity.md): a change is
  named by what it says and what it supersedes.
- [`design-register-types.md`](design-register-types.md): register types;
  the history is free, and a schema is its reading.
- [`SIMPLIFY-PLAN.md`](SIMPLIFY-PLAN.md): one functor, one schema, one
  evaluator, one fold.
