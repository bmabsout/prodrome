# Prodrome — the specification

Prodrome is a temporal, content-addressed event database: event objects in a
DAG, folds that turn them into belief at a moment, and FPL, a fuzzy temporal
logic whose terms are a todo's fulfillment as a function of time. This
document is the contract; the crates in this repository are its reference
implementation. §9 lists the laws every implementation must pass, bit for bit
where this text says so and to 1e-9 where it says so, and `conformance/*.py`
holds the vectors they are checked against.

## 1. Principles

- **Types first.** Every value below is a closed algebraic type. Invariants
  live in smart constructors that return an error; a record that exists is
  valid. Names that mean different things are different types even when they
  are all strings: `Hash`, `TodoId`, `Actor`, `Name` do not mix.
- **One evaluator.** Fulfillment is computed in one place, and every consumer
  reads numbers it produced.
- **Order is causal, time is data.** Events are ordered by the DAG. The `at` a
  writer stamped is read by the folds as data and never decides a merge.
- **Decoration, not evaluation.** An explanation or a rendering is an
  annotation of the core's structures, the same shape with a value at each
  node, never a second reading of the semantics.
- **Add kinds, never change shipped fields.** A stored constructor's fields
  are frozen; `""` means absent on shipped string fields; evolution is a new
  constructor.

## 2. Values and the literal grammar

Every stored object is one expression of a small Python-literal grammar,
printed canonically and parsed strictly. Printing is total and deterministic:
equal values print byte-identically.

- `None`, `True`, `False`.
- Integers: decimal, optional leading `-`.
- Floats: Python `repr`, the shortest string that round-trips; integral values
  carry `.0`; exponent form when the decimal exponent is `< -4` or `>= 16`;
  `inf` and `nan` are not storable.
- Strings: Python `repr`: single quotes unless the text contains `'` and no
  `"`; escapes `\\`, the delimiter, `\n`, `\r`, `\t`; other non-printable
  characters as `\xNN`, `\uNNNN`, `\UNNNNNNNN` per `str.isprintable`;
  printable non-ASCII as itself.
- `datetime(Y, M, D, h, m, s)`, with a seventh microsecond argument only when
  non-zero; naive local time.
- `timedelta(days=…, seconds=…, microseconds=…)`, only the non-zero parts, in
  that order; `timedelta()` for zero.
- Tuples: `(a, b)`, `(a,)`, `()`.
- Constructor calls `Name(field=value, …)`: every field, in declared order,
  keyword form. The admitted names are the vocabulary of §4 and §7 plus
  `datetime` and `timedelta`. §4's half is the core's kinds UNION the host
  payload's — its record kind and the constructors that kind's fields nest —
  so the whitelist is a union and is still a whitelist; where the two would
  name the same constructor, the core's wins and the host's is unreachable.

Parsing admits exactly this grammar: no names, operators, comprehensions or
attribute access. A call dispatches to its smart constructor, whose error is
the parse error. Anything else is a refusal, never a crash; the parser is
fuzzed.

**The vector vocabulary.** `conformance/*.py` — the evidence §9 is checked
against — is one expression of THIS grammar, printed by this printer and read
by this parser, against a SECOND whitelist that is disjoint from a store's:
`Folds`, `Dags`, `Fpl`, `Series`, `View`, `Links`, `Recurs`; `FoldCase`,
`DagCase`, `FplCase`, `SeriesCase`, `ViewCase`, `LinkCase`, `RecurCase`;
`Bound`, `Spec`, `Content`, `Object`, `Parents`, `Conflict`, `RowConflict`,
`Refused`, `Sample`, `Knot`, `Asked`, `Row`; and, for an explanation's
decoration, `Node`, `NoteEntry`, `One`, `Many`, `Maps`, `Fields`, `Pair`. A
STORE's parse admits none of these and a VECTOR's parse admits none of §4's
or §7's: two vocabularies, one grammar, one parser, and neither can widen the
other.

Inside a vector, every stored object, event and term is a STRING holding its
canonical print — never a nested literal — because those exact bytes are what
the vector is evidence of (§9.1). Instants are `datetime(...)`; the mappings a
vector needs (by todo id, by object name) are tuples of a named constructor,
since this grammar has tuples and no mapping.

## 3. Objects and the DAG

- An **envelope** is `Sealed(prev, event)`, one parent with `prev == ""` at
  genesis, or `Woven(parents, event)`, two or more parents, sorted and
  distinct, where `event` may be `None`: a merge is structure.
  `parents_of(Sealed("", …))` is `()`.
- An object's **name** is `sha256(utf8(print(envelope)))` in lowercase hex.
  The file `objects/<name>.py` holds exactly that print, and a loader
  re-hashes the bytes before parsing them.
- **Heads.** `HEAD` holds one tip; `refs/` holds one file per head while there
  are several. `tips()` is `{HEAD}` unless `refs/` is non-empty.
- **Linearisation** is Kahn's algorithm over parents with a min-heap on the
  name: deterministic, and arbitrary between incomparable objects, which is
  what registers make visible (§6.6). A missing parent or a cycle is a
  refusal.
- `ancestors(x)` is the transitive parent closure; `concurrent(a, b)` holds
  when neither is an ancestor of the other.
- **`verify`** reports an object not hashing to its name, a missing parent, a
  cycle, an unreachable object, a HEAD inconsistent with the objects, a
  malformed `Woven`, a stale head, and an event the policy does not `confirm`
  (§5) dated before any of its ancestors — a writer whose stamp the host
  forces cannot legitimately be dated behind what it was written on top of,
  where a backfill can.
- **`adopt(source, tip)`** copies verified objects in. A tip already contained
  changes nothing; a tip containing every head fast-forwards; otherwise it
  becomes a second head. The source may be another store or a map of prints
  that arrived over a wire (`adopt_objects`); only where the bytes are read
  from differs. **`merge(parents)`** writes a `Woven` and makes it the single
  tip.

## 4. Events

Kinds and fields, in order; all shipped; `""` means absent.

- `Created(todo, at, actor, text, note)`
- `Completed(todo, at, actor, note)`, `Cancelled(…)`, `Reopened(…)`
- `Tended(todo, at, actor, note)` — care taken on a todo that is never done,
  a pass at a recurring chore. It changes no state, spec or content: §6.1
  folds it into the tendings, and §7.3's `Recur` reads them.
- `SpecRevised(todo, at, actor, spec, note)`
- **The record kind**: `KIND(todo, at, actor, <the host's fields>)`.

The six kinds above are the DATABASE's: their fields are its semantics, and
they are frozen here. A record is a todo's CONTENT, and content is a
deployment's — so a host **payload** supplies

- `KIND`, the constructor name, and the record's fields and their declared
  order, which together are the stored bytes and are frozen the same way;
- the VOCABULARY of the constructors those fields nest, which is §2's
  whitelist's other half;
- the parse and the print of those fields, the parse refusing what a smart
  constructor refuses;
- and the two readings §6 takes from a record and the only two: the spec it
  carries (§6.2, §6.4) and its checklist length (§6.4).

Nothing else about a record is read anywhere in this document. The REFERENCE
PAYLOAD — the one `conformance/*.py` was taken with, and the one every
`Authored(...)` in those vectors round-trips under — is `Authored(todo, at,
actor, kind, created, body, spec, rationale, category, waiting_on, detail,
source, subtodos, notes, note)`: `body` and `detail` are `MarkupSource`,
`waiting_on` is `StringSource`, `rationale` a tuple of strings, `source` a
`Source(sender, subject, date, hash, thread_id, message_id)` or `None`,
`subtodos` a tuple of `SubTodo(body, done)`, `notes` a tuple of `Note(on,
lines)` with `on` a site literal. `MarkupSource` and `StringSource` are opaque
text in that host's two rendering languages; the database stores and prints
them and never interprets them.

## 5. Standing

The database does not decide whom to believe. It asks the HOST, about one
event at a time, and the answer is a `Standing`:

- **`Binds`** — the folds take it. It writes its registers and it is what the
  store believes.
- **`Claims`** — the folds refuse it. It is stored and it is SHOWN, beside the
  answer that stands, and it changes no confirmed reading.

A **policy** is that function: `standing(event) : Standing`, of the EVENT and
nothing else — not of the log, not of the position, not of the moment. Every
fold in §6 takes one, §6.7 takes one, and §3's `verify` takes one; nothing in
the database reads an actor name to decide anything.

A policy answers one further question, whose default is the reading above:
`confirms(event)`, "is this the host's own word", which is `standing(event) ==
Binds` unless the policy says otherwise. Two readers ask it and neither is a
fold — §6.7's `confidence` field, and §3's clock rule — and a policy that folds a
writer's events while still marking them as that writer's is why it is asked
separately. `Claims` implies not `confirms`.

**The reference policy** — the one `conformance/*.py`'s `untrusted` fields
name, and the one this repository's vectors were taken under — is a set of
actor names. An event `Claims` when its actor is on the set and it is a
lifecycle, `Tended` or `SpecRevised` event; everything else `Binds`, a
content record included, because writing content is what such a writer is for
and a fold that hid its writes would be an outage that reports success. It
`confirms` an event exactly when the actor is not on the set — so a content
record from a named actor binds and is still shown as that actor's. The empty
set is the policy under which everything binds and nothing is a claim.

**Both readings stay in the database** (§6.7): the CONFIRMED one, under the
host's policy, and the CLAIMED one, under the policy where everything binds. A
store that kept only the filtered reading could not show a claim at all, and
showing one is the whole point of storing it.

## 6. Folds

Every fold takes the linearised events, a moment `t` and a §5 POLICY, and reads
the events with `at <= t` in causal order.

1. `env_at(events, t, policy)`, the environment, in two halves. `outcomes :
   TodoId → Completed(at) | Cancelled(at)`: the last binding write wins and
   `Reopened` clears. `tended : TodoId → set of instants`: the instant of
   every binding `Tended`, a grow-only set folded by union, so nothing
   removes a tending and no order among them matters. Only events the policy
   says `Binds` write either half.
2. `specs_at`: the latest spec per todo, from a record's payload (whoever
   wrote it) or a `SpecRevised` the policy binds.
3. `authored_at`: the latest record per todo, whoever wrote it — no policy
   parameter: content renders, and §6.7 marks the row instead.
4. `flatten(events, t, policy) : TodoId → Term`: one function per todo. Its
   head is `checklist(first spec ever, first checklist length ever)`; at every
   moment a spec, a checklist length or the bound state changed there is a
   `Piece(at, term)`: a flat 1.0 while resolved, else `checklist(spec in
   force, items in force)`; assembled by `mk_piecewise`. "Spec" and "items"
   are the payload's two readings (§4) and the whole of what a record
   contributes. A `Tended` puts no piece: it is care and not a transition,
   and the terms that read tendings read them from the environment (§7.3),
   so no stored function changes meaning. The head extends to
   −∞: a todo's function is total over time, and before anything was recorded
   about a half its unit is the earliest recorded demand of that half. A
   completion recorded before its record therefore stands against the demand
   the record says it had. Consequence: the prefix law (§9.2) is exact for
   every moment after a todo's first spec and first checklist are recorded; a
   late first half re-heads the curve before it. Absent when there was never
   a spec or a checklist. `checklist(own, n)` is `own` when `n == 0`;
   `Conj(n × Flat(0.5))` when `own` is `None`; else `OffsetBy(own, Conj(…))`.
5. `history(events, policy)`: the environment as a function of time; per
   todo, the sequence of `(at, binding | None)`, and every binding tending,
   a grow-only set being its own history; `history.at(t)` equals
   `env_at(events, t)`, the tendings dated at or before `t` included.
6. **Registers** over nodes `(name, parents, event)`: one register per `(kind
   ∈ {state, spec, content}, todo)` holds its frontier, the writes no later
   write descends from. One write is a value; more is a conflict. A `Tended`
   writes no register: it joins the tendings, a grow-only set kept beside the
   frontiers, so concurrent tendings are their union and never a conflict.
   `extend(state, nodes)` is a monoid action. Projections pick the write
   latest in the linearisation, so on any DAG `env_of` (the tendings with
   it), `specs_of` and `content_of` equal folds 1–3, and `conflicts_of` names
   the rest.
7. **The entry.** `entries(nodes, t, policy) : [Entry]`, one row per todo
   any event mentions, ordered by id. It is the composition of the folds and
   §7, stated once so every consumer performs it once. With `confirmed =
   fold(nodes, t, policy)`: `outcome = env_of(confirmed)[todo]`; `claim =
   env_at(events, t, everything-binds)[todo]` where it names a different
   outcome than
   `outcome`, absent where they agree; `spec = flatten(…)[todo]` and `value`
   the fulfillment at `t` of `link(spec, flatten(…))` (§7.2) under
   `confirmed`'s environment, absent together — and where `spec` does not
   link, `value` is the `LinkError` instead of a number;
   `content = chosen_of(confirmed, content)[todo]`, the name of the winning
   object and never the record — a consumer that wants the payload looks the
   object up, because what a record MEANS is the host's; `conflicts = conflicts_of(confirmed)[todo]`;
   `stream` is every node whose event names the todo, in causal order. A todo
   whose events are all dated after `t` is still a row, open and unpriced:
   `t` asks what is believed, not what exists. `confidence` says whether the
   answer is the confirmed reading whole: a claim refused, a winning content
   record the policy does not `confirm`, or both. Checked against `conformance/view/*.py` — SEEDED, not
   taken from the reference like the rest of `conformance/`: random logs
   drawn from this crate's own generator (`core/tests/common/mod.rs`'s
   `a_log`, the one `core/tests/fold_laws.rs`'s properties draw from too)
   under a fixed seed, folded at several instants under two reference
   policies. Their `untrusted` fields are read through the reference policy;
   the stored bytes and every expected answer are unchanged.
   `core/examples/generate_view_vectors.rs` regenerates them by hand; nothing
   under `cargo test` or CI ever does.

## 7. FPL

`Term` is the fixed point of `TermF a`:

```
Flat(value) | Decay(start, end, end_date, lead_up, start_date?) | Curve(points)
| Conj(terms, p) | Offset(delta, a) | Gate(gate, body) | Shift(delta, a)
| Within(window, p, a) | Importance(w, a) | After(event, anchor, term, pending, needs?)
| Recur(todo, anchor, term, pending) | Periodic(period, anchor, term)
| Piecewise(head, pieces) | OffsetBy(delta, term) | Ref(todo)
```

Semantics `⟦t⟧(now, env) ∈ [0, 1]`:

- `Flat`: `value`. `Decay`: 1.0 before `start_date`; 0.98 before `end_date −
  lead_up`; linear from `start` to `end` across the window; `end` after.
  `Curve`: linear between points, clamped outside.
- `Conj`: the power mean of the members' values with exponent `p`, each
  clamped to at least 0.001; `p = 0` is the geometric mean; empty is 0.5.
  `Offset`: `x(1 − |δ|) + max(0, δ)`. `Gate`: `max(1 − ⟦gate⟧, ⟦body⟧)`.
  `Shift`: `⟦a⟧(now + δ)`. `Within`: the power mean of 65 samples over
  `[now, now + window]`. `Importance`: `⟦a⟧^w`.
- `After`: unbound, `⟦pending⟧(now)`; `Completed(done)`, `⟦term⟧(now − (done
  − anchor))`; `Cancelled`, 1.0. A binding counts only when `done <= now`.
- `Recur`: with `s = last_tended(env, todo, now)`, the latest tending of
  `todo` at or before `now`: none, `⟦pending⟧(now)`; else `⟦term⟧(now − (s −
  anchor))` — `After`'s slide, from the last pass (§7.3).
- `Periodic`: `⟦term⟧(anchor + ((now − anchor) mod period))`, the remainder
  Euclidean (non-negative), in microseconds.
- `Piecewise`: the piece in force, the last with `at <= now`, else the head.
  `OffsetBy`: `offset(⟦term⟧, ⟦delta⟧)`.
- `Ref(todo)`: no reading of its own. It is a variable, bound by `link`
  (§7.2); the semantics above are defined on CLOSED terms only.
- **Normal form** (`mk_piecewise`): no pieces means the head; nested pieces
  are spliced; no adjacent equal pieces; instants strictly increasing.
  `normalize` pushes `Conj`, `Offset`, `Gate`, `Importance` and `OffsetBy`
  under `Piecewise` over the merged partition, translates instants under
  `Shift`, and leaves `Within`, `After`, `Recur` and `Periodic` in place: a
  window, a lookup and a fold of time onto one cycle do not commute with a
  partition.
- **Explain** is a decoration, `Cofree TermF (value, notes)`: the term's shape
  with each node's fulfillment at the moment its parent used it, plus notes
  (`Conj`: certifies, shares; `Within`: peak instant and share; `After`:
  bound; `Recur`: bound — `pending` or `tended` — and, when tended, the last
  tending and the hours since it; `Periodic`: the start of the cycle in
  force; `Piecewise`: since, pieces). A serialization of one is the term's own
  shape carrying those two at every node.
- **Breakpoints**: the slope changes and jumps of the exact fragment (`Flat`,
  `Decay`, `Curve`, `Piecewise` of exact parts, `Offset`, `Shift`, constant
  composites). A series with a knot at each and a second knot before each jump
  is the curve; other terms are sampled and say so.
- A term has ONE serialization and it is §2's literal print. A JavaScript
  boundary may carry a JSON shape of the same term — the reference one is
  `prodrome-wasm`'s `json` module, lowercase kind tags and spans in hours —
  but that is a boundary's business and not the database's, exactly as a
  rendering is (§1).

### 7.1 The chain compiler

`After` and `Recur` (§7.3) are the terms that read history. Every other
constructor is a function of `now` and its subterms, so a term with neither
anywhere in it can be evaluated against no environment at all. The compiler
is that erasure: given a term and the environment as of one instant, it
answers a term with the same reading and no `After` or `Recur` left.

```
compile(term, env) : Compiled
compile_chain(functions, env) : (todo → Compiled) | ChainError::Cycle(path)
```

`Compiled` is the term the compiler produced together with the LINKS it
resolved. Its readers — `term`, `fulfillment(now)`, `explain(now)`, `notes` —
take no environment, and that is the whole claim: a compiled term cannot
consult one, because the constructors that would have been compiled away.

**Per link.** With `After(e, anchor, term, pending, needs)` and what the
snapshot says about `e`:

- ABSENT — `compile(pending)`. `bound` answers `None` at every instant, so the
  link is its pending branch and nothing else.
- `Completed(τ)` — `Piecewise(compile(pending), [(τ, Shift(anchor − τ,
  compile(term)))])`. Before τ the link is unbound; from τ the body slides by
  the slippage, and sliding `now` by a constant IS a `Shift`.
- `Cancelled(τ)` — `Piecewise(compile(pending), [(τ, Offset(1.0,
  compile(term)))])`. `x·(1 − |1|) + max(0, 1) = 1` for every `x`: the moot
  constant is a graded offset at δ = 1, and writing it as one keeps the mooted
  demand in the tree where a reader can still see what was dropped.

So each link contributes ONE graded offset δ ∈ [0, 1] — its MOOT GRADE, 1
where the upstream was cancelled and 0 everywhere else — applied with §7's
corrected form; δ = 0 is elided because `offset(x, 0) = x`, and so is a zero
`Shift`. The result is `normalize`d, so a chain (A needs B needs C) is ONE
schedule at the root over the merged partition of the links' instants, and not
three freeze quantifiers the evaluator re-enters at every sample.

**Per recurrence.** `Recur(todo, anchor, term, pending)` with the snapshot's
tendings `s₁ < … < sₙ` of `todo` compiles to `Piecewise(compile(pending),
[(sᵢ, Shift(anchor − sᵢ, compile(term)))])`: `Completed(τ)`'s case once per
pass, exact because `last_tended` picks the latest tending at or before `now`
and a piece at `sᵢ` is in force from `sᵢ` until the next. A recurrence is not
a link: it contributes no note and no graph edge.

**`needs` is the COMPILER's field, not the evaluator's.** §7's semantics have
never read it and this does not change that: consuming it as a VALUE would
move a reading, and the law below forbids that. It is the link's declared LEAD
TIME, and the compiler is the first reader holding it beside the upstream's
actual instant — so a resolved link reports `ready = τ + needs`, the earliest
moment the link's own demand could be met, in its note. A host schedules by
it. The evaluator still does not read it.

**Cancellation is reported, never silent.** Every link becomes a note on the
compiled term — `pending`; `completed` with its slippage and its `ready`;
`moot` with its instant — and `explain` puts the moot ones at the root under
`moot`. §7's "a Cancelled upstream prices as moot and MUST be surfaced by
reporting" is a requirement on the REPORTING, and compiling is where it is
cheapest to honour: a 1.0 from a cancellation is indistinguishable from a 1.0
from a demand met, and the note is the only thing that tells them apart.

**Cycles are a value.** A single term is a tree and cannot cycle.
`compile_chain` is handed a MAP of todo to function whose links draw a graph
over those todos, and a cycle in that graph is a modelling error the compiler
is the first reader to see whole. It is refused as `ChainError::Cycle(path)`,
carrying the path so the host can name the loop, and never as a partial answer.

**This is an OPTIMISATION and not a semantics change.** Its law is §9.13,
`fulfillment(compile(t, env).term, now, ∅) == fulfillment(t, now, env)`. No
stored byte moves, no vector moves, and a host that never compiles reads
exactly what it read before.

**The cost.** The interpreter consults the environment once per `After` node
per evaluation, and `Within` is 65 evaluations of its subterm — so one link
under a window costs 65 lookups, and a chain of `d` links under one costs
65·d, at every query. The compiler pays one walk of the term (the resolution,
then `normalize`'s merge of the partition) ONCE, and every evaluation
afterwards pays none, because there is no `After` left to pay for.

WHAT THAT IS AND IS NOT WORTH, measured rather than assumed: on a chain of
three links under one window, 2000 evaluations, the lookups go from 390 000 to
0 for a 19 µs compile — and the WALL CLOCK moves about 7% in a release build,
because a lookup in a three-entry map is cheap. The saving being claimed is not
the microseconds. It is that the environment leaves the query path entirely: a
compiled term is a term (§7), so it can be cached, stored, shipped over a wire
and evaluated somewhere that holds no history at all, and the chain it came
from is one schedule a reader can see rather than a nest to re-enter.

### 7.2 References and linking

`Ref(todo)` is "the fulfillment of todo `todo`": a subtodo's parent, a group,
any todo whose demand is composed from other todos'. `todo` obeys `TodoId`'s
rule and the constructor refuses anything else. It is a LEAF of `TermF` and a
VARIABLE: a term holding one is OPEN, and evaluation — `fulfillment`,
`explain`, series knots, the compiler — is defined on CLOSED terms only, those
with no `Ref` anywhere. `Closed` is that type, so evaluating an open term is
a type error and not a wrong number.

```
link(term, specs : todo → Term) : Closed | LinkError
```

`link` substitutes every `Ref(x)` with `specs[x]`, linked in turn: bind for
the free monad over `TermF`, with todo ids as its variables. A rebuilt
`Piecewise` goes back through `mk_piecewise`, so a spec that is a schedule,
landing in a piece, is spliced like any nested schedule. A closed term is its
own link, untouched. `specs` is each todo's OWN function; for a store it is
`flatten`'s (§6.4), so `Ref(x)` means x's whole function — its revisions and
its lifecycle — and a completed child reads 1.0 from its completion.

**Refusals are values.** `LinkError::Unknown(x)` where `specs` holds no `x`.
`LinkError::Cycle(path)` where the references loop: the substitution keeps the
path of references it is expanding, and meeting one already on it is the
cycle, reported with that path, its first todo repeated at the end. So `link`
is total and never loops. A cycle is refused where it is REACHED: specs that
loop elsewhere do not stop a term that never names them from linking.

No stored byte moves: `Ref` is a new constructor (§1), and a store without one
reads exactly what it read before.

### 7.3 Recurrence

A recurring todo is never done: vinegar the toothbrush every two months, pay
the rent on the first. Recording a pass as `Completed` would close it, so a
pass is a `Tended` (§4), which leaves the todo's state alone and joins the
environment's TENDINGS — per todo, a grow-only set of instants (§6.1). Two
replicas' tendings merge by union and never conflict (§6.6).

```
last_tended(env, todo, now) : Instant | None
```

is the latest tending of `todo` at or before `now`, under `bound`'s guard: a
pass recorded later never rewrites an earlier moment. Two terms read time
cyclically:

- `Recur(todo, anchor, term, pending)` repeats from the LAST PASS. `term` is
  authored against `anchor` and re-anchored to the last tending exactly as
  `After` re-anchors to a completion: a pass at `s` reads `term` at `now − (s
  − anchor)`, and no pass yet reads `pending`. `todo` obeys `TodoId`'s rule.
  The toothbrush is

  ```
  Recur(todo='toothbrushvinegar', anchor=datetime(2026, 8, 12, 0, 0, 0),
        term=Decay(start=0.98, end=0.3, end_date=datetime(2026, 10, 11, 0, 0, 0),
                   lead_up=timedelta(days=60), start_date=None),
        pending=Flat(value=0.3))
  ```

  — a decay from 0.98 on the day of a pass to 0.3 sixty days on, restarted by
  every pass, and 0.3 while it was never done.
- `Periodic(period, anchor, term)` repeats ON THE CALENDAR, whatever is done:
  `term` on `[anchor, anchor + period)`, and every instant, before `anchor`
  too, folded onto that cycle by a Euclidean remainder. `period` is positive.
  Rent and quarterly taxes are this.

Neither stores a curve and a `Tended` creates no piece, so `flatten` (§6.4)
gains no case and no stored object changes meaning: this is evolution by
adding kinds (§1). Both are temporal, so `normalize` leaves them in place;
`Recur` reads history, so the compiler resolves it (§7.1); neither is in the
exact fragment of the breakpoints, so a series over one is sampled.

A caution on reading the future, which `After` shares: a pass BEFORE the
anchor reads `term` later than `now`, so a `term` that itself reads history —
a nested `Recur` or `After` — reads it there. `Recur`'s own lookup never
looks past `now`; its body is a function of time like any other.

No stored byte moves: `Tended`, `Recur` and `Periodic` are new constructors
(§1), and a store without them reads exactly what it read before.

## 8. Types an implementation must have

`Hash` (64 hex), `TodoId`, `Actor`, `Name`; `Payload`, §4's record kind as a
parameter — a constructor name, a field order, a vocabulary, a parse, a print,
`spec` and a checklist length — carried by value and never as an existential,
since one store holds one record shape; `Standing = Binds | Claims` and
`Policy`, §5's standing as a parameter — one function of an event, carried by
value like the payload and for the same reason, since one store reads under one
policy; `Envelope = Sealed | Woven`;
`TodoEvent`; `Term` as `Fix TermF`; `Closed` (§7.2), a term with no `Ref`,
which is what every evaluator takes, and `LinkError = Unknown | Cycle`;
`Env`, the environment, outcomes beside the grow-only tendings (§6.1, §7.3);
`Explanation = Cofree TermF Annotation`; `Compiled` (§7.1), a term with every
`After` and `Recur` resolved beside the `Link`s it resolved, whose readers
take no environment because a compiled term cannot consult one, and
`ChainError`, whose
`Cycle` carries the path; `Frontier`, a non-empty ordered set
of writes; `Folded`; `Breaks`; `Entry` (§6.7), whose price and function are
absent together and whose `Confidence` is a sum with no "provisional for no
reason" inhabitant. Smart constructors validate; records are data; engine
code never calls a raw constructor.

## 9. Laws

Against `conformance/*.py` and on generated inputs:

1. `print ∘ parse` is the identity on every stored object, and `hash(print)`
   is its name. Records included: every `KIND(...)` print in the vectors
   round-trips byte for byte under the payload they were taken with.
2. **Prefix.** Appending a later event changes no earlier moment's reading,
   except the head's unit (§6.4): a todo's first spec or first checklist,
   recorded late, re-heads its curve before it. With both halves recorded,
   exact.
3. Independent events commute; a uniform shift of every `at` changes no
   winner.
4. `history.at(t) == env_at(t)`.
5. `mk_piecewise` is a normal form: unit, join, idempotent, no adjacent
   repeats. `normalize` preserves every reading, leaves nothing buried, and is
   idempotent.
6. Registers equal the folds on any DAG; conflicts are exactly the registers
   both branches wrote; a merge settles nothing; a descending write settles.
7. Interpolation between series knots equals evaluation on the exact
   fragment.
8. Fulfillment within 1e-9 of the reference on every vector; hashes, prints,
   linearisations, frontiers and `verify` findings exactly.
9. **The view is the composition** (§6.7): on any DAG and at any moment,
   every field of every entry equals the fold it is named by, and the rows
   are exactly the todos the events mention. Against `conformance/view/*.py`
   (seeded, §6.7) on linear chains and against `core/tests/fold_laws.rs`'s
   property on random DAGs.

Laws 10–12 QUANTIFY OVER THE POLICY (§5). They are what makes "the host
decides" honest: the database is a two-reading fold with a parameter, and these
say the parameter cannot do anything but select.

10. **The two readings are one under the policy that binds everything.** With
    every event binding, the claimed reading and the confirmed reading are
    equal at every moment: every fold answers the same, no entry carries a
    claim, and no entry is provisional. The empty reference roster is that
    policy and answers alike.
11. **A claiming event never moves the confirmed reading.** Append an event the
    policy only lets `Claim`, at any instant and about any todo, and every
    confirmed answer at every moment is the answer it was — the environment,
    the specs, the content, the functions, the whole history. It is stored and
    it is shown; it is not folded.
12. **Standing selects events, not positions.** Because `standing` is a
    function of the event alone, folding under a policy equals folding the
    sub-log of the events it binds under the policy that binds everything —
    and that stays true under any permutation of the log, since filtering
    commutes with reordering. Which events count is a function of the event
    SET; law 3 is the other half, that only which of them WINS is a function
    of the order.

Law 13 quantifies over the ENVIRONMENT, and it is what makes §7.1's compiler an
optimisation rather than a second evaluator.

13. **Compilation preserves every reading** (§7.1). For every term and every
    environment, `fulfillment(compile(t, env).term, now, ∅)` equals
    `fulfillment(t, now, env)` to 1e-9 at every instant — the compiled side
    read against the EMPTY environment, which is the statement's teeth: it
    answers the same while unable to look anything up, because `After` and
    `Recur` are the only constructors that read the environment and compiling
    leaves neither. On `core/tests/fpl_laws.rs`'s own generators, the
    `a_term()` and `an_env()` §9.5 is checked over, tendings included; and
    pinned by every §9.8 vector, which the compiler does not touch, and every
    `conformance/recur.py` sample, which it reads the same.

Law 14 quantifies over the SPECS, and it is what makes §7.2's `link` a
substitution and nothing more.

14. **Linking is bind** (§7.2). A closed term links to itself against any
    specs. `fulfillment(link(Ref(x), specs), now)` equals
    `fulfillment(link(specs[x], specs), now)` at every instant. Linking
    against specs that still hold references reads the same as linking the
    specs first and the term against the closed ones. The linked term is in
    `mk_piecewise`'s normal form. A loop is refused with a path that closes
    on itself, every step of it a reference its spec holds, and is never
    evaluated. `Ref` round-trips through print and parse like every other
    constructor (law 1). On `core/tests/fpl_laws.rs`'s generators, over
    acyclic specs; and against `conformance/link.py`, written BY HAND from
    these laws — exact linked prints, readings to 1e-9, an unknown todo and a
    cycle refused.

Laws 15 and 16 are §7.3's: care is a set, and recurrence reads it as of now.

15. **A tending is care, not state** (§4, §6). A `Tended` changes no
    outcome, spec, content record, function or register at any moment, and
    every entry keeps its state, its function and its content — an open todo
    stays open. The tendings are a grow-only set: `history.at(t)` holds the
    ones dated at or before `t` (law 4), the registers' tendings are the
    folds' on any DAG (law 6), a merge of two histories folds to the union of
    their tendings, and a tending writes no register, so never a conflict.
    `last_tended(env, x, now)` is the latest tending at or before `now`, and
    a tending dated later changes nothing it answers. On
    `core/tests/fold_laws.rs`'s generators, which draw `Tended` among the
    other kinds, and `core/tests/fpl_laws.rs`'s.
16. **Recurrence reads time as §7.3 says.** With the last tending of `x` at
    `s`, `Recur(x, anchor, term, pending)` at `s + d` equals `term` at
    `anchor + d`; with no tending at or before `now` it equals `pending` at
    `now`; and a tending of `x` dated after `now` changes nothing its own
    lookup reads at `now`. `Periodic(period, anchor, term)` at `now + period`
    equals itself at `now`, and on `[anchor, anchor + period)` equals `term`.
    Both round-trip through print and parse (law 1), and the smart
    constructors refuse a `Recur` whose `todo` is not a `TodoId` and a
    `Periodic` whose `period` is not positive. On `core/tests/fpl_laws.rs`'s
    generators; and against `conformance/recur.py`, written BY HAND from
    these laws — `Tended` prints folded under the reference policy, a claimed
    pass among them, and exact term prints with readings to 1e-9.

## 10. Non-goals

A clock in the merge; a second evaluator; a rendering as a source of truth;
multi-tenancy; a hosted service.
