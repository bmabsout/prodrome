# Changelog

All notable changes to this project are documented here. The format is
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html) for its
API. THE STORED BYTES ARE A SEPARATE PROMISE and a stricter one: a shipped
constructor's fields never change, in any release.

## [Unreleased]

The Prodrome in the browser, a read-only viewer of this crate's roadmap
folded and typeset on the reader's machine, and GitHub issues mirrored onto
that roadmap. Typst and GitHub enter the repository only as optional extras
beside the core; `core/` and `cli/` depend on neither, and no stored byte
changes.

And `Absent`, the empty term (`docs/design-empty-term.md`): an object with no
claim on attention is priced by a term whose value is `∅`, so FPL is total
and an unpriced todo stops being a special case in every reader. A MINOR
change under this file's rule: no stored object changes, a term without
`Absent` reads exactly what it read, and the API breaks where a value was an
`f64`.

And the object store is durable and its fsck honest, which are PATCH fixes
under this file's rule: no stored byte changes, and a healthy store reads and
verifies exactly as it did.

### Added

- **`Absent`, a new leaf of `TermF` (SPEC §7):** `∅` at every instant,
  printed `Absent()`, built by `fpl::mk_absent`. `∅` is the identity of
  composition: `Conj` and `Within` mean over what has a value (`∅` when
  nothing does; `Conj([])` is still 0.5), an absent gate or `OffsetBy` delta
  is none, and every other operator passes `∅` through. `After`'s and
  `Recur`'s `pending` may be absent; a cancelled `After` is still 1.0.
- **A `Ref` to a known todo with no spec links to `Absent` (SPEC §7.2)**,
  where it was refused; `LinkError::Unknown` is now only an id the store
  has never seen, and says so. `fold::link_specs(functions, known)` takes the
  todos the store knows.
- **One list order (SPEC §6.7):** `view::list_order`, ascending in value,
  ties by id, then every row with no number, `absent` or not linking, by id.
  `prodrome list` sorts by it; prodrome-wasm's `entries` answers it as
  `order`; the Typst package lists by it (`listed`, which replaces
  `by-fulfillment`) and sorts nothing itself.
- prodrome-wasm: `fold` answers `functions`, every todo the objects mention
  with its function or `absent`, which is what `link` binds against; the JSON
  codec has `{"kind": "absent"}`.
- `fpl::holds_absent`. The chain compiler writes a cancelled link over a body
  holding an `Absent` as `Gate(body, Flat(1.0))`, since an offset of `∅` is
  `∅`; an `Absent`-free body is still `Offset(1.0, body)`.
- **SPEC law 18** and its evidence: `core/tests/fpl_laws.rs` (an absent
  member changes no conjunction, a term without `Absent` has a value
  everywhere, `∅` composes as §7 says, `explain` carries it and gives it no
  share, an exact term's knots are its curve), whose generators, moved to
  `core/tests/common/terms.rs`, now draw `Absent` so laws 5, 13 and 14 cover
  it too; `core/tests/fold_laws.rs` (a `Ref` links to `Absent` for a known
  todo with no function, and every row reads that link).
  `conformance/absent/{fpl,series,view}.py` are NEW and SEEDED like the view
  vectors, by `core/examples/generate_absent_vectors.rs`, run by hand.

- **`typst-wasm/`**, Typst 0.15.1 compiled to WebAssembly
  (`nix build .#prodrome-typst-wasm`), in a cargo workspace of its own so the
  core's lock file never sees it. Exports `compile_html(source, files)` (the
  HTML document or diagnostics with spans, as a value, never a throw),
  `highlight`, `complete`, `hover` and `add_font`; every offset is UTF-16.
  The module is 22.0 MB, 6.3 MB with brotli.
- **`typst/`**, the `prodrome-typst` package: the roadmap list ordered by
  fulfillment, an item's page, and two marks: a value's pie beside its
  percentage, and a todo's thirty days (fifteen back, fifteen ahead) as one
  thick line, the past faded, in a 0–100% frame. Every value is drawn in
  `colour(v)`, a sample of one continuous OKLCH gradient from red to green;
  Problem, Watch and Fine are words only. Instants read as dates, and an item's
  price in words ("42% now, falling from 55% to 5% by Sep 29, 17:00, over 3
  days"), with its explanation's parts beneath it. It reads the core's JSON
  and computes nothing about fulfillment. `checks.prodrome-typst` compiles
  its examples to PDF and HTML, and its laws (`tests/laws.typ`).
- **`viewer/`**, the static web app (`nix build .#prodrome-viewer`), with
  routes `#/` and `#/todo/<id>`, a Typst editor demo on the item page, and a
  service worker that caches the shell per build and the objects forever. It
  never scrolls sideways at a phone's width, and its smoke test checks the
  list against `prodrome list`'s order at 420px.
  `viewer/assemble.sh` puts any store's objects beside it.
- **`.github/workflows/pages.yml`** deploys it on pushes to `main`, by hand,
  and after `roadmap-data`'s `verify` succeeds on a push (`workflow_run`: a
  push trigger for that orphan branch would never fire from `main`).
- `prodrome-wasm`: `entries` also answers `created`, each todo's first
  `Created` instant and last `Created` text, the body `prodrome list` shows
  for a todo with no record. Additive; the rows are unchanged.
- **`prodrome-github PAYLOAD.json`**, in its own crate `github/`, apart from
  the core and its CLI: one GitHub webhook delivery, `issues` or
  `issue_comment`, applied to a store. An opened issue becomes item
  `gh-<number>` (a `Created` and a record with the title as its body, the
  issue's URL as its `source.message_id`, category `issue` and a flat 80%); a
  retitle is a content revision with no spec; closed is `Completed` and
  reopened is `Reopened`. A `/price` comment by an OWNER, MEMBER or
  COLLABORATOR is a `SpecRevised` written as the commenter: `/price 40`,
  `/price 30 --deadline 2026-10-15 [--end N] [--lead-up DAYS]`, `/price ref
  gh-7`, or `/price accept`, which re-issues `pricing-bot`'s latest proposal.
  Anyone else's `/price` is ignored before it is parsed. `--proposal FILE`
  records a pricing reply's `/price` line as a claim by `pricing-bot`, dated no
  earlier than the store's latest event, so it keeps §3's clock rule for a
  reader who names the bot untrusted. Deterministic and offline: every
  instant comes off the payload, and a delivery already applied appends
  nothing. Pinned by fixture payloads and two properties — replaying what was
  applied changes nothing, and close then reopen folds to open.
- **`.github/workflows/mirror.yml`** runs that verb on every issue event and
  `/price` comment and pushes the result to `roadmap-data`, welcoming each new
  issue once with its item link.
- **`.github/workflows/price.yml`**: Claude, with no tools and one turn,
  proposes a price for each new or edited issue; a job without the model
  records it as a `pricing-bot` claim and posts it.
- `docs/github-agent.md`: the flow and its trust boundaries.
- `Price::DEFAULT_START`, `DEFAULT_END` and `DEFAULT_LEAD_UP_DAYS` in
  `prodrome-cli`, so `--deadline` and `/price … --deadline` write one decay.

### Changed (breaking)

- **A value is `Option<f64>`**, `None` being `∅`, so it cannot be read as a
  number: `fpl::fulfillment`, `Compiled::fulfillment`, `Explanation::value`
  and `breaks::Knot::value`. prodrome-wasm's `fulfillment` answers
  `undefined` for `∅`, and a knot or an explanation node `null`.
- **Every `view::Entry` has a function:** `Priced` is `Price` and
  `Entry::priced: Option<Priced>` is `Entry::price: Price`, whose `spec` is
  `Absent` where the todo has no spec and no checklist and whose `value` is
  `Result<Option<f64>, LinkError>`. `Entry::spec` answers `&Term` and
  `Entry::value` a number, `∅` or the `LinkError`.
- On the wire an entry's `value` is a number, `"absent"`, or `null` where it
  does not link, and its `spec` is always a print. `prodrome show` prints
  `price absent` and the spec `Absent()`; `prodrome list` shows `∅` in the
  price column (`—` still marks a function that does not link).
- The Typst package reads `order` from its data, and draws `"absent"` and
  `null` values as unpriced; the viewer marks every todo the store knows.
- The frozen view vectors change in exactly their unpriced rows, which now
  read `value='absent'` and `spec='Absent()'`.

### Fixed

- **A write is durable (SPEC §3).** An object was written to `<name>.tmp`
  and renamed with no sync, so a power loss could leave a zero-length file
  under a name that promises content. It is now written to a randomly named
  temp file created exclusively, synced before the rename, and `objects/` is
  synced once per append or adoption. Law: a crash between the write and the
  rename leaves no object, only a temp that `verify` reports
  (`a_crash_before_the_rename_leaves_only_a_temp` in `core/src/store.rs`).
- **`verify` sees everything in `objects/` (SPEC §3).** It looked at `.py`
  files only, so a temp an interrupted write left, or any other stray, was
  never reported. Every entry that is not `<name>.py` for a well-formed name
  is now a finding, by name; the reads pass over such entries rather than
  refusing a `.py` whose stem is not a name. Law: `verify_names_every_stray`
  in `core/src/store.rs`.
- **One damaged object no longer stops every write (SPEC §3).** `tips()`
  refused a store holding a file that fails its hash, and `append` asks
  `tips()`, so one such file stopped the store with no way out. It still
  refuses rather than guess, and now names the object and the fix:
  `EventStore::quarantine(name)`, and `prodrome quarantine <name>`, move a
  file failing its hash from `objects/` to `quarantine/` (a file that hashes
  to its name is refused), after which the store answers and `verify`
  reports a receipt for the quarantined file until the object is restored.
  Test: `after_quarantine_the_store_answers_and_verify_holds_the_receipt` in
  `core/src/store.rs`, and `prodrome-cli`'s verbs.
- **Git must not rewrite object bytes (SPEC §3, README).** A store under git
  needs `objects/** -text -diff` in its `.gitattributes`, or a line-ending
  conversion leaves files that no longer hash to their names; documented,
  and added to this repository's own `.gitattributes`.
- `breaks::series_knots` puts a knot a second before a jump that falls
  exactly on the window's end, as it does for every other jump, instead of
  drawing a ramp across it. No frozen series moves.

### Unchanged

- **The stored bytes.** `Absent` is a new constructor; a record whose `spec`
  is `None` reads as `Absent` at the todo, and `flatten` is unchanged, so an
  old store reads as before except that a `Ref` to an unpriced todo now
  links. Every reference vector answers what it answered.

## [0.9.0] - 2026-09-27

A MINOR version under this file's rule: no stored object
changes, every vector answers what it answered, and the API breaks where the
heads used to be. A store's tips are DERIVED from its objects (SPEC §3), so a
store is its `objects/` and nothing else, and a `git merge` of two clones of a
store is the Prodrome's own union with nothing that can conflict.

### Changed (breaking)

- **Tips are derived (SPEC §3):** the objects no object names as a parent.
  `store::tips_of` is the pure function; `EventStore::tips` applies it to the
  store and is now fallible (`Result<BTreeSet<Hash>, _>`), since it reads
  every object it has not verified.
- **`HEAD` and `refs/` are gone, not cached.** Nothing needed them: every read
  already loaded the objects, the objects imply exactly the heads the files
  named (law 17, checked on every `conformance/dag.py` store laid out as the
  0.8 writer left it), and a cache is a second record of the heads that can
  disagree with the first — which in a git-tracked store is also the one file
  two clones can conflict on, the thing this change exists to remove. Old
  stores READ unchanged; the store never writes or deletes those files, and
  `verify` reports each as a leftover, so dropping them (`git rm HEAD refs`)
  is an explicit act. This repository's own `roadmap/HEAD` is removed; the
  tip derived is the tip it named.
- `EventStore::tip` is removed. "The" tip of a store with two is not a
  question with an answer; callers ask `tips`.
- `append(event, None)` seals on EVERY tip: none is genesis, one is a
  `Sealed` byte for byte as before, several is a `Woven` carrying the event.
  In 0.8 it sealed on `HEAD` alone and left the other head standing; now the
  next write after a union settles it. `merge(None, _)` joins every tip, as
  before.
- `verify` no longer reports an unreachable object or a stale head (neither
  can exist: every object is a tip or beneath one), nor anything about HEAD
  and `refs/` agreeing; it reports a leftover `HEAD` or `refs/`. Every
  missing parent is reported, not only the first a walk reached. An object
  that nothing names — once an "orphan" — is a tip.
- `adopt`/`adopt_objects` verify every object before writing any, and write
  parents before children, because an object is in the store the moment its
  file is: a refused adoption leaves nothing behind, and a crash mid-adoption
  leaves a store closed under parents. There is no placement step left.
- `prodrome-wasm`: `verify_objects(objects)` derives the tips from the
  objects it was handed (the `tips` argument is gone) and answers them.

### Added

- `EventStore::objects`, every object by name, reverified — the one read the
  others are taken from; `read_dag` is one pass over the files.
- **A parent index** in each `EventStore` handle (shared by its clones): name
  → parents for every object it has verified, so `tips` is a directory
  listing plus the objects it has not seen. A memo of a pure function (a
  name is the hash of its bytes), so it needs no invalidation, and a deleted
  file is simply no longer listed. Measured in release, on a synthetic store
  of 1000 objects (366 KB): 39 ms to derive by loading everything, 2.8 ms
  warm through the index, against 2.6 µs to read 0.8's `HEAD`; 0.6 ms for all
  40 `dag.py` stores. The one thing it gives up, stated and test-pinned: a
  handle that verified an object before it was tampered with keeps its
  parents for `tips`. Every read of the objects, and `verify`, rehash every
  file.
- **SPEC law 17** and its evidence: `core/tests/tips.rs` (a union's tips
  compose, and two stores' files in one directory derive them; the listing
  order does not matter; the tips cover the store and none rests on another,
  on random DAGs, pure and on disk); `core/tests/fold_laws.rs` (two writers'
  unioned directories read, fold and view as their `adopt`-merge, and the
  next append weaves what a merge would); `core/tests/dag.rs` (derived tips
  equal the old `HEAD`/`refs/` on every conformance store). The vectors are
  unchanged.

## [0.8.0] — 2026-09-25

A MINOR version: recurrence, as one new event kind and two new terms. No
stored byte changes and every existing vector answers what it answered; the
environment's type grows a second half, and every caller follows it.

### Added

- **`Tended(todo, at, actor, note)`, a new event kind (SPEC §4):** a pass at
  a todo that is never done. It changes no state, spec or content, and it puts
  no piece in `flatten`. `event::mk_tended`; the reference policy lets a
  roster actor's tending only claim, like a completion.
- **The tendings (SPEC §6.1):** per todo, the instant of every binding
  `Tended`, a grow-only set folded by union. `env_at`, `history` and the
  registers carry it; a tending writes no register, so concurrent tendings
  are their union and never a conflict. `fpl::last_tended(env, todo, now)` is
  the latest tending at or before `now`.
- **`Recur(todo, anchor, term, pending)` (SPEC §7.3):** `term`, authored
  against `anchor`, re-anchored to the last tending of `todo` as `After` is to
  a completion; `pending` before the first. `mk_recur` refuses a `todo`
  `TodoId` would refuse. Its explanation notes `bound` (`pending` or
  `tended`), and when tended, `tended` and `agoHours`.
- **`Periodic(period, anchor, term)` (SPEC §7.3):** `term` read at `anchor +
  ((now − anchor) mod period)`, the remainder Euclidean, so moments before
  `anchor` repeat too. `mk_periodic` refuses a period that is not positive.
  Its explanation notes `cycleStart`.
- `fpl::phase(period, anchor, now)`, the instant a `Periodic` reads.
- The chain compiler resolves a `Recur` into the schedule its tendings make:
  pending before the first, and from each tending the body slid by its
  slippage. §9.13 now quantifies over tendings too.
- **SPEC laws 15 and 16** and their evidence: `core/tests/fold_laws.rs` draws
  `Tended` among the generated kinds and checks a tending changes no state,
  spec, content, function, register or entry, and that tendings merge by
  union; `core/tests/fpl_laws.rs` checks `last_tended` reads as of now,
  `Recur` re-anchors, waits and ignores later tendings, `Periodic` repeats,
  and both round-trip and refuse what they must. `conformance/recur.py` is
  NEW and written BY HAND: `Tended` prints folded under the reference policy,
  a claimed pass among them, and readings to 1e-9. `Recurs` and `RecurCase`
  join the vector vocabulary.
- `prodrome-wasm`: `lifecycle` spells `Tended`; the JSON codec has `recur`
  (`todo`, `anchor`, `term`, `pending`) and `periodic` (`periodHours`,
  `anchor`, `term`).

### Changed (breaking)

- `fold::Env` and `fpl::Env` are structs, `outcomes` beside `tended`
  (`TodoId`/`String` → `BTreeSet<Instant>`), where they were maps of
  outcomes. A caller that read the map reads `env.outcomes`; `Env::new()` is
  still the empty environment. `fold::History` holds the tendings too and
  answers them from `tended()`.
- `TodoEvent` has a `Tended` arm; an exhaustive match gains one.
- The wasm environment crosses as `{"outcomes": {…}, "tended": {"<todo>":
  ["<iso>", …]}}`, both in `fold`'s `env` and in what `fulfillment`,
  `explain` and `compile` read; `fold`'s `history` is `{"bindings": {…},
  "tended": {…}}`, which `series_knots` reads. A half left out is empty, so
  `{}` is still the empty environment; any other key, the old bare map
  included, is refused.

### Unchanged

- **The stored bytes**, and every existing conformance vector: `Tended`,
  `Recur` and `Periodic` are new constructors, and a store without them reads
  exactly what it read before. `flatten` gains no case.

## [0.7.0] — 2026-09-25

A MINOR version: a new constructor, and the evaluator's signatures move so
that an unbound reference is a type error. No stored byte changes, and every
existing vector answers what it answered.

### Added

- **`Ref(todo)`, a new FPL leaf (SPEC §7.2):** the fulfillment of another
  todo, for subtodos and groups whose demand is composed from other todos'.
  It prints as `Ref(todo='<id>')`; `mk_ref` and the parse refuse an id
  `TodoId` would refuse. A term holding one is OPEN.
- **`fpl::link(term, specs) -> Result<Closed, LinkError>`:** every `Ref(x)`
  replaced by `specs[x]`, recursively — bind for the free monad over `TermF`.
  A rebuilt `Piecewise` goes back through the normal form. Refusals are
  values: `LinkError::Unknown(id)`, and `LinkError::Cycle(path)`, found on
  the path being expanded, so `link` is total and never loops.
- `fpl::Closed`, a term with no `Ref`: `Closed::of(term) -> Option<Closed>`,
  `term()`, `into_term()`, `normalize()`. `TermF::transpose`, the traversal
  in `Result` that `link` is written with. `From<LinkError> for
  ProdromeError`.
- `fold::link_specs(&flatten(…))`: a chain's functions as `link` reads them,
  so `Ref(x)` means x's whole function, its lifecycle included.
- `Entry::unlinked() -> Option<&LinkError>`.
- **SPEC law 14** and its evidence: `core/tests/fpl_laws.rs` checks a closed
  term links to itself, a reference reads what its spec reads, linking
  commutes with substitution order, the linked term is in normal form, a
  loop is refused with a real cycle, and `Ref` round-trips print and parse.
  `conformance/link.py` is NEW and written BY HAND from those laws: exact
  linked prints, readings to 1e-9, an unknown todo and a cycle refused. Its
  names, `Links`, `LinkCase` and `Refused`, join the vector vocabulary.
- `prodrome-wasm` exports `link(term, specs)`, answering the closed term in
  the JSON shape (`{"kind": "ref", "todo": …}` is the new tag). `json_entry`
  grows one key, `unlinked`.

### Changed (breaking)

- `fpl::fulfillment` and `fpl::explained` take `&Closed`, as do
  `breaks::series_knots` and `chain::compile`; `chain::compile_chain` and
  `chain::chain_order` take `&BTreeMap<String, Closed>`, and
  `Compiled::term()`/`into_term()` answer a `Closed`. A caller holding a
  `Term` with no reference wraps it with `Closed::of`; one holding a
  function from `flatten` links it with `link(term, &link_specs(&functions))`.
- `view::Priced::value` is `Result<f64, LinkError>`: the entry's value is its
  function LINKED against every todo's function (§6.7). `Entry::value()`
  still answers an `Option<f64>`, `None` where the function does not link.
- The wasm exports that evaluate refuse a term that still holds a `ref`.

### Unchanged

- **The stored bytes**, and every existing conformance vector: `Ref` is a new
  constructor, and a store with none reads exactly what it read before.

## [0.6.0] — 2026-09-10

A VERSION because the core's public API grew: `prodrome::chain` is a module a
reader can depend on. NOTHING ELSE MOVED — no stored byte changes, no vector
changes, no reading changes, and the interpreted path is the one it was.

### Added

- **`chain`, the chain compiler (SPEC §7.1).** `After` is the ONE term that
  reads history; every other constructor is a function of `now` and its
  subterms. `chain::compile(term, env)` erases it — the term comes back with
  every `After` resolved against the snapshot and `normalize`d, so a chain of
  dependencies is ONE schedule at the root instead of a freeze quantifier the
  evaluator re-enters at every sample. `Compiled`'s readers — `term`,
  `fulfillment(now)`, `explain(now)`, `notes`, `links`, `moot` — take NO
  ENVIRONMENT, which is the claim: a compiled term cannot consult one, because
  the only constructor that would is gone.
- EACH LINK IS ONE GRADED OFFSET δ ∈ [0, 1], applied with §7's corrected form.
  `x·(1 − |1|) + max(0, 1) = 1` for every `x`, so a CANCELLED upstream is
  `Offset(1.0, …)` — the moot constant written as the offset it is, with the
  demand that was dropped still in the tree where a reader can see it. δ = 0 is
  the identity and is elided, as is a zero `Shift`. A completed upstream is a
  `Piecewise` whose piece slides the body by the slippage; an unbound one is
  its pending branch and nothing else.
- A cancellation is REPORTED and never silent (§7): every link is a `Link` on
  the compiled term, `moot()` names the cancelled ones, and `explain` carries
  them at the root under `moot`. A 1.0 from a cancellation is indistinguishable
  from a 1.0 from a demand met, and the note is the only thing that tells them
  apart.
- `chain_order` and `compile_chain` take the chain as a MAP of todo to
  function — `fold::flatten`'s answer — and read the graph its links draw over
  those todos. A cycle is refused as a VALUE, `ChainError::Cycle(path)`, the
  path naming the loop; a link onto something outside the map is not an edge,
  because the snapshot answers it.
- **`After.needs` is CONSUMED, by the compiler and not by the evaluator.** §7's
  semantics have never read it and this does not start: consuming it as a value
  would move a reading. It is the link's declared LEAD TIME, and the compiler
  is the first reader holding it beside the upstream's actual instant, so a
  resolved link reports `ready = at + needs`. A host schedules by it.
- **SPEC §9.13**, the law that makes the above an OPTIMISATION and not a second
  semantics: `fulfillment(compile(t, env).term, now, ∅)` equals
  `fulfillment(t, now, env)` to 1e-9 at every instant, the compiled side read
  against the EMPTY environment. `core/tests/fpl_laws.rs` quantifies it over
  the same `a_term()` and `an_env()` §9.5 runs on, with two more properties
  beside it — no `After` survives, and an environment that answers differently
  about everything changes nothing.
- `core/tests/chain.rs` pins the cases: what each link compiles to, as an exact
  print; the cancellation report; that `needs` moves no number; the cycle and
  its message; and THE COST, as a test rather than as a claim — a chain of
  three links under one `Within` costs up to 65·3 = 195 environment lookups per
  interpreted evaluation and none at all compiled, asserted on the counting env
  in both its readings (the `After` nodes that could ask, and an environment
  that would show if one did) and never on the clock.
  MEASURED AND STATED AS MEASURED: 390 000 lookups become 0 for a 19 µs
  compile, and the wall clock moves about 7% in release, because a lookup in a
  three-entry map is cheap. What the compiler buys is that the environment
  leaves the query path — a compiled term can be cached, stored, shipped and
  evaluated where no history exists — and not a constant factor.
- `prodrome-wasm` gains ONE export, `compile(term, env)`, returning the
  compiled term's canonical print beside its links. The other exports are
  unchanged in shape and in answer.

### Unchanged

- **The stored bytes**, and every conformance vector. The compiler adds no
  constructor, reads no new field and changes no existing term's value; §9.8's
  150 terms answer exactly what they answered, which is the frozen pin under
  §9.13's property.
- **The interpreted path.** `fpl::fulfillment`, `fpl::explained` and
  `breaks::series_knots` are untouched, and a host that never compiles reads
  what it read before. Compiling is an optimisation a caller opts into.
- The wasm wire, apart from the one export added above.

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
