# Design: a change is named by what it says and what it supersedes

Status: DESIGN, 2026-09-28, not implemented. The contract it proposes is
SPEC.md's "Draft A", which amends §3, §6.6, §6.7 and §9 and is not in force
until an implementation lands it. This page is the reasoning. A second
session builds it (the plan is at the end).

## The problem

Reading the store the way the Pijul maintainers would:

1. **A change is named by its POSITION.** `append` seals the event on every
   tip (§3), so an object's name hashes the whole global tip it was written
   on. The same logical change, made on two replicas, re-proposed after a
   rejection, or resubmitted by a host that retried, is two different
   objects. So idempotence, "the second time is a no-op", is not a property
   of the database. Every host that needs it builds it again.
2. **Its dependencies are everything before it**, never what it needs. A
   `Completed` on todo `a` "depends on" an unrelated edit to todo `b` because
   `b` happened to be at the tip. So two changes to different todos are never
   independent in the DAG, only in the folds (law 3 holds, but only because
   the folds ignore most of the DAG). A replica cannot take one todo's
   history without everything that happened to be written before it.
3. **A merge is an object.** A `Woven` over two tips is a fact written by
   whoever merged, named by what they held. Two replicas that each merge the
   same pair of histories agree only when neither appended in the meantime.

The brief this page answers was cut off after point 1. Points 2 and 3 are
reconstructed from its framing (the Pijul model: a change is its content plus
its explicit dependencies, and a state is a set of changes). The reviewer
should say if the brief went somewhere else.

The evidence that point 1 is real is in this repository. `prodrome-github`
works around it twice. It dedups replays by "an event whose canonical print
the store already holds is not written again" (`github/src/lib.rs`, module
doc). It dedups `/price accept` by `(todo, actor, at, note)` (`Known::accepted`).
And it dates the bot's claims at the latest instant of EVERY event in the
store (`proposed`), because §3's clock rule is checked against ancestors and
a claim's ancestors are the whole store. Each of these is the database's
job.

## What stays

The Pijul model is not taken whole. Pijul's changes are edits to a text,
merges have to be computed, and a conflict is a graph state. Prodrome's
changes are writes to registers (§6.6). The registers already are Pijul's
best idea: a conflict is a VALUE (a frontier of two or more writes), it is
shown and never hidden, and the next write that descends from it settles it.
What is missing is only at the bottom: what a write descends FROM.

So these do not move: events (§4), standing (§5), every fold's definition
(§6), FPL (§7), content addressing (a name is the hash of the print), the
union of files as the merge, and every stored byte. `Sealed` and `Woven` stay
readable forever (§1: add kinds, never change shipped fields).

## The proposal

### 1. One new envelope: `Change(deps, event)`

```
Change(deps=('3f…', 'a0…'), event=Completed(todo='a', …))
```

- `deps` is a tuple of object names, sorted and distinct, possibly EMPTY.
  `event` is required: a change without an event would be a merge, and there
  are none (4 below).
- Its name is `sha256(utf8(print(envelope)))`, as for every object.
- `parents_of(Change(deps, _)) = deps`. That one line is how §3's tips,
  ancestry, linearisation, `concurrent`, `adopt` and `verify` and §6.6's
  registers read it. They are defined over parents and they stay so.

### 2. `deps` is the frontier of the registers the event writes

When a writer appends `event`, `deps` is the union, over the registers
`writes_of(event)` names (§6.6), of that register's frontier in the writer's
store. It is computed STRUCTURALLY, under the policy where everything binds
and with no `t`. The name has to be a function of what the writer held, and
never of a reader's policy or moment: a claim a reader refuses is still
something the writer saw and wrote over.

- `Completed(a)` depends on the writes in `a`'s state register, and nothing
  else. An edit to `b` is not beneath it.
- `Authored(a)` carrying a spec depends on `a`'s content and spec frontiers.
- `Created` and `Tended` write no register, so their `deps` are `()`. A
  creation or a pass is a fact with nothing to supersede. The same `Tended`
  mirrored twice is one object.
- A write to a register with a conflict depends on every write in the
  frontier, so it SETTLES the conflict, exactly as §6.6 says a descending
  write does today.
- `deps` is an antichain: no dep is an ancestor of another, because a
  frontier never is.

What this buys: **a change's name is a function of its event and the part of
the past it supersedes, and of nothing else.** Two replicas that agree about
`a`'s state register write byte-identical `Completed(a)` objects, whatever
each holds about every other todo. Their union holds one file.

This is exactly §6.6's "later means descends from, and nothing else", moved
from the reading to the writing. Registers already ignore the rest of the
DAG, and now the DAG leaves it out too.

### 3. `append` is idempotent

`append(event)` first asks whether the store holds an object whose event
prints byte-identically to `event`. If it does, it writes nothing and answers
that object's name, the first such in the linearisation. Otherwise it writes
`Change(deps, event)`.

The check is over the WHOLE store, not over the current frontier. That is a
deliberate choice, and the reason is a replay after a rejection. The bot
proposes `P`, a maintainer overrides it with `M`, then the webhook delivery
that carried `P` is replayed. A frontier-only check would miss `P` (it is
superseded) and write a new `P'` depending on `M`, which would resurrect the
rejected proposal in the claimed reading. A whole-store check writes nothing.
A host that means to assert the same thing AGAIN, having seen `M`, is saying
something new, and its event differs: a new `at`, a new note.

So identity is exactly as good as the host's determinism. The database cannot
tell one act retried from two acts that print alike, and it does not try:
`at` is data (§1) and the host stamps it. The mirror already stamps from the
payload, and a host stamping from a clock gets a new change per attempt,
which is what its bytes say. (Pijul is the same: a change's header carries
its timestamp.)

This is the property `docs/design-empty-term.md` wanted for proposals: "a
second submission of the same object reads that completion and returns the
same response". The name `append` answers is the key the effect is recorded
against.

### 4. No merges

A store of changes is a SET. Its reading is the folds over the linearisation
of that set, which is already a function of the set alone (§3, law 17). Two
replicas are merged by uniting their files, as today. With `deps` per
register there is nothing to weave: after a union, the next write to a
register depends on whatever that register's frontier holds, including a
conflict it settles. `append` never writes a `Woven` into a store of changes.
`merge` stays for stores written before this, which it can still settle
(write 7 below).

The tips remain derived and law 17 still holds of them, but they stop
MEANING "the heads to seal on". In a store of changes there are many tips,
roughly one per register chain, and nothing reads them to write.

### 5. Twins, and the one fold change

Two replicas that DISAGREE about a register can still each write the same
event over it: the same delivery, processed by two runners that had diverged
on that todo. Those are two objects, `e₁` over frontier `{x}` and `e₂` over
`{y}`, carrying one event. Call them twins. They are distinct changes: each
records a different view, and the name is honest about that.

The fold keeps them distinct with one exception. **A frontier whose writes
all carry byte-identical events is a value, not a conflict.** Two replicas
that independently made the same write agree, and showing "`e` versus `e`"
would be noise the next write has to clear. Every other reading of twins is
the DAG's: each supersedes what its own deps name. A later write that
descends from `e₁` only leaves `e₂` in the frontier. If that write differs
from `e`, this is a real conflict, and it is shown: the replica that wrote
`e₂` never saw it.

**Rejected: quotienting the DAG by event**, which would treat twins as one
node whose deps are the union. It would let `e` settle `y` on the strength of
a view nobody who wrote `e₁` had. It would also break §6.6's cheap monoid
action, because a twin arriving late would move a node already positioned
and every descendant's ancestry with it.

**Rejected: naming a change by its event alone**, with deps kept beside it.
Two replicas would write different bytes under one file name, so the union of
files could conflict, which breaks law 17, the reason the store merges by
`git merge`.

### 6. What `verify` checks

Everything it checks today, reading `deps` as parents, plus these:

- a `Change`'s `deps` sorted and distinct;
- no dep an ancestor of another (an antichain, as a frontier is);
- every dep writes at least one register the event writes. A dep that
  doesn't is not a frontier member the writer could have seen: it is a
  hand-written object or a writer's bug.

The clock rule (§3) keeps its wording, "dated before any of its ancestors",
and gets narrower ancestors. That is what it always meant: a writer cannot be
dated behind what it wrote OVER. `proposed`'s workaround of dating a claim
past everything in the store becomes dating it past its deps.

### 7. Stores written before this

A store with no `Change` reads, folds, derives and verifies exactly as it
did: no stored byte moves. In a mixed store, a `Change`'s deps may name
`Sealed` and `Woven` writes. A legacy object's ancestry is still its global
position, which OVERSTATES what it depends on and is therefore safe: it can
only make a later write settle more, never less, than the chain said. No
migration rewrites anything. An 0.9 writer appending into a mixed store seals
a `Woven` over every tip, which is legal and merely conservative.

An older READER refuses a store that holds a `Change`: the constructor is not
on its whitelist (§2), and the refusal names it. `Tended` and `Absent` set
the precedent. Under CHANGELOG.md's rule this is a MINOR change: a new
constructor, no shipped field moved.

## The laws (Draft A, §9)

- **19. A name is its event and its view.** The object `append(event)` writes
  is a function of `event` and of the frontiers of the registers `event`
  writes, read structurally, and of nothing else the store holds. Adding to a
  store objects that write other registers changes no name `append` would
  write. Two stores that agree on those frontiers write byte-identical
  objects.
- **20. Append is idempotent.** Appending an event whose print the store
  already holds writes nothing and answers the first object in the
  linearisation that carries it. A replay of any sequence of appends, in any
  order, onto the store they produced leaves its object set unchanged.
- **21. Independence is structural.** Two changes that write disjoint
  registers are concurrent unless one names the other's ancestor. Every fold
  reads the same whichever the linearisation puts first. This is law 3 given
  teeth: independence is now what the DAG says, not something the folds
  forgive.
- **22. No stored byte moves.** On every `conformance/dag.py` store, and on
  generated stores of `Sealed` and `Woven`, every reading, tip, linearisation
  and `verify` finding is unchanged. A `Change`'s deps are its parents in
  every definition of §3 and §6.
- **Amended 6.** Conflicts are exactly the registers both branches wrote
  DIFFERENT events to. Agreeing twins are one value.
- **Amended 17(b).** In a store of changes, after two writers' files are
  united, the next `append` on the union writes the object a single writer
  holding the union would write. No `Woven` is involved.

## Open questions for the reviewer

1. **Agreeing twins (5).** This is the one change to how a fold reads. The
   alternative is to show twins as a conflict and let the next write clear
   it, which keeps law 6 word for word. Recommendation: take the rule.
   Idempotence across replicas that shows a conflict is only half kept.
2. **`Created` and `Tended` with empty deps.** The alternative is to anchor
   every event on its todo's `Created`, which reads more naturally in a
   stream. That dependency supersedes nothing, though, and it would make a
   pass mirrored before the creation arrived a different object from the same
   pass mirrored after. Recommendation: empty.
3. **Dependencies through terms.** A spec holding `Ref(b)` or `After(b)` reads
   `b`. Should it depend on `b`'s writes? Recommendation: no. Those are
   readings in TIME, resolved by `link` and the environment at a moment (§7),
   not supersession. A spec does not overwrite `b`.
4. **Retiring `merge`.** Keep it for legacy stores, or refuse it on a store
   that holds any `Change`? Recommendation: keep it. A `Woven` in a store of
   changes is harmless, and settling a legacy fork is its one remaining use.

## Cost, stated

`append` today lists tips. It will fold the store's structure to find the
register frontiers, and index event prints for the idempotence check: O(n)
per append with no cache. §6.6's monoid action is what makes a cache honest:
keep the `Folded` for the store and `extend` it with the objects since. At
the live chain's size, the whole register fold was measured at about 1.5 ms
(`core/src/registers.rs`, 2026-09-07). So the uncached append costs that,
once per write. The cache is an optimisation for the implementing session to
measure, not assume.

## Plan for the implementing session

Nothing here is started. In order, each step green before the next:

1. `core/src/event.rs`: `Envelope::Change { deps, event }`, `mk_change` (deps
   sorted, distinct, `event` required), `parents_of`, print and parse, and
   §2's whitelist. Law 1 over it in `core/tests/events.rs` and the fuzzer.
2. `core/src/registers.rs`: `deps_for(state, event)`, the structural frontier
   union, and the agreeing-twins rule in `Frontier::is_conflict` and
   `conflicts_of`. Amended law 6 in `core/tests/registers.rs`.
3. `core/src/store.rs`: `append` writes `Change` and is idempotent (law 20).
   `verify` gains 6's checks. `merge` is unchanged.
4. `core/tests/change.rs`: laws 19–22 as properties over `common`'s
   generators, extended to draw two diverging writers and replays, among them
   the rejection replay of 3. `core/tests/tips.rs` and `fold_laws.rs` for
   amended law 17(b).
5. `conformance/change.py`, written BY HAND from the laws, like `link.py`:
   exact prints and names of a small store of changes, a replay that writes
   nothing, a twin pair, and a mixed store over a `conformance/dag.py` one.
   `Change` joins the vector vocabulary only as a string, as every stored
   object does (§2).
6. `github/`: drop the replay dedup the database now does, keep
   `Known::accepted` (it keys on the comment, not the event), and date a
   claim past its deps instead of the store.
7. `cli/` and `wasm/` read `Woven` today. Teach them `Change` wherever they
   match on envelopes.
8. SPEC: fold Draft A into §3, §6.6, §6.7 and §9 and delete the draft
   section. CHANGELOG: under Unreleased, as a MINOR change.
