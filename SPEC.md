# Prodrome — the specification

Prodrome is the temporal-logic database under Suzatary: content-addressed
event objects in a DAG, folds that turn them into belief at a moment, and
FPL, a fuzzy temporal logic whose terms are a todo's fulfillment as a
function of time. This document is the contract an implementation meets. The
Rust crate in this directory is THE implementation as of 2026-09-06; the
Python package `suzatary/prodrome` + `suzatary/fpl.py` was the reference and
is now records, smart constructors and a printer that call it.
`prodrome/conformance/*.json` are vectors generated from that reference while
it was one, and the laws in §9 are what any implementation must pass, bit for
bit where the spec says so and to 1e-9 where it says so. Bassel, 2026-09-06: "build it
in rust", "keep in mind our type driven development", and "the typst docs
are a decoration (à la annotating using Cofree)".

## 1. Principles

- **Types first.** Every value below is a closed algebraic type. Invariants
  live in smart constructors that return an error, never in checks scattered
  at use sites; a record that exists is valid. Names that mean different
  things are different types even when they are all strings: an object
  `Hash`, a `TodoId`, an `Actor`, `MarkupSource` (typst markup), `StringSource`
  (the inside of a typst string) do not mix.
- **One evaluator.** Fulfillment is computed in exactly one place. Every
  consumer (document, web, briefing, phone) reads numbers the core produced.
- **Order is causal, time is data.** No clock decides who wins. Events are
  ordered by the DAG; `at` is an instant the writer stamped, read only as data.
- **Decoration, not evaluation.** The explain tree, the rendered document and
  the web pages are ANNOTATIONS of the core's structures — a `Cofree`-shaped
  tree that carries the same shape plus a value at each node — never a second
  reading of the semantics. Typst is a printer of a decorated tree.
- **Add kinds, never change shipped fields.** A stored constructor's fields are
  frozen forever; `""` means absent on shipped string fields; evolution is a
  new constructor.

## 2. Values and the literal grammar

Every stored object is one expression in a tiny Python-literal grammar,
printed canonically, parsed strictly. Printing is total and deterministic:
equal values print byte-identically, and the print is the identity (§3).

Grammar (expression):
- `None`, `True`, `False`
- integers: decimal, optional leading `-`
- floats: Python `repr` — the shortest string that round-trips; integral
  values carry `.0` (`1.0`, `-0.0`); exponent form `1e-05` / `1e+16` when the
  decimal exponent is `< -4` or `>= 16`; `inf`/`nan` are NOT storable.
- strings: Python `repr` — single quotes unless the text contains `'` and no
  `"` (then double quotes); escapes `\\`, the delimiter, `\n`, `\r`, `\t`;
  other non-printable characters as `\xNN`, `\uNNNN`, `\UNNNNNNNN` per
  Python's `str.isprintable`; printable non-ASCII is written as itself.
- `datetime(Y, M, D, h, m, s)` with a seventh `µs` argument only when non-zero;
  naive local time only.
- `timedelta(days=…, seconds=…, microseconds=…)` — only the non-zero parts,
  in that order; `timedelta()` for zero.
- tuples: `(a, b)`, one element as `(a,)`, empty as `()`.
- constructor calls: `Name(field=value, ...)` — EVERY field, in declared
  order, keyword form. The names admitted are the closed vocabulary of §4
  and §7 plus `datetime`/`timedelta`.

Parsing admits exactly this grammar and nothing else (no names, operators,
comprehensions, attribute access); a call dispatches to the smart constructor,
whose error is the parse error. Anything else is a refusal (`ValueError`-shaped),
never a crash: parsing is fuzzed (tests/test_fuzz.py).

## 3. Objects, hashing, the DAG

- An **envelope** is `Sealed(prev, event)` (one parent; `prev == ""` at genesis)
  or `Woven(parents, event)` (two or more parents, sorted, distinct; `event`
  may be `None` — a merge is structure). `parents_of(Sealed(""))` is `()`.
- An object's **name** is `sha256(utf8(print(envelope)))`, lowercase hex; the
  file is `objects/<name>.py` holding exactly that print. A loader re-hashes
  the bytes before parsing: tamper-evidence on read.
- **Heads**: `HEAD` holds one tip; `refs/` holds one file per head when there
  are several. `tips()` is `{HEAD}` unless `refs/` is non-empty.
- **Linearisation**: Kahn's algorithm over parents with a min-heap on the
  name — deterministic, and arbitrary between incomparable objects (which is
  what registers make visible, §6.6). Missing parent or cycle: refusal.
- **`ancestors(x)`**, **`concurrent(a,b)`** = neither is an ancestor of the other.
- **`verify`** reports: an object not hashing to its name; a parent that does
  not exist; a cycle; an unreachable object; HEAD/objects inconsistency; a
  malformed `Woven`; a stale head; an UNTRUSTED actor's event dated behind any
  of its parents (§5).
- **`adopt(source, tip)`**: copy verified objects in; a contained tip changes
  nothing; a tip containing every head fast-forwards; else a second head.
  **`merge(parents)`** writes a `Woven` and makes it the single tip.

## 4. Events

Kinds and fields, in order (all shipped; `""` absent):
- `Created(todo, at, actor, text, note)`
- `Completed(todo, at, actor, note)`, `Cancelled(...)`, `Reopened(...)`
- `SpecRevised(todo, at, actor, spec, note)`
- `Authored(todo, at, actor, kind, created, body, spec, rationale, category,
  waiting_on, detail, source, subtodos, notes, note)` where `body`/`detail` are
  `MarkupSource`, `waiting_on` is `StringSource`, `rationale` a tuple of
  strings, `source` a `Source(sender, subject, date, hash, thread_id,
  message_id) | None`, `subtodos` a tuple of `SubTodo(body, done)`, `notes` a
  tuple of `Note(on, lines)` with `on` a site literal.
The literal vocabulary is exactly these plus §7's terms and `datetime`,
`timedelta` (tests/test_contracts.py pins the set).

## 5. Trust

`binds(event, untrusted)`: an event changes belief iff it is an `Authored`
record (content, from any actor; its spec rides with it) or its actor is not
in `untrusted`. Untrusted lifecycle and `SpecRevised` events are provisional:
stored, shown as claims, never folded. `untrusted` is a set of actor names, the
deployment's whole policy.

## 6. Folds

All folds take the linearised events and a moment `t`; they consider events
with `at <= t` IN CAUSAL ORDER (`chronological`).
1. `env_at(events, t, untrusted) : TodoId -> Completed(at) | Cancelled(at)`:
   last binding write wins; `Reopened` clears; only `binds` events write.
2. `specs_at`: the latest spec — `Authored.spec` (any actor) or trusted
   `SpecRevised.spec`.
3. `authored_at`: the latest `Authored` per todo, any actor.
4. `flatten(events, t, untrusted) : TodoId -> Term`: ONE function per todo —
   head = `checklist(first spec ever, first checklist length)`; at every
   moment a spec, a checklist length, or the trusted state changed, a
   `Piece(at, term)`: a flat 1.0 while resolved, else
   `checklist(spec in force, items in force)`; built by `mk_piecewise`
   (normal form). **The head extends to −∞** (Bassel, 2026-09-06: "we need
   something for all of time as a unit"): a todo's function is total over
   time, and its unit — what it is before anything was recorded about a half
   — is the EARLIEST recorded demand of that half, applied backward. A
   completion recorded before its record (the backfill) therefore stands
   against the demand the record says it had; nothing invents one. Stated
   consequence: the prefix law (§9.2) is exact for every moment after a
   todo's first spec AND first checklist are recorded; a late first half
   re-heads the curve before it. Absent when there was never a spec or a
   checklist. `checklist(own, n)` = `own` if `n == 0`; `Conj(n × Flat(0.5))`
   if `own` is None; else `OffsetBy(own, Conj(...))`.
5. `history(events, untrusted)`: the environment as a function of time —
   per todo the sequence of (at, binding|None); `at(t)` equals `env_at(·, t)`.
6. **Registers** over nodes (name, parents, event): a register per
   `(kind ∈ {state, spec, content}, todo)` holds its FRONTIER — the writes no
   later write descends from (ancestry bitsets over positions). One write is a
   value, more is a CONFLICT. `extend(state, nodes)` is a monoid action.
   Projections pick the write latest in the linearisation, so on any DAG
   `env_of/specs_of/content_of` equal folds 1–3; `conflicts_of` names the rest.

## 7. FPL

`Term` is the fixed point of the functor `TermF a`:
`Flat(value) | Decay(start, end, end_date, lead_up, start_date?) |
Curve(points) | Conj(terms, p) | Offset(delta, a) | Gate(gate, body) |
Shift(delta, a) | Within(window, p, a) | Importance(w, a) |
After(event, anchor, term, pending, needs?) | Piecewise(head, pieces) |
OffsetBy(delta, term)`.
Semantics `⟦t⟧(now, env) ∈ [0,1]`:
- Flat: v. Decay: 1.0 before `start_date`; 0.98 before `end_date − lead_up`;
  linear from `start` to `end` across the window; `end` after. Curve: linear
  between points, clamped outside.
- Conj: `power_mean(values, p)` — clamp each to ≥ 0.001, `p=0` geometric,
  empty = 0.5. Offset: `x(1−|δ|) + max(0, δ)`. Gate: `max(1 − ⟦gate⟧, ⟦body⟧)`.
  Shift: `⟦a⟧(now + δ)`. Within: power mean of 65 samples over
  `[now, now+window]`. Importance: `⟦a⟧^w`. After: unbound → `⟦pending⟧(now)`;
  `Completed(done)` → `⟦term⟧(now − (done − anchor))`; `Cancelled` → 1.0;
  a binding counts only if `done <= now` (`bound`). Piecewise: the piece in
  force (last with `at <= now`, else head). OffsetBy: `offset(⟦term⟧, ⟦delta⟧)`.
- **Normal form** (`mk_piecewise`): unit (no pieces → head), join (nested
  pieces spliced), no adjacent equal pieces, instants strictly increasing.
  `normalize` pushes Conj/Offset/Gate/Importance/OffsetBy under Piecewise over
  the merged partition; Shift translates instants; Within/After stay.
- **Explain** is a decoration: `Cofree TermF (value, notes)` — the term's own
  shape, each node carrying its fulfillment at the moment the parent used it
  (Shift/Within/After re-anchor time for their child) plus notes (Conj:
  certifies, shares; Within: peakAt, peakShare; After: bound; Piecewise: since,
  pieces). Its JSON is `to_json`'s shape plus those keys.
- **Breakpoints**: slope changes and jumps of the exact fragment (Flat, Decay,
  Curve, Piecewise of exact parts, Offset, Shift, constant composites); a
  series with a knot at each and a second before each jump IS the curve.
- JSON: `to_json`/`from_json` with the kind tags in `fpl.py`.

## 8. Types an implementation must have

`Hash` (64 hex), `TodoId`, `Actor`, `Login`, `MarkupSource`, `StringSource`,
`NoteSite`; `Envelope = Sealed | Woven`; `TodoEvent`; `Term` as `Fix TermF`;
`Explanation = Cofree TermF Annotation`; `Frontier` as a non-empty ordered set
of writes; `Folded` state; `Breaks`. Smart constructors (`mk_*`) validate;
records are dumb data; engine code never calls a raw constructor.

## 9. Laws (the conformance suite)

Every implementation, on `prodrome/conformance/*.json` and on the live chain:
1. print ∘ parse = id on every stored object, and hash(print) = name.
2. Prefix: append any later event; every earlier moment reads the same —
   except the head's unit (§6.4): a todo's FIRST spec or FIRST checklist,
   recorded late, re-heads its curve before it. Both halves recorded, exact.
3. Independent events commute; a uniform shift of every `at` changes no winner.
4. `history.at(t) == env_at(t)`.
5. `mk_piecewise`: unit, join, idempotent, no adjacent repeats; `normalize`
   preserves every reading and leaves nothing buried; both idempotent.
6. Registers equal the folds on any DAG; conflicts are exactly the registers
   both branches wrote; a merge settles nothing; a descending write settles.
7. Interpolation between series knots equals evaluation on the exact fragment.
8. `fulfillment` within 1e-9 of the reference on every vector; hashes, prints,
   linearisations, frontiers, and `verify` findings exactly.

## 10. Non-goals

A clock in the merge; a second evaluator anywhere; typst as a source of
truth; multi-tenant; a hosted service.
