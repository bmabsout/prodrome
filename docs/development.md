# Development

How to build and test Prodrome, what CI runs, what the conformance vectors
are, and where each part of the repository lives. To change semantics,
change [`SPEC.md`](../SPEC.md) in the same pull request, and add a law or a
vector that pins the change.

## Testing

```console
$ nix flake check              # fmt, tests and clippy, in the sandbox
$ nix develop -c cargo test    # the same with a toolchain in hand
$ PRODROME_MAX_CASES=32 nix develop -c cargo test   # each law at most 32 cases
```

EVERY PROPERTY LAW STATES ITS FULL CASE COUNT, and `PRODROME_MAX_CASES` caps
every law at that many cases: the same laws, each over fewer draws, and none
skipped. The rule is one function, `core/tests/common/cases.rs`, and every
law's configuration goes through it.

## CI

`.github/workflows/ci.yml` runs two jobs on the flake's toolchain:

- `cargo`, the loop. It runs two checks in parallel, through `nix develop`:
  `lint` (fmt, clippy at `-D warnings`, the doctests) and `test` (every
  other test in the workspace, then `prodrome verify` and `list` over the
  `roadmap-data` branch, with the binary the tests built). Each check
  restores its `target/` from its own run on main. A file counts as
  unchanged when its git blob name is the one that `target/` was built from
  (`.github/scripts/target-sources.sh`), so a pull request recompiles only
  the crates it changed. A pull request runs each law at 32 cases at most;
  main and the nightly run (fresh seeds) run the full counts.
- `nix`, the gate: `nix flake check`. It runs everything above at the full
  counts in the sandbox, and builds the wasm, the viewer, Typst and the
  release binaries. It runs on pull requests and on main, and its store
  paths go to the public `bmabsout` Cachix cache.

The cap of 32 was measured. Each law was run at caps from 2 to 128, and
compared with two runs at the full counts. At 32, the laws together cover
98.7% of the core's lines that the full runs cover. Apart from the fuzz
laws, a cap of 16 leaves eight laws short of what one full run reaches, and
32 leaves three; the furthest of them reaches 90%. The four laws in
`core/tests/all/fuzz.rs` are the exception at any cap: they search 4096
random inputs for rare ones, and only main and the nightly run carry them.

## Conformance vectors

`conformance/` holds vectors that an implementation must reproduce. The laws
in SPEC §9 are property tests over generated logs and two-replica stores.

THE VECTORS ARE LITERALS of the crate's own grammar (SPEC §2). The crate's
own parser reads them, against a second closed vocabulary. Evidence for "one
grammar, one printer, one parser" written in a second format would be a
second format to keep in step. Every stored object, event and term inside a
vector is a string that holds its canonical print, because those bytes are
what the vector is evidence of.

Most vectors were taken from a reference implementation. These are the
exceptions:

- **`conformance/view/*.py`** (SPEC §6.7) is SEEDED: there is no reference
  to take it from any more. It comes from this crate's own log generator
  (`a_log` in `core/tests/common/mod.rs`), under a fixed seed. It is still
  FROZEN: `cargo test` only reads it. Regenerating it is a separate, manual
  step, `cargo run --example generate_view_vectors -p prodrome-core`, that
  `nix flake check` and CI never run.
- **`conformance/absent/*.py`** (SPEC §9.18) is seeded in the same way, for
  `Absent`: terms, exact series and logs drawn from the generators that the
  laws of `∅` run over (`core/tests/common/terms.rs`, and
  `a_log_with_absence`). It is frozen, and regenerated only by hand with
  `cargo run --example generate_absent_vectors -p prodrome-core`.
- **`conformance/link.py`** (SPEC §7.2) is written BY HAND from `link`'s
  laws: a few specs, the prints they link to, their values at a few
  instants, and one refused loop. A law's own consequences are the evidence
  there, and a reader can redo every number in it.
  **`conformance/recur.py`** (SPEC §7.3) is written in the same way: a chore
  tended twice and claimed once, a rent that repeats, and their readings.

`wasm/conformance/` holds the JSON boundary's own evidence: the `to_json`
and `explain` shapes for the same 150 terms, frozen when the codec moved
there.

## Repository layout

```
core/         prodrome-core: the database. Its module list, in
              core/src/lib.rs, maps each module to its SPEC section.
              `reference` is the payload the vectors were taken with.
cli/          prodrome-cli: the `prodrome` binary. The verbs, over the
              reference payload and the reference policy, and the only
              clock in the workspace.
wasm/         prodrome-wasm: the core compiled for the browser, built with
              that payload. `wasm/exports/` holds its exports, generic over a
              schema, and `json`, the term codec the JavaScript side reads.
conformance/  the vectors, as literals of the grammar in SPEC §2
docs/         the guide, the design notes, and these pages
.github/      CI, and the workflows that mirror and price issues and
              publish the viewer
SPEC.md       the specification

              OPTIONAL EXTRAS; nothing above depends on them:
github/       prodrome-github, the binary that mirrors GitHub issues into a store
typst-wasm/   Typst compiled for the browser, its own workspace
typst/        prodrome-typst, the Typst package of layouts
viewer/       the static web app published to GitHub Pages
```

**The core stays free of any markup language.** `core/` and `cli/` store a
record's text as opaque strings. They never read it as Typst, Markdown or
anything else, and the issue mirror writes titles as plain text. The viewer
renders a store whose items hold Typst. It is an optional extra built on top
of the core, and an example of how to use it, not a part of it.
