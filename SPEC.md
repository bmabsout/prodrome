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
`Folds`, `Dags`, `Fpl`, `Series`, `View`; `FoldCase`, `DagCase`, `FplCase`,
`SeriesCase`, `ViewCase`; `Bound`, `Spec`, `Content`, `Object`, `Parents`,
`Conflict`, `RowConflict`, `Sample`, `Knot`, `Asked`, `Row`; and, for an
explanation's decoration, `Node`, `NoteEntry`, `One`, `Many`, `Maps`,
`Fields`, `Pair`. A STORE's parse admits none of these and a VECTOR's parse
admits none of §4's or §7's: two vocabularies, one grammar, one parser, and
neither can widen the other.

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
- `SpecRevised(todo, at, actor, spec, note)`
- **The record kind**: `KIND(todo, at, actor, <the host's fields>)`.

The five kinds above are the DATABASE's: their fields are its semantics, and
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
lifecycle or `SpecRevised` event; everything else `Binds`, a content record
included, because writing content is what such a writer is for and a fold that
hid its writes would be an outage that reports success. It `confirms` an event
exactly when the actor is not on the set — so a content record from a named
actor binds and is still shown as that actor's. The empty set is the policy
under which everything binds and nothing is a claim.

**Both readings stay in the database** (§6.7): the CONFIRMED one, under the
host's policy, and the CLAIMED one, under the policy where everything binds. A
store that kept only the filtered reading could not show a claim at all, and
showing one is the whole point of storing it.

## 6. Folds

Every fold takes the linearised events, a moment `t` and a §5 POLICY, and reads
the events with `at <= t` in causal order.

1. `env_at(events, t, policy) : TodoId → Completed(at) | Cancelled(at)`.
   The last binding write wins; `Reopened` clears; only events the policy
   says `Binds` write.
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
   contributes. The head extends to
   −∞: a todo's function is total over time, and before anything was recorded
   about a half its unit is the earliest recorded demand of that half. A
   completion recorded before its record therefore stands against the demand
   the record says it had. Consequence: the prefix law (§9.2) is exact for
   every moment after a todo's first spec and first checklist are recorded; a
   late first half re-heads the curve before it. Absent when there was never
   a spec or a checklist. `checklist(own, n)` is `own` when `n == 0`;
   `Conj(n × Flat(0.5))` when `own` is `None`; else `OffsetBy(own, Conj(…))`.
5. `history(events, policy)`: the environment as a function of time; per
   todo, the sequence of `(at, binding | None)`; `history.at(t)` equals
   `env_at(events, t)`.
6. **Registers** over nodes `(name, parents, event)`: one register per `(kind
   ∈ {state, spec, content}, todo)` holds its frontier, the writes no later
   write descends from. One write is a value; more is a conflict.
   `extend(state, nodes)` is a monoid action. Projections pick the write
   latest in the linearisation, so on any DAG `env_of`, `specs_of` and
   `content_of` equal folds 1–3, and `conflicts_of` names the rest.
7. **The entry.** `entries(nodes, t, policy) : [Entry]`, one row per todo
   any event mentions, ordered by id. It is the composition of the folds and
   §7, stated once so every consumer performs it once. With `confirmed =
   fold(nodes, t, policy)`: `outcome = env_of(confirmed)[todo]`; `claim =
   env_at(events, t, everything-binds)[todo]` where it names a different
   outcome than
   `outcome`, absent where they agree; `spec = flatten(…)[todo]` and `value`
   its fulfillment at `t` under `confirmed`'s environment, absent together;
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
| Piecewise(head, pieces) | OffsetBy(delta, term)
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
- `Piecewise`: the piece in force, the last with `at <= now`, else the head.
  `OffsetBy`: `offset(⟦term⟧, ⟦delta⟧)`.
- **Normal form** (`mk_piecewise`): no pieces means the head; nested pieces
  are spliced; no adjacent equal pieces; instants strictly increasing.
  `normalize` pushes `Conj`, `Offset`, `Gate`, `Importance` and `OffsetBy`
  under `Piecewise` over the merged partition, translates instants under
  `Shift`, and leaves `Within` and `After` in place.
- **Explain** is a decoration, `Cofree TermF (value, notes)`: the term's shape
  with each node's fulfillment at the moment its parent used it, plus notes
  (`Conj`: certifies, shares; `Within`: peak instant and share; `After`:
  bound; `Piecewise`: since, pieces). A serialization of one is the term's own
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

## 8. Types an implementation must have

`Hash` (64 hex), `TodoId`, `Actor`, `Name`; `Payload`, §4's record kind as a
parameter — a constructor name, a field order, a vocabulary, a parse, a print,
`spec` and a checklist length — carried by value and never as an existential,
since one store holds one record shape; `Standing = Binds | Claims` and
`Policy`, §5's standing as a parameter — one function of an event, carried by
value like the payload and for the same reason, since one store reads under one
policy; `Envelope = Sealed | Woven`;
`TodoEvent`; `Term` as `Fix TermF`;
`Explanation = Cofree TermF Annotation`; `Frontier`, a non-empty ordered set
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

## 10. Non-goals

A clock in the merge; a second evaluator; a rendering as a source of truth;
multi-tenancy; a hosted service.
