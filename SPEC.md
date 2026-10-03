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
  keyword form. The admitted names are the vocabulary of §3, §4 and §7 plus
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
`Folds`, `Dags`, `Fpl`, `Series`, `View`, `Links`, `Recurs`, `Changes`;
`FoldCase`, `DagCase`, `FplCase`, `SeriesCase`, `ViewCase`, `LinkCase`,
`RecurCase`, `ChangeCase`; `Bound`, `Open`, `Spec`, `Content`, `Object`,
`Parents`, `Conflict`, `RowConflict`, `Refused`, `Step`, `Price`, `Sample`,
`Knot`, `Asked`, `Row`; and, for an explanation's
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

- An **object** is one of five constructors.
  - `Genesis(label, nonce)` begins a prodrome. `label` is the host's name for
    it and `nonce` is 32 lowercase hex characters drawn once, so two
    prodromes a host labels alike are two. It has no parents and no event,
    and its name is the prodrome's identity.
  - `Change(genesis, deps, event)` is an event, named by its prodrome and by
    the writes it supersedes. `deps` are object names, sorted, distinct and
    possibly empty; `event` is required, since a change without one would be
    a merge and there are none.
  - `Snapshot(genesis, tips, previous)` attests a state of one prodrome.
    `tips` are sorted, distinct and non-empty, and `previous` is a snapshot's
    name or `''`.
  - `Sealed(prev, event)` and `Woven(parents, event)` are the LEGACY
    envelopes, read forever: `append` writes neither, and `merge` writes a
    `Woven` only over a legacy store's tips. A `Sealed` has one parent,
    `prev == ""` at a legacy root; a `Woven` has two or more, sorted and
    distinct, and its `event` may be `None`: a merge is structure.

  `parents_of` is a `Change`'s `deps`, a `Snapshot`'s `tips` plus a non-empty
  `previous`, a `Sealed`'s `prev` (none at a root), a `Woven`'s `parents`,
  and nothing for a `Genesis`. Every rule below that reads parents reads
  these.
- An object's **name** is `sha256(utf8(print(object)))` in lowercase hex.
  The file `objects/<name>.py` holds exactly that print, and a loader
  re-hashes the bytes before parsing them. `event_id(e) =
  sha256(utf8(print(e)))` names an EVENT apart from where it was written; it
  is never stored.
- **Every object has exactly one genesis.** A `Change` or a `Snapshot` names
  it; a `Genesis` is its own; a legacy object's is the `Sealed("", …)` root
  it rests on, or, where a legacy store already joined two roots, the least
  such root by name, whose one prodrome every legacy object is in. No `deps`
  or snapshot edge crosses geneses. So a store may hold several prodromes, a
  union of two stores of different geneses is DISJOINT, and every fold,
  register and entry factors into per-genesis ones, a todo keyed by
  `(genesis, todo)` (§6). A `Change` may depend on legacy writes, whose
  global ancestry overstates what it depends on; that is safe, since a
  later write can only settle more than the chain says, never less.
- **A write is durable.** The print goes to a randomly named temp file in
  `objects/`, created exclusively (never a fixed `<name>.tmp`, which two
  writers of one object would share), and is synced to disk before it is
  renamed to `<name>.py`; the directory is synced once after a batch of
  placements (one append, one adoption). So a crash leaves either no object
  or the whole one, never an empty file under a name that promises content,
  and at worst a temp that `verify` reports.
- **Tips are derived.** `tips()` is the set of objects no object names as a
  parent: a function of the object set alone, in any order it is listed. A
  store on disk is its `objects/` and nothing else, so two copies that each
  only added objects are merged by uniting the files — a `git merge` of two
  clones is this union and cannot conflict. A store under git needs
  `objects/** -text -diff` in the `.gitattributes` that governs it, so that
  no line-ending conversion or text merge rewrites the bytes a name is the
  hash of. A store written before 0.9 also holds `HEAD` (one tip) and
  `refs/` (one file per head while there were several); nothing reads them,
  the objects derive exactly what they named (§9.17), and `verify` reports
  them as leftovers to delete.
- **`init(label)`** writes a prodrome's `Genesis`, once.
- **Appending writes a `Change`**, never a `Woven`, into the writer's
  genesis; a store of several geneses is written through a handle that
  names one.
  - If the genesis holds an object whose event prints byte-identically to
    `event`, `append(event)` writes nothing and answers the first such
    object in the linearisation, even one since superseded. Appending is
    IDEMPOTENT: replaying a delivery whose claim was overridden brings
    nothing back, and a host that means to assert it again, having seen the
    override, says something new — a new `at`, a new note.
  - Otherwise it writes `Change(genesis, deps, event)`. `deps` are the
    HEADS of `event`'s entity in the writer's genesis: every object the
    writer holds whose event names that entity, in any register or none,
    that no other such object descends from. They are an antichain, and
    what the writer SAW of the entity, as a git commit's parents are what
    its author had. Each register's frontier is beneath them, so a write
    still supersedes what it supersedes (§6.6), and nothing else. The heads
    are read STRUCTURALLY: under the policy where everything binds, and at
    no moment, because a name records what its writer held and never a
    reader's policy or moment. The first write to an entity has `deps` of
    `()`. (Before 2026-10-01 `deps` were only the frontiers of the
    registers `event` writes, so a writer's add and complete of one entity
    were recorded as concurrent; objects written then keep their bytes and
    read as they did.)
  - A write to an INFLATIONARY register (§6) whose value is not `≥` the
    reading it supersedes is refused before any object carries it, by the
    same structural reading. Of the todo's registers only the tendings are
    inflationary, and a `Tended` supersedes nothing, so no event of §4 is
    refused; a schema whose register supersedes and is inflationary is held
    to it (law 32).

  `deps` never leave the entity (one todo, under the todo schema): a store
  of changes is a disjoint union of per-entity DAGs, and changes to
  different entities are concurrent. A change's name is a function of its
  genesis, its event and what its writer held of its entity, and of nothing
  else: two replicas that agree about an entity write byte-identical
  changes to it, whatever else each holds. Two that disagree may each write
  one event over their own heads; those are TWINS, two objects with one `event_id`, each
  superseding only what its own deps reach, and a register reads them as
  one candidate (§6).
- **`snapshot()`** writes `Snapshot(genesis, tips, previous)`: the genesis's
  tips, and as `previous` the last snapshot among them; with nothing written
  since that one, it is the answer and nothing is written. Its closure is
  exactly what it attests: an object is in it iff it existed when the
  snapshot was written, because a name cannot be computed before what it
  hashes, and on a `previous` chain each closure contains the one before. No
  fold reads a snapshot, and no change depends on one. When to write one is
  the host's call.
- **Linearisation** is Kahn's algorithm over parents with a min-heap on the
  name: deterministic, causal first, and arbitrary only between incomparable
  objects, which the name orders and no instant does. It orders an entity's
  `stream` (§6.7) and numbers objects for the ancestry index, and decides
  no value (law 25). A missing parent or a cycle is a refusal.
- `ancestors(x)` is the transitive parent closure; `concurrent(a, b)` holds
  when neither is an ancestor of the other.
- **`verify`** reports an object not hashing to its name, a missing parent, a
  cycle, a malformed `Woven`, a leftover `HEAD` or `refs/`, every entry of
  `objects/` that is not named `<name>.py` for a well-formed name (a temp an
  interrupted write left, or a stray) as garbage, each by name — the reads
  pass over such an entry, and only `verify` speaks of it — a receipt for
  each file in `quarantine/`, which stands in for the missing parent that
  object would otherwise be reported as, and an event the
  policy does not `confirm` (§5) dated before any of its ancestors — a writer
  whose stamp the host forces cannot legitimately be dated behind what it was
  written on top of, where a backfill can; a change's ancestors are what its
  deps rest on, so it cannot be dated behind what it was written OVER. It
  also reports an object naming a genesis the store does not hold, an edge
  between geneses, a dep another dep of the same change rests on, a dep that
  is not a write to its change's entity, and an object a snapshot attests
  that the store lacks. There is no unreachable object and no stale
  head to report: every object is a tip or beneath one, and no tip rests on
  another.
- **`quarantine(name)`** moves `objects/<name>.py` to `quarantine/<name>.py`
  when its bytes do not hash to `name`, and refuses a file that does (that
  file is the object, even one this reader cannot parse). A read refuses a
  file failing its hash rather than guess what it held — `tips()` among them,
  and so every append — and its refusal names the object and this operation
  (`prodrome quarantine <name>`). Set aside, the store is what it holds
  without it: `tips()` answers (the object's parents may be tips again), and
  `verify` reports the receipt until the object is restored from a replica
  and the quarantined file deleted. An append made meanwhile rests on the
  heads of what was held; once the object is back, a head it puts beneath
  another is reported as a redundant dep, which changes no ancestry.
- **`adopt(source, tip)`** copies in everything `tip` rests on that the store
  lacks, a change's or a snapshot's genesis included, verifying all of it
  before writing any, and writing parents before children, since an object
  belongs to the store as soon as its file exists. Placement falls out of the
  derivation: a tip already contained changes nothing; a tip containing every
  tip fast-forwards; otherwise it is a second tip. The source may be another
  store or a map of prints that arrived over a wire (`adopt_objects`); only
  where the bytes are read from differs. A change's bytes say nothing of
  which store holds it, so moving its file between two stores of one genesis
  changes no name (law 29).
- **`merge(parents)`** writes a `Woven` over them, every legacy tip by
  default: a legacy store's join, which a store of changes never needs.
- **A replica** is any set of objects closed under parents that reads (its
  objects, their fold, its tips), appends and receives: a store on disk, a
  store in memory, or an overlay. `receive(seeds)` takes in what `seeds`
  rest on that it lacks, verifying every object as `adopt` does before it
  takes any, parents first. An append decides on what the replica holds by
  the rules above, so two replicas holding the same objects write the same
  bytes, wherever each is held.
- **`accept(from, to, chosen)`** is `to.receive(chosen)` with `from`'s
  prints: the join restricted to what `chosen` rest on. Where `chosen` is
  closed under parents over `to`, `to` reads as it did plus exactly
  `chosen` (law 39). `sync`, an overlay's flush and `adopt` are it, with
  everything, all of the overlay's own objects and one tip chosen.
- **`sync(from, to)`** is `to.receive(from.tips())`: `to ∪ from`, the join.
  It reads no register. It is idempotent and order-free, and a sync cut
  short leaves `to` closed under parents and is completed by the next
  (law 35).
- **An overlay** over a replica writes to memory and reads `own ∪ base`:
  the base's objects and fold, with its own objects folded over them. It
  writes nothing to the base until flushed, and its flush is `sync(overlay,
  base)`.
- **A decision is the store of record's.** A question no monotone reading
  answers ("is it still standing, so I may perform?") is asked of a store on
  disk under its lock, and the record of its answer appended and synced
  before the lock is let go and the effect performed. A store in memory and
  an overlay hold no lock another writer takes and do not write through, so
  no decision is asked of one; monotone writes need no lock, and every
  replica appends them.
- **A history held in a register** is held by its heads: an event field of
  names, printed as a tuple sorted and distinct, whose register's type says
  which schema's objects they name (design §6.3). THE OBJECT MODEL GAINS
  NOTHING: no constructor and no field of an envelope is new, the inner
  objects are objects of their own schema in a replica of it, and their
  names are global. A NEST of such histories flattens into one history
  keyed by path, each object at the path of the register that held its
  history and then its entity's key, under the name it had: no object is
  renamed and no dep rewritten. An object's parents are of its own level,
  so a held history's head, or a parent within it, that its level lacks is
  a missing object; causality crosses levels only where a write names the
  inner heads it saw.

## 4. Events

A store is opened at a **schema**: its event vocabulary, the entity each
event is about, the registers each event writes (§6), and which events a
policy is asked about (§5). A schema is the STORE'S type and no object
carries it, so `Genesis(label, nonce)` is frozen as it was; a stored print
is parsed against the envelopes' vocabulary (§3) and then the schema's, and
an event the schema does not name is refused at that boundary. Every event
says about what, when its writer thought it was (data, §1) and who.

The REFERENCE schema is the **todo schema**, below: every conformance vector
was taken at it, and every legacy envelope (`Sealed`, `Woven`) belongs to
it. Kinds and fields, in order; all shipped; `""` means absent.

- `Created(todo, at, actor, text, note)`
- `Completed(todo, at, actor, note)`, `Cancelled(…)`, `Reopened(…)`
- `Tended(todo, at, actor, note)` — care taken on a todo that is never done,
  a pass at a recurring chore. It changes no state, spec or content: §6.1
  folds it into the tendings, and §7.3's `Recur` reads them.
- `SpecRevised(todo, at, actor, spec, note)`
- **The record kind**: `KIND(todo, at, actor, <the host's fields>)`.

The six kinds above are the TODO SCHEMA's: their fields are its semantics,
and they are frozen here. Each is about the todo it names. A record is a
todo's CONTENT, and content is a deployment's — so a host **payload**
supplies

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
the database reads an actor name to decide anything. The SCHEMA says which of
its events a policy is asked about (§4); any other binds whoever wrote it.
The todo schema asks about every kind but the record.

A policy answers one further question, whose default is the reading above:
`confirms(event)`, "is this the host's own word", which is `standing(event) ==
Binds` unless the policy says otherwise. Two readers ask it and neither is a
fold — §6.7's `confidence` field, and §3's clock rule — and a policy that folds a
writer's events while still marking them as that writer's is why it is asked
separately. `Claims` implies not `confirms`.

**The reference policy** — the one `conformance/*.py`'s `untrusted` fields
name, and the one this repository's vectors were taken under — is a set of
actor names, over any schema. An event `Claims` when its actor is on the set
and the schema asks about it — under the todo schema, a lifecycle, `Tended`
or `SpecRevised` event; everything else `Binds`, a
content record included, because writing content is what such a writer is for
and a fold that hid its writes would be an outage that reports success. It
`confirms` an event exactly when the actor is not on the set — so a content
record from a named actor binds and is still shown as that actor's. The empty
set is the policy under which everything binds and nothing is a claim.

**Naming is structural, belief is not.** A change's `deps` (§3) are read
under the policy where everything binds, because a name records what its
writer held. A reading's registers still fold only what its policy binds
(§6): a claim neither joins a confirmed frontier nor removes a write from
one, whatever its deps, so an untrusted change cannot settle a conflict
between trusted writes in the confirmed reading. A LATER binding write that
descends from it does, since its writer held everything the claim names.
Each reading has its own candidates — the confirmed reading's are the
binding writes, and a register none of whose writes binds is unwritten
there; the claimed reading's are all of them — so a claim never moves a
confirmed price (law 11), and one more urgent than the confirmed answer is
SHOWN (§6.7). Twins cannot differ in standing: standing is a function of the
event, and twins carry one.

**Both readings stay in the database** (§6.7): the CONFIRMED one, under the
host's policy, and the CLAIMED one, under the policy where everything binds. A
store that kept only the filtered reading could not show a claim at all, and
showing one is the whole point of storing it.

## 6. Folds

**A schema's registers.** An entity's REGISTERS are a product, one
component per register the schema names, each a join-semilattice, so the
product is one and folding a stream into it is a monoid action (6). The
schema routes each event to the registers it writes. A register a write
SUPERSEDES in holds its FRONTIER, the writes to it no later write to it
descends from; one that only accumulates is a grow-only set, joined by
union. Each register has a TYPE: its values, a partial order on them, and
the value a write gives it. The order decides no succession; the register's
reading is the MAXIMAL values of its frontier's values, the free completion
of the order: one is a value, and more is a conflict. A type may declare its
register INFLATIONARY, every write `≥` the reading it supersedes; §3's
append refuses a write that is not, before any object exists, and the
reading of such a register is then a homomorphism (law 32). A schema MAY
also carry a PRICE: the §7 term each candidate of an entity's reading prices
as, the reading priced as `Least` over them, their meet (law 34); for the
todo, a candidate is a world (4). An entity whose schema has no price has
none at all, which is not `Absent`.

**A schema may be declared.** A schema is a value a store is opened at, and
no object carries it. Beside a schema written as code, one may arrive as
TEXT, in the §2 grammar under the schema language's own closed vocabulary:
`Schema(key, events, registers)`, each `Event(name, fields)` a constructor
whose `Field(name, type)`s are in printed order, a type one of `Text()`,
`Integer()`, `Instant()`, `Reference()` (a §3 name), `Enum(alternatives)`
and `List(item)`, and each `Register(name, order, inflationary, writes)`
ordered by one of `Discrete()`, `Total()` (over an integer),
`Machine(covers)` (the reflexive and transitive closure of its `(lower,
upper)` pairs over an enum's alternatives) and `Inclusion()` (a list read as
a set), written by `Write(event, field)`s. It is ADMITTED only when, in this
order: it parses; it is its own canonical print, every set (events and
registers by name, alternatives, covers, writes) sorted, so its name is the
hash of its text as an object's is; its names are unique, and no event's is
an envelope's or the grammar's own; every event has the key field, a text;
every event has `at`, an instant, and `actor`, a text; every register is
written, by existing fields of one type, the type its order reads; every
machine's covers are acyclic; and every inflationary register's order has a
bottom. A refusal names the first law that fails. An admitted schema's
events print as their constructor with every field in declared order, its
registers are read as every register is (the maximal values of the
frontier under the declared order), and an inflationary one is held to §3's
refusal. A declared schema carries no price.

Every fold is a projection of one entity's registers at a moment `t` under a
§5 POLICY. A write joins its register when it is dated at or before `t` and
the policy binds it — an event the schema does not ask about joins whoever
wrote it — and what it supersedes is ancestry alone: no clock and no
linearisation picks a winner. Every fold factors per genesis (§3), an entity
keyed by `(genesis, key)`; in a store of one genesis that key reads as the
entity's.

**The todo schema's registers** are its state, spec and content, which a
write supersedes in, and its tendings. The orders of the first three are
discrete, a value being the event that wrote it, so a reading is its
CANDIDATES, the distinct events of its frontier, exactly as before, and
twins (§3) are one candidate; the tendings are a grow-only set, ordered by
inclusion. None is inflationary: a `Reopened` after a `Completed` goes
back, which is why a todo's conflicts are shown and priced. Its valuation is
FPL, below. A content record joins whoever wrote it.

1. `env(t, policy)`, the environment, in two halves. `outcomes : TodoId →
   set of candidate bindings`: the candidates of the todo's state register,
   each `Completed(at)` or `Cancelled(at)`, with "open" a candidate where a
   `Reopened` is one; a todo whose one candidate is open is unbound.
   `tended : TodoId → set of instants`: the instant of every binding
   `Tended`, a grow-only set folded by union, so nothing removes a tending
   and no order among them matters. Only events the policy says `Binds`
   write either half.
2. `specs(t, policy)`: the candidate specs of each todo's spec register,
   written by a record's payload (whoever wrote it) or a `SpecRevised` the
   policy binds.
3. `content(t)`: the candidate records of each todo's content register,
   whoever wrote them — no policy parameter: content renders, and §6.7 marks
   the row instead.
4. `flatten(t, policy) : TodoId → Term`: one function per todo. Its head is
   `checklist(first spec ever, first checklist length ever)`, from the writes
   no other write to their register precedes; at every moment a spec, a
   checklist length or the bound state changed there is a `Piece(at, term)`:
   a flat 1.0 while resolved, else `checklist(spec in force, items in
   force)`; assembled by `mk_piecewise`. "Spec" and "items" are the
   payload's two readings (§4) and the whole of what a record contributes. A
   `Tended` puts no piece: it is care and not a transition, and the terms
   that read tendings read them from the environment (§7.3), so no stored
   function changes meaning. The head extends to −∞: a todo's function is
   total over time, and before anything was recorded about a half its unit
   is the earliest recorded demand of that half. A completion recorded
   before its record therefore stands against the demand the record says it
   had. Consequence: the prefix law (§9.2) is exact for every moment after a
   todo's first spec and first checklist are recorded; a late first half
   re-heads the curve before it. Absent when there was never a spec or a
   checklist. `checklist(own, n)` is `own` when `n == 0`; `Conj(n ×
   Flat(0.5))` when `own` is `None`; else `OffsetBy(own, Conj(…))`.

   **A conflict prices as its most urgent world.** A WORLD of todo `x` picks
   one candidate from each of `x`'s written state, spec and content
   registers; its log is the log less the other candidates of those
   registers. The term in each piece is `Least` (§7) over the worlds in force
   at that piece's instant, each world's term as above, so one world is the
   term above byte for byte. Low is urgent, so a conflict rises and never
   sinks, and nothing picks a winner. Taking the most urgent spec and the
   most urgent state separately could build a combination no writer wrote; a
   world is always a reading some writer's view contains. The worlds are the
   product of the three candidate counts, and a conflict is shown and
   settled by the next write, so they are few.
5. `history(policy)`: the environment as a function of time. `history.at(t)`
   is the registers folded at `t`, which change only at the instants writes
   are dated; the tendings, a grow-only set, are their own history. No
   causal order enters it.
6. **Registers** over nodes `(name, parents, event, genesis)`, under any
   schema; for the todo's, one register
   per `(genesis, kind ∈ {state, spec, content}, todo)` holds its frontier,
   the writes to it that no later write to it descends from. A `Tended`
   writes no register: it joins the tendings, a grow-only set kept beside the
   frontiers, so concurrent tendings are their union and never a conflict.
   `extend(state, nodes)` is a monoid action. SUPERSESSION IS PER REGISTER:
   a write supersedes exactly the writes TO ITS OWN REGISTERS that it
   descends from, through any path; descending from a write in another
   register supersedes nothing there, and a conflict is two or more writes
   to one register none of which descends from another (law 37). A write
   that descends from a conflict settles it, and nothing else does: a merge
   settles nothing.
   Ancestry is indexed within each `(genesis, todo)` for changes, whose deps
   never leave a todo, and over every legacy object for legacy objects, so
   only a mixed store pays for both. `conflicts_of` names each register whose
   frontier holds more than one write.
7. **The entry**, under a schema with a valuation; here the todo's.
   `entries(nodes, t, policy) : [Entry]`, one row per todo
   any event mentions, per genesis and by id. It is the composition of the
   folds and §7, stated once so every consumer performs it once. With
   `confirmed` the registers under the policy: `outcome` is the todo's
   candidate bindings, one member where nothing conflicts; `claim` is the
   candidate bindings under everything-binds, where their kinds differ from
   `outcome`'s, absent where they agree; `spec = flatten(…)[todo]`, `Absent`
   (§7) where the todo has none, and `value` the fulfillment at `t` of
   `link(spec, specs)` (§7.2) under `confirmed`'s environment, where `specs`
   is `flatten(…)` over the todo's prodrome with `Absent` for every other
   todo it mentions — so `value` is a number, `absent` where it reads `∅`,
   or, where `spec` does not link, the `LinkError`, and under a conflict it
   is the least over worlds; `content` is the names of the candidate content
   records, sorted, and never the records — a consumer that wants the
   payload looks the object up, because what a record MEANS is the host's;
   `conflicts = conflicts_of(confirmed)[todo]`; `stream` is every node whose
   event names the todo, in causal order — the one reading the linearisation
   orders, and since deps never leave a todo, Kahn's algorithm over the
   todo's own objects. CAUSALITY FIRST: a change rests on every write to
   its entity its writer held (§3), so a writer's own writes to one entity
   stream in the order it wrote them, whatever registers they touch and
   whatever instants they carry (law 36), and a write that reached a writer
   before it wrote streams before its write. The name breaks a tie only
   between writes that truly were concurrent; no instant ever does, so a
   host never stamps instants apart to order its own writes. A todo whose
   events are all dated after `t` is still a row, open and `absent`: `t` asks what is believed, not what exists. A
   LIST of entries is in one order, defined here and by no host: ascending
   in value, ties by `(genesis, id)`, then every row with no number, `absent`
   or not linking, by `(genesis, id)`.
   `confidence` says whether the
   answer is the confirmed reading whole: a claim refused, a candidate
   content record the policy does not `confirm`, or both. Checked against `conformance/view/*.py` — SEEDED, not
   taken from the reference like the rest of `conformance/`: random logs
   drawn from this crate's own generator (`core/tests/common/mod.rs`'s
   `a_log`, the one `core/tests/all/fold_laws.rs`'s properties draw from too)
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
| Piecewise(head, pieces) | OffsetBy(delta, term) | Least(terms) | Ref(todo)
| Absent
```

Semantics `⟦t⟧(now, env) ∈ [0, 1] ∪ {∅}`. `∅` is NO VALUE: a note, a
reference, a proposal nobody has priced has no claim on attention, and `∅` is
not a number — an implementation's value type keeps it apart (the reference's
is `Option<f64>`). It is the identity of composition: it neither raises nor
lowers anything it is composed with.

- `Absent`: `∅` at every instant. It has no field; it prints `Absent()`.

- `Flat`: `value`. `Decay`: 1.0 before `start_date`; 0.98 before `end_date −
  lead_up`; linear from `start` to `end` across the window; `end` after.
  `Curve`: linear between points, clamped outside.
- `Conj`: the power mean, with exponent `p`, of the values of the members
  that have one, each clamped to at least 0.001; `p = 0` is the geometric
  mean; `∅` when no member has a value; empty is 0.5 (`Conj([])` has no
  member to be absent, and keeps its stored meaning).
  `Offset`: `x(1 − |δ|) + max(0, δ)`. `Gate`: `max(1 − ⟦gate⟧, ⟦body⟧)`; an
  absent gate is no gate, `⟦body⟧`, and an absent body is `∅`.
  `Shift`: `⟦a⟧(now + δ)`. `Within`: the power mean of those of the 65
  samples over `[now, now + window]` that have a value, `∅` when none does.
  `Importance`: `⟦a⟧^w`. `Offset`, `Shift` and `Importance` take `∅` to `∅`.
- `Least`: the minimum of the members that have a value, `∅` when none does,
  so `Absent` is its identity, as for `Conj`. It has at least one member,
  and `Least([t])` is `t`. It is commutative, associative and idempotent: a
  semilattice. It is how a conflict prices (§6.4), since min cannot be
  written with the other constructors — `Conj`'s `p → −∞` limit is min, but
  `inf` is not storable, and `Gate` is a max.
- `After`: unbound, `⟦pending⟧(now)`; `Completed(done)`, `⟦term⟧(now − (done
  − anchor))`; `Cancelled`, 1.0, whatever `term` reads. A binding counts only
  when `done <= now`. `pending` may be `Absent`, as `Recur`'s may. Where the
  environment holds several candidate bindings of `event` (§6.1), `After`
  reads the minimum over them of the reading under each.
- `Recur`: with `s = last_tended(env, todo, now)`, the latest tending of
  `todo` at or before `now`: none, `⟦pending⟧(now)`; else `⟦term⟧(now − (s −
  anchor))` — `After`'s slide, from the last pass (§7.3).
- `Periodic`: `⟦term⟧(anchor + ((now − anchor) mod period))`, the remainder
  Euclidean (non-negative), in microseconds.
- `Piecewise`: the piece in force, the last with `at <= now`, else the head
  — which may be `Absent`: priced from a date, or unpriced after one.
  `OffsetBy`: `offset(⟦term⟧, ⟦delta⟧)`; an absent delta is no offset,
  `⟦term⟧`, and an absent term is `∅`. So `checklist(own, n)` with an
  absent `own` reads as the checklist alone.
- `Ref(todo)`: no reading of its own. It is a variable, bound by `link`
  (§7.2); the semantics above are defined on CLOSED terms only.
- **Normal form** (`mk_piecewise`): no pieces means the head; nested pieces
  are spliced; no adjacent equal pieces; instants strictly increasing.
  `Absent` is a leaf and `normalize` leaves it where it is. `normalize`
  pushes `Conj`, `Least`, `Offset`, `Gate`, `Importance` and `OffsetBy`
  under `Piecewise` over the merged partition, translates instants under
  `Shift`, and leaves `Within`, `After`, `Recur` and `Periodic` in place: a
  window, a lookup and a fold of time onto one cycle do not commute with a
  partition.
- **Explain** is a decoration, `Cofree TermF (value, notes)`: the term's shape
  with each node's fulfillment at the moment its parent used it — `∅` where
  the node has none — plus notes (`Conj`: certifies, and shares over the
  members that have a value, an absent member's 0, both omitted where the
  conjunction is `∅`; `Within`: peak instant and share over the samples that
  have a value, omitted where none does, the subterm then explained at `now`;
  `After`:
  bound; `Recur`: bound — `pending` or `tended` — and, when tended, the last
  tending and the hours since it; `Periodic`: the start of the cycle in
  force; `Piecewise`: since, pieces). A serialization of one is the term's own
  shape carrying those two at every node.
- **Breakpoints**: the slope changes and jumps of the exact fragment (`Flat`,
  `Decay`, `Curve`, `Absent`, `Piecewise` of exact parts, `Offset`, `Shift`,
  `Least` of exact parts, constant composites). `Absent` is exact and has
  none; a `Least`'s include the instants where two of its members cross. A series with a knot
  at each and a second knot before each jump is the curve; a knot is `∅`
  where the term has no value, and no line is drawn to or from one. Other
  terms are sampled and say so.
- **Next change** (`observe::next_change(term, now, env, observation)`): a
  view shows a value at a precision, so what it draws is a function of the
  OBSERVED value. An `Observation` is a quantisation: `[0, 1]` in `n ≥ 1`
  equal steps, a value read as the step it rounds to (`down`, `nearest`
  with a half read upward, or `up`), and `∅` read as itself; it is
  monotone, so each step observes an interval and `observation ∘ ⟦t⟧` is a
  step function of time. The answer is `(at, exact)`: the first instant
  after `now`, on the microsecond grid, at which the observed value
  differs from its value at `now`, or none for never. EXACT on the exact
  fragment: affine between its breakpoints, so the step inside each
  stretch is found by bisection against the evaluator itself, and constant
  past the last. CONSERVATIVE elsewhere: the first instant at which an
  enclosure of the term over `[now, t]` (each atom's range, monotone
  composites on their children's bounds, a `Within` over its widened span,
  `After` and `Recur` split where a binding or tending comes into force,
  `Periodic` folded onto one cycle) stops being one observed value. Never
  later than the true change, possibly earlier; an enclosure over all time
  that is one value proves never, which is exact. `exact` is a fact about
  the behaviour, not the term's syntax. The environment is a snapshot, read as
  evaluation reads it: a binding or tending dated after `now` comes into
  force at its instant.
- A term has ONE serialization and it is §2's literal print. A JavaScript
  boundary may carry a JSON shape of the same term — the reference one is
  `prodrome-wasm-exports`'s `json` module, lowercase kind tags and spans in
  hours — but that is a boundary's business and not the database's, exactly
  as a rendering is (§1).

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

- UNBOUND — `compile(pending)`. `bound` answers `None` at every instant, so
  the link is its pending branch and nothing else.
- `Completed(τ)` — `Piecewise(compile(pending), [(τ, Shift(anchor − τ,
  compile(term)))])`. Before τ the link is unbound; from τ the body slides by
  the slippage, and sliding `now` by a constant IS a `Shift`.
- `Cancelled(τ)` — `Piecewise(compile(pending), [(τ, Offset(1.0,
  compile(term)))])`. `x·(1 − |1|) + max(0, 1) = 1` for every `x`: the moot
  constant is a graded offset at δ = 1, and writing it as one keeps the mooted
  demand in the tree where a reader can still see what was dropped.
- A CONFLICT, several candidate bindings of `e` (§6.1) — `Least` of the link
  compiled under each candidate, so the compiled term reads what §7's
  `After` reads and law 13 holds as stated.

So each link contributes ONE graded offset δ ∈ [0, 1] — its MOOT GRADE, 1
where the upstream was cancelled and 0 everywhere else — applied with §7's
corrected form; δ = 0 is elided because `offset(x, 0) = x`, and so is a zero
`Shift`. An offset of `∅` is `∅`, and a cancelled upstream is moot whatever
the body reads, so a body holding an `Absent` is written `Gate(compile(term),
Flat(1.0))` instead: `max(1 − x, 1) = 1`, an absent gate is no gate, and the
dropped demand stays in the tree. The result is `normalize`d, so a chain (A
needs B needs C) is ONE
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
any todo whose demand is composed from other todos', or an unpriced object
that serves one. `todo` obeys `TodoId`'s rule and the constructor refuses
anything else. It is a LEAF of `TermF` and a
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
`flatten`'s (§6.4) over every todo of the term's prodrome — an unqualified
`Ref(todo)` names that todo in its own genesis (§3) — and `Absent` for a known
todo with none — a record whose `spec` is `None`, or a todo only ever
created — so `Ref(x)` means x's whole function, its revisions and its
lifecycle, a completed child reads 1.0 from its completion, and a reference
to an unpriced todo links and reads `∅`, which a conjunction passes over.

**Refusals are values.** `LinkError::Unknown(x)` where `specs` holds no `x`:
for a store, an id its prodrome has never seen.
`LinkError::Cycle(path)` where the references loop: the substitution keeps the
path of references it is expanding, and meeting one already on it is the
cycle, reported with that path, its first todo repeated at the end. So `link`
is total and never loops. A cycle is refused where it is REACHED: specs that
loop elsewhere do not stop a term that never names them from linking.

No stored byte moves: `Ref` is a new constructor (§1), and a store without one
reads exactly what it read before. Nor does `Absent`, a new constructor too: a
record whose `spec` is `None` reads as `Absent` at the todo, so an old store
reads as before except that a `Ref` to an unpriced todo, refused before `∅`
existed, now links.

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

`Hash` (64 hex), `TodoId`, `Actor`, `Name`; `Schema`, §4's store type as a
parameter — an event vocabulary with its parse and print, the key, instant
and actor of each event, the registers it writes, which events a policy is
asked about, and an entity's registers as a product — carried by value and
never as an existential, since one store holds one schema, its vocabulary
a value the store is opened at (a unit for a schema written as code, its
declaration for a declared one, §6), and beside it,
each only where the schema has it, `Price`, the term each candidate of a
reading prices as (§6), `History: Price`, the terms of each moment of a
price that changes with its history (§6.4), `Bind`, what a `Ref` reads of an
entity in its store (§6.1, §7.2), and `Row`, what a row shows and when it is
provisional (§6.7); a register TYPE, its
values, their `Order` and the value a write gives it; `Payload`, the todo
schema's record kind as a parameter — a constructor name, a field order, a
vocabulary, a parse, a print, `spec` and a checklist length;
`Standing = Binds | Claims` and
`Policy`, §5's standing as a parameter — one function of an event, carried by
value like the schema and for the same reason, since one store reads under one
policy; `Envelope = Genesis | Change | Snapshot | Sealed | Woven` over a
schema's events, each object's genesis beside it (§3);
`TodoEvent`, the todo schema's events; `Term` as `Fix TermF`, `Absent` among its leaves; a value as
`[0, 1] ∪ {∅}`, `∅` never a number (`Option`); `Closed` (§7.2), a term with
no `Ref`, which is what every evaluator takes, and `LinkError = Unknown |
Cycle`;
`Env`, the environment, each todo's candidate outcomes beside the grow-only
tendings (§6.1, §7.3);
`Explanation = Cofree TermF Annotation`; `Compiled` (§7.1), a term with every
`After` and `Recur` resolved beside the `Link`s it resolved, whose readers
take no environment because a compiled term cannot consult one, and
`ChainError`, whose
`Cycle` carries the path; `Frontier`, an ordered set of writes, empty for
an unwritten register, and its candidates; `Order`, a partial order on a
register's values (`Discrete` by default, `Total`, inclusion on a set), and
`Inflationary`, a register type's declaration that a write only grows its
reading; `Folded`; `Replica`, what every store is (§3), and `sync`, generic over
two of them; `Decision`, the store of record locked, the only value a
coordinated decision is asked of and one only a store on disk makes;
`Breaks`; `Entry`
(§6.7) over a schema with a valuation, keyed by genesis and key, whose
reading is the schema's (a todo's outcome is a candidate set), whose
function is always
there (`Absent` where the todo has none), whose value is a number, `∅` or a
`LinkError`, and whose `Confidence` is a sum with no "provisional for no
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
   reading.
4. `history.at(t) == env(t)`: by definition, since both are the registers
   folded at `t` (§6.5).
5. `mk_piecewise` is a normal form: unit, join, idempotent, no adjacent
   repeats. `normalize` preserves every reading, leaves nothing buried, and is
   idempotent.
6. The folds are the register projections (§6), so there are not two
   implementations to compare; conflicts are exactly the registers both
   branches wrote; a merge settles nothing; a descending write settles.
7. Interpolation between series knots equals evaluation on the exact
   fragment: a line between two numbers, `∅` between two `∅`s, and a pair
   with one absent end only ever the second before a jump.
8. Fulfillment within 1e-9 of the reference on every vector; hashes, prints,
   linearisations, frontiers and `verify` findings exactly.
9. **The view is the composition** (§6.7): on any DAG and at any moment,
   every field of every entry equals the fold it is named by, and the rows
   are exactly the todos the events mention. Against `conformance/view/*.py`
   (seeded, §6.7) on linear chains and against `core/tests/all/fold_laws.rs`'s
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
    SET; law 25 is the other half, that no linear extension of the DAG reads
    differently.

Law 13 quantifies over the ENVIRONMENT, and it is what makes §7.1's compiler an
optimisation rather than a second evaluator.

13. **Compilation preserves every reading** (§7.1). For every term and every
    environment, `fulfillment(compile(t, env).term, now, ∅)` equals
    `fulfillment(t, now, env)` to 1e-9 at every instant — the compiled side
    read against the EMPTY environment, which is the statement's teeth: it
    answers the same while unable to look anything up, because `After` and
    `Recur` are the only constructors that read the environment and compiling
    leaves neither. On `core/tests/all/fpl_laws.rs`'s own generators, the
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
    constructor (law 1). On `core/tests/all/fpl_laws.rs`'s generators, over
    acyclic specs; and against `conformance/link.py`, written BY HAND from
    these laws — exact linked prints, readings to 1e-9, an unknown todo and a
    cycle refused. Law 18 says what a reference to an unpriced todo links
    to.

Laws 15 and 16 are §7.3's: care is a set, and recurrence reads it as of now.

15. **A tending is care, not state** (§4, §6). A `Tended` changes no
    outcome, spec, content record, function or register at any moment, and
    every entry keeps its state, its function and its content — an open todo
    stays open. The tendings are a grow-only set: `history.at(t)` holds the
    ones dated at or before `t` (law 4), a merge of two histories folds to
    the union of their tendings, and a tending writes no register, so never
    a conflict.
    `last_tended(env, x, now)` is the latest tending at or before `now`, and
    a tending dated later changes nothing it answers. On
    `core/tests/all/fold_laws.rs`'s generators, which draw `Tended` among the
    other kinds, and `core/tests/all/fpl_laws.rs`'s.
16. **Recurrence reads time as §7.3 says.** With the last tending of `x` at
    `s`, `Recur(x, anchor, term, pending)` at `s + d` equals `term` at
    `anchor + d`; with no tending at or before `now` it equals `pending` at
    `now`; and a tending of `x` dated after `now` changes nothing its own
    lookup reads at `now`. `Periodic(period, anchor, term)` at `now + period`
    equals itself at `now`, and on `[anchor, anchor + period)` equals `term`.
    Both round-trip through print and parse (law 1), and the smart
    constructors refuse a `Recur` whose `todo` is not a `TodoId` and a
    `Periodic` whose `period` is not positive. On `core/tests/all/fpl_laws.rs`'s
    generators; and against `conformance/recur.py`, written BY HAND from
    these laws — `Tended` prints folded under the reference policy, a claimed
    pass among them, and exact term prints with readings to 1e-9.

Law 17 is §3's: the tips are the objects, so a union of stores is a union of
files.

17. **The tips are derived.** (a) The tips of a union of two stores are the
    tips of either side that the union does not name as a parent, and equal
    the tips of the union's object set; two stores' files copied into one
    directory derive exactly them. (b) Two writers who each append to a copy
    of one store, and whose object directories are then united (a `git
    merge`), read, fold and view exactly as the Prodrome's own replica merge
    of the two stores (`adopt` of the other's tips). (c)
    Derivation does not depend on the order objects are listed or files were
    written. And NO STORED BYTE MOVES: for every `conformance/dag.py` store,
    laid out with the `HEAD` and `refs/` its writer left, the derived tips are
    exactly the heads those files named. On `core/tests/all/tips.rs`'s random
    DAGs, pure and on disk, which also check that the tips cover the store and
    none rests on another; `core/tests/all/fold_laws.rs`'s two-writer property;
    and `core/tests/all/dag.rs`.

Law 18 is §7's empty term: `∅` is an identity, and nothing mistakes it for a
number.

18. **`Absent` is the identity of composition** (§7). (a) `Conj(ts ++
    [Absent], p) = Conj(ts, p)` whenever some member of `ts` has a value, bit
    for bit, and `∅` with it when none does; `Conj([])` is 0.5 and
    `Conj([Absent])` is `∅`. An absent gate or offset is none; an absent
    body or term, and `Offset`, `Shift`, `Importance`, `Within` or
    `Periodic` of `Absent`, is `∅`. (b) A term with no `Absent` evaluates
    exactly as it did before `Absent` existed: a value in [0, 1] at every
    instant, and every vector of laws 8, 13, 14 and 16 unmoved. (c) Against
    a store, `link(Ref(x))` is `Absent` where `x` is known and has no
    function, `x`'s function linked where it has one, and `Unknown(x)` where
    no event names `x`; every entry's value is its function so linked and
    read.
    (d) `explain` and the series carry `∅` where a node has no value — the
    root reads what `fulfillment` reads, every `Absent` node is `∅`, an
    absent member's share is 0 — and `Absent` is exact, with no breakpoints,
    so law 7 holds over it. Laws 5, 13 and 14 are checked over generators
    that draw `Absent` among their leaves too. On `core/tests/all/fpl_laws.rs`'s
    and
    `core/tests/all/fold_laws.rs`'s generators; and against
    `conformance/absent/*.py` — `fpl.py`, `series.py` and `view.py` in the
    shapes of `conformance/fpl.py`, `conformance/series.py` and
    `conformance/view/*.py` — SEEDED like the view vectors (§6.7): drawn from
    those same generators under a fixed seed, and regenerated only by hand,
    by `core/examples/generate_absent_vectors.rs`. The frozen view vectors
    (law 9) changed exactly where this law says they must: their unpriced
    rows now read `value='absent'` and `spec='Absent()'`.

Laws 19–29 are §3's change identity and §6's candidates. Each names the
property test under `core/tests/all/` that checks it, on the generators the
laws above draw from, extended to diverging writers, replays, conflicts and
several geneses. `conformance/change.py`, written BY HAND from laws 19–21,
23, 24 and 28 like `link.py` (its written objects derived again under law
36's deps, every read unchanged), pins exact prints and names, a replay that
writes nothing, a twin pair, a conflict priced as its most urgent world, two
geneses united, a snapshot chain and a mixed store over a
`conformance/dag.py` one (`change_vectors.rs`).

19. **A name is its genesis, event and view.** `append(event)` writes a
    function of its genesis, `event`, and the structural heads of `event`'s
    entity. Adding objects of other entities, which do not carry `event`
    (law 20), changes no name. `change.rs::name_ignores_other_entities`.
20. **Append is idempotent.** Appending an event whose print the genesis
    holds writes nothing and answers the first object carrying it. Replaying
    any sequence of appends onto the store they produced leaves its object
    set unchanged, the replay after a rejection included (§3).
    `change.rs::replay_is_a_no_op`,
    `change.rs::replay_after_rejection_writes_nothing`.
21. **Independence is structural.** Every dep is a write to its change's
    todo, so `deps` never leave a todo, changes to different todos are
    concurrent, and a store written todo by todo is the store written in
    time order. `change.rs::deps_name_one_todo`,
    `registers.rs::deps_name_one_todo`.
22. **No stored byte moves.** Every `conformance/dag.py` store, and every
    generated store of `Sealed` and `Woven`, derives, linearises and
    verifies exactly as before, and reads exactly as before wherever no
    register has two candidates. `dag.py`'s `env` changed exactly where law
    24 says it must: at a todo whose state is in conflict, it is the
    candidate set. The existing `dag.rs`, `tips.rs`, `fold_laws.rs` and
    vector suites.
23. **Geneses are disjoint.** Every object has one genesis, and no edge
    crosses geneses. The files of two prodromes united verify clean, and
    fold, and view, to the union of the per-genesis folds keyed by
    `(genesis, todo)`. `change.rs::no_edge_crosses_geneses`.
24. **A conflict prices as its most urgent candidate.** With one candidate
    per register, every reading is as it was, bit for bit. Otherwise an
    entry's value is the least over its worlds. Agreeing twins are one
    candidate. `fold_laws.rs::one_candidate_reads_as_before`,
    `fold_laws.rs::conflict_is_its_most_urgent_world`,
    `registers.rs::agreeing_twins_are_one_candidate`.
25. **The linearisation decides no value.** Folding any linear extension of
    the DAG gives the same entries; only `stream`'s order may differ.
    `fold_laws.rs::any_linear_extension_folds_alike`.
26. **`Least` is a semilattice with `Absent` as identity.** It is
    commutative, associative and idempotent, and `Least([t]) = t`.
    `normalize` and `compile` preserve its reading (laws 5, 13), and it
    round-trips (law 1). `fpl_laws.rs::least_is_a_semilattice`, with `Least`
    drawn by `a_term()` so laws 1, 5, 13 and 14 cover it.
27. **A claim settles nothing it is not trusted to.** A claiming change,
    whatever its deps, leaves every confirmed frontier as it was, and the
    confirmed price under a conflict is the least over binding candidates
    only. `fold_laws.rs::claim_never_settles`.
28. **A snapshot attests its closure.** Its name changes if any object in
    its closure does, `verify` reports an object it attests that the store
    lacks, and on a `previous` chain each closure contains the one before.
    `change.rs::snapshot_commits_to_closure`,
    `change.rs::snapshot_chain_grows`.
29. **Placement is not identity.** Moving a change's file between two stores
    of one genesis changes no name. A change re-proposed over the same
    heads is the same object, and over moved heads it has the same
    `event_id`. Checked against two plain stores, a base and a store staged
    over it: `change.rs::accept_is_a_same_name_move`,
    `change.rs::reproposal_is_recognised`.

Laws 30–32 are §6's register types (`docs/design-register-types.md`, its
laws 2 to 4), on generated values and DAGs, with a state machine in the
tests beside the vocabulary's orders. That §4's registers read byte for
byte as before is law 22 and every vector suite, unchanged.

30. **An order is a partial order.** Each value type's `≤` is reflexive,
    antisymmetric and transitive.
    `order_laws.rs::every_order_is_a_partial_order`.
31. **A reading is the completion.** A register reads the maximal values of
    its frontier's values, each once, and one value exactly when one is
    greatest; under the discrete order on events that is the candidates.
    `order_laws.rs::a_reading_is_the_maximal_values`,
    `order_laws.rs::the_discrete_reading_is_the_candidates`.
32. **An inflationary reading is a homomorphism.** An append to an
    inflationary register whose value is not `≥` the reading it supersedes
    is refused, and over the histories so written `read(h₁ ∪ h₂) =
    max(read(h₁) ∪ read(h₂))`. Through the store's own append, at a schema
    with an inflationary register, the refusal writes nothing.
    `order_laws.rs::an_inflationary_reading_is_a_homomorphism`,
    `review.rs::the_append_refuses_a_move_back`,
    `review.rs::the_reading_of_a_union_is_the_join_of_the_readings`.

Law 33 is the design's law 1, over two schemas: the todo schema and a
second one in the tests (`review.rs`), a machine priced by a flat term per
phase and nothing else, so the law is seen to ask nothing of a schema. The
design's law 5 is law 34, over any `Price`, and law 24 is it for the todo
through the entry; its law 6 is law 22.

33. **A reading is a function of the object set.** Every parents-first
    permutation of a history, every duplication of its objects, and every
    partition into two replicas folded apart and then merged read the same,
    register by register, under any schema. And the route agrees with its
    names: one write joins exactly the registers its schema says it writes,
    and each register type values exactly those writes.
    `schema_laws.rs::a_todo_reading_is_a_function_of_the_object_set`,
    `schema_laws.rs::a_review_reading_is_a_function_of_the_object_set`,
    `schema_laws.rs::a_todo_write_joins_the_registers_it_names`,
    `schema_laws.rs::a_review_write_joins_the_registers_it_names`.
34. **A conflict prices as `Least` over its candidates.** Under any schema
    with a `Price`, a reading whose candidates price nothing has no price,
    one priced candidate is the price written as it, and otherwise the
    price's value at every instant is the least of the candidates' values,
    `∅` the top: below each, and attained. A todo's moment with nothing
    first written and no head is its reading's terms.
    `schema_laws.rs::a_todo_conflict_prices_as_the_least_of_its_candidates`,
    `schema_laws.rs::a_review_conflict_prices_as_the_least_of_its_candidates`.

The design's law 7 is law 35: replicas, wherever each is held.

35. **Sync is the join.** Between any two replicas, on disk or in memory, a
    sync whose wire is cut takes nothing; a sync of a down-set and then a
    whole one leaves the receiver reading what a fresh store reads of the
    union of both directories, a second sync takes nothing, and two
    replicas synced into a third in either order read the same. A store in
    memory answers every append of any history as a store on disk does,
    byte for byte. An overlay reads and appends as a store given its own
    objects and its base's, its base reads as one given only what reached
    it, and its flush leaves the base reading the union.
    `replica.rs::sync_is_the_join`,
    `replica.rs::a_store_in_memory_appends_what_a_disk_store_appends`,
    `replica.rs::an_overlay_reads_as_its_union_until_it_is_flushed`.

Laws 36 and 37 are §3's deps and §6.6's supersession, on two writers whose
replicas in memory sync in between and whose events are dated anywhere.

36. **A change rests on what its writer saw.** Each append descends from
    every write to its entity its writer held, in any register, and from no
    other: happens-before within an entity is what the deps say, for one
    writer and across writers.
    So a writer's own successive writes to one entity stream in the order
    it wrote them, whatever their registers and instants.
    `causal.rs::a_change_rests_on_what_its_writer_saw`,
    `causal.rs::an_add_then_a_complete_stream_in_the_order_written`.
37. **Supersession is per register.** Each register's frontier is its
    writes that no other write TO IT descends from: a write descending from
    a write in another register supersedes nothing there.
    `causal.rs::a_change_rests_on_what_its_writer_saw`.

Law 38 is §7's next change, over generated terms, environments, instants
and observations.

38. **A next change is never late.** The observed value is constant on
    `[now, at)`; where the answer is exact it differs at `at`; it is never
    later than the first change an hourly sampling finds; and it is a
    function of its arguments, so every instant of an exact step answers
    that step's end.
    `observe.rs::every_term_is_constant_until_its_answer`,
    `observe.rs::an_exact_term_answers_its_next_step`,
    `observe.rs::the_answer_is_never_later_than_a_sampled_change`,
    `observe.rs::every_instant_of_a_step_answers_its_end`.

Law 39 is the design's law 10: nests of histories, over generated nests of
two and three levels.

39. **A nest flattens to a history.** Histories keyed by path form a monad:
    `join ∘ unit = id`, `join ∘ fmap unit = id`, `join ∘ join = join ∘ fmap
    join`, `fmap` a functor and `join` natural, `bind` is `join ∘ fmap`;
    reading the joined history at a path where no key begins another is
    reading the history held there; and a join restricted to chosen values
    reads as the parent plus exactly them, as `accept` of a down-set of an
    overlay's objects leaves its base reading as one given exactly those.
    Over histories held in registers, the join holds every object of every
    level once under its name, reading it at a held register's path reads
    as the inner replica does, a pointer reads as the union of its maximal
    writes' heads, an outer write's past reaches an inner object only
    through heads it or a write beneath it named, and the memoised fold of
    the join's algebra is the join and recomputes after a deep write
    exactly the spine it moved.
    `nest.rs::unit_is_a_unit_of_join`, `nest.rs::join_is_associative`,
    `nest.rs::fmap_is_a_functor_and_join_is_natural`,
    `nest.rs::bind_is_the_monads`, `nest.rs::reading_commutes_with_join`,
    `nest.rs::a_restricted_join_reads_as_the_parent_plus_the_chosen`,
    `replica.rs::accepting_chosen_objects_reads_as_the_parent_plus_them`,
    `nest_histories.rs::three_levels_flatten_and_read_as_their_parts`,
    `nest_histories.rs::concurrent_pointers_read_as_the_union_of_their_histories`,
    `nest_histories.rs::the_memoised_fold_recomputes_only_the_spine_a_write_moved`.

Law 40 is a schema declared as data (§6), against its twin written as
code and on generated declarations.

40. **A declared schema is its interpretation.** A declared schema
    equivalent to one written as code prints every event alike, so holds
    the same objects under the same names, and reads every register's
    frontier and maximal writes alike, at every moment, over every
    generated history; through a store it writes, refuses and answers as a
    twin alike, and syncs as any replica. Each declared order is a partial
    order over generated values. A lawful declaration is admitted in its
    canonical print, which parses and prints back the same and is named by
    its hash; a declaration with one defect is refused under exactly the
    law the defect breaks.
    `declared.rs::a_declared_schema_reads_every_history_as_its_rust_twin`,
    `declared.rs::a_declared_store_appends_as_its_rust_twin`,
    `declared.rs::a_declared_reading_is_a_function_of_the_object_set`,
    `declared.rs::every_declared_order_is_a_partial_order`,
    `declared.rs::a_lawful_schema_is_admitted_in_its_canonical_print`,
    `declared.rs::a_broken_schema_is_refused_under_the_law_it_breaks`,
    `nest_histories.rs::a_nest_of_declared_proposals_flattens_and_reads_as_its_parts`.

## 10. Non-goals

A clock in the merge; a second evaluator; a rendering as a source of truth;
multi-tenancy; a hosted service.
