# Register types: the history is free, a schema is its reading

Status: design, 2026-10-01; stages 1 and 2 built (§8). Successor to
`design-change-identity.md`, whose frontier this names as the universal
part.

## 0. Why

The database holds one vocabulary. §4 says it outright: "the six kinds above
are the DATABASE's", and §6 reads them into four fixed registers (state,
spec, content, tendings), with FPL as the only valuation. A host whose
entities are not todos fakes being one. Three fakes are visible in the one
host that exists:

- **An inbox of proposals.** A proposal is a state machine (standing, then
  sent, failed, may-have-gone, checked, dismissed, superseded). Stored as a
  todo, its outcome is JSON in a `Completed`'s note, a dismissal is a
  `Cancelled`, an attempt is a `Tended`, and a proposal that serves nothing
  carries an FPL price of `Absent`: a valuation the type does not have.
- **A conversation.** A transcript is a grow-only set of lines in causal
  order. Stored as a todo, every line is a content record on one register,
  and "owed an answer" is derived by hand.
- **A page.** A card's "pending" flag was a state machine nothing tied to the
  store. A lost answer left it pending forever, though the store held the
  outcome.

Each fake is the same missing abstraction: what a register IS, given by its
type, with the database owning only what every type shares.

## 1. The history is the free join-semilattice

A replica holds a set of content-addressed objects closed under their parents
and deps (§3): a DOWN-SET of the causal order. Merging two replicas is set
union. Union is associative, commutative and idempotent, so:

- every replica that has seen the same objects is the same replica
  (convergence), whatever the order and however often each arrived;
- adopting an object twice is adopting it once, by identity, because its name
  is its bytes;
- a duplicate request is one object (`Change` replays write nothing, §3).

These hold for every type, because no type is consulted. Nothing above this
layer re-proves them. This is the universal construction: the free
join-semilattice on the objects a host writes, with union as its join.

## 2. A register reads its frontier

A schema routes each event to the registers it writes, with a value for
each. A register's reading at a moment, under a policy (§5), is its
FRONTIER: the binding writes no other binding write to it descends from.
Frontiers ordered by causal domination form a lattice, and reading the
frontier commutes with union:

    frontier(h₁ ∪ h₂) = frontier(h₁) ⊔ frontier(h₂)

So every register gets a join-semilattice for free, and a frontier with more
than one write is not an error: it is an element of that lattice with more
than one maximal write. This is today's `fold::Frontier`, and today's
`Register` trait already says the rest: a fold is a semilattice of writes.

**Causal order decides succession; nothing else does.** A write supersedes
exactly the writes it descends from. No clock, no name order and no value
order ever decides that one write replaced another. §3 and §6 stay true.

## 3. A type supplies an order; the join is free

What a register's type supplies is a PARTIAL ORDER on its values, `≤`. It
decides nothing about succession (§2). It decides only which concurrent
values are redundant: the register's reading is the set of MAXIMAL values
among its frontier's values,

    read(r) = max≤ { value(w) : w ∈ frontier(r) }

which is the free join-semilattice completion of the order (finite
antichains of the poset, ordered by domination). One maximal value is a
VALUE. More is a CONFLICT, shown whole as today. The type never writes a
join; the completion is the join.

- **Discrete order** (`a ≤ b` iff `a = b`). Every distinct value of the
  frontier is a candidate: exactly today's `candidates()`, twins counted
  once. The todo's state, spec and content registers are discrete.
- **Total order.** The reading is always one value, the greatest. A register
  that may only grow (a version counter, a high-water mark) is total.
- **A state machine** ordered by "further along". A proposal's machine is
  `standing < failed < sent`, `standing < unknown < checked`,
  `standing < dismissed`, `standing < superseded`. A retry that sent after a
  failed attempt reads `sent` (failed ⊔ sent = sent, since failed < sent).
  `sent` and `dismissed` are incomparable, and that is the one real conflict:
  one device sent while another dismissed. The order puts the conflict where
  it is true and nowhere else.
- **Sets under inclusion.** The grow-only set (the tendings, a transcript's
  lines) is the order `⊆` with union as its completion's join.

**Convergence needs no law from the type.** The reading is a function of the
object set (§1), for any partial order. A type that supplies a wrong order
reads wrongly; it cannot make two replicas disagree.

### 3.1 Inflationary registers

A type MAY declare its register INFLATIONARY: every write's value is `≥` the
values of the writes it supersedes. When it is, the reading itself is a
semilattice homomorphism,

    read(h₁ ∪ h₂) = read(h₁) ⊔ read(h₂)

and the register can be read incrementally, coordination-free (§6). The
declaration is not trusted. The append path enforces it: an append whose
value is not `≥` the reading it supersedes is refused, by the store's smart
constructor, before any object exists.

The proposal machine is inflationary. The todo's state register is not
(`Reopened` after `Completed` goes back), which is precisely why todos need
shown conflicts and a valuation of them (§4), and why the reading of a todo's
state is never incremental in the strong sense. Nothing is lost by that: the
frontier is still free.

## 4. A valuation is optional

A schema MAY value its entities: a map from an entity's reading to an FPL
`Term`, its price (§7). For a register in conflict, the valuation of the
antichain is `Least` over its members' valuations: the meet in the
fulfillment order, "price a conflict as its most urgent world". That is
today's rule, now stated as what it is: the valuation's own meet, applied to
an antichain. It is not the register's join.

An entity whose schema has no valuation has NO price. It is not `Absent`.
`Absent` stays a value of FPL meaning "this todo prices no opinion"; it
stops standing in for "this type has no price".

**Two operations, two laws.** The completion's join merges CONCURRENT VERSIONS
of one register: idempotent, since a value joined with itself is itself.
FPL's conjunction (`Conj`, the power mean, §7) AGGREGATES DIFFERENT ENTITIES:
a commutative operation that is deliberately not idempotent (a `Flat(1.0)`
member dilutes, law-tested). Calling both "the connective" would hide that
they obey different laws. A schema chooses its order; FPL's conjunction is
not a register's business.

## 5. A schema

A host declares a schema, and the database is generic over it:

- an EVENT VOCABULARY: closed constructors, their frozen fields and their
  nested vocabulary, parsed by §2's whitelist walker and printed canonically,
  exactly as a payload is today;
- an ENTITY KEY each event names (a todo, a proposal, a conversation);
- its REGISTERS, a product: each a name, a value type with its order, whether
  it is inflationary, and the route from each event to the registers it
  writes with what value;
- optionally a VALUATION (§4).

The registers of an entity are a product of semilattices, so they are a
semilattice; the fold of a stream is a monoid action (`extend`), as §6.6
already states. Standing (§5) is unchanged: a policy answers per event, the
confirmed reading folds what binds, the claimed reading folds everything,
and both stay in the database.

**The schema is the store's type, not a field of an object.** A host opens a
store at a schema, as it opens one at a payload today, and the schema is the
parse: an event the schema's vocabulary does not name is refused at the
boundary. No object carries its schema, so `Genesis(label, nonce)` stays
frozen. A host may say in a genesis's label what a prodrome is for.

**The todo schema is the reference schema.** §4's six kinds and the record
kind, routed into state (discrete), spec (discrete, valued by FPL), content
(discrete) and tendings (grow-only), with FPL's valuation and `Least`. Every
legacy envelope belongs to it. Every conformance vector reads byte for byte
as before: the generalisation must be invisible to the one schema that
exists.

## 6. The application is the same structure (FRP)

A page is a reading too: a fold over the replica's history ∪ the page's
INTENTS. An intent is a write the page has asked the box for and not yet seen
in the history. Intents form a grow-only set, and an intent leaves the view
once a history write SUBSUMES it (the box's record of the act). So a view is
a monotone function of two semilattices, and drawing it is idempotent.

A lost answer cannot freeze such a view: the pending state is an intent, and
the replica's next sync either holds the outcome (the intent is subsumed) or
does not (the intent is still visible, and asking again is safe because the
effect is memoised by its hash, §1).

**CALM names where coordination is needed.** Monotone readings, the readings
of inflationary registers and grow-only sets, can be computed
incrementally and need no coordination. A NON-MONOTONE question needs a
point of coordination: "is this proposal still standing, so I may send?"
is a negation over the history, and the host answers it under a lock, once.
The design predicts exactly the one lock the host found it needed, and every
new non-monotone question is a lock to name, never a race to discover.

What stays out of the store: events too frequent or too local to deserve an
object (a keystroke, a streaming answer's text). They are in-memory registers
of the same types, never content-addressed.

## 7. Laws

Each is a property test over generated histories, in the core's `tests/`:

1. **Free.** A reading is a function of the object set: every permutation,
   duplication and partition-then-merge of a history reads the same.
2. **Order.** Each value type's `≤` is reflexive, antisymmetric and
   transitive, over generated values.
3. **Completion.** A reading is the set of maximal values of the frontier's
   values; it is one value exactly when one is greatest.
4. **Inflationary.** For a register declared inflationary, an append whose
   value is not `≥` the reading it supersedes is refused, and
   `read(h₁ ∪ h₂) = read(h₁) ⊔ read(h₂)`.
5. **Valuation.** A conflict's price is `Least` over its candidates' prices.
6. **Reference.** Under the todo schema every existing vector reads
   unchanged (the successor of law 22).

## 8. Stages

In this repository, each one PR:

1. **Order and completion.** `Order` on values (a `Discrete<T>` default), the
   completion over `Frontier`, the inflationary refusal, laws 2 to 4. The
   todo registers are discrete; law 6 holds. BUILT: `fold::Order`,
   `fold::maximal` and `Frontier::read`, `fold::Inflationary` and
   `fold::grows` (called by the store from stage 2, when a schema has an
   inflationary register that supersedes); SPEC §6 and laws 30 to 32.
2. **Schema.** The trait (vocabulary, key, registers as a product, route,
   valuation), the todo schema as its reference instance, stores and folds
   generic over it, `view` over a schema with a valuation. Law 1 and law 6.
   BUILT: `schema::Schema`, implemented by the event type itself, so a
   schema's events are a closed sum and nothing else parses into one
   (`TodoEvent<P>` is the todo schema, in `todo`); an entity's registers as
   a `fold::Product` of `Frontier`s and grow-only sets, each read under a
   `fold::RegisterType`; `schema::Valuation`, its price `Least` over a
   reading's worlds by construction (`fold::price`); `EventStore`, `Dag`,
   `Envelope`, `Change`, the fold, `registers`, `verify`, `Policy` and
   `view::entries` generic over a schema, the append calling `grows` through
   `Product::grows`. A review schema in the tests, with no valuation, runs
   law 1 beside the todo's and law 4 through the append (SPEC laws 32 and
   33); law 5 is SPEC law 24 and law 6 is law 22, every vector unchanged.
3. **The wasm.** The exports read a store at a schema.

In a host (the first one), after stage 2:

4. **The inbox as a proposal schema**, its machine ordered as in §3,
   inflationary, valued by what it serves; the existing objects migrated
   once, on a copy first, with every proposal's reading pinned.
5. **Conversations as a conversation schema** (a transcript as a grow-only
   set in causal order; owed as a reading).
6. **A page as a fold over history ∪ intents**, the proposal card first.

## 9. Non-goals

- A last-writer-wins register. Time is data (§1): no value order is a clock.
- Operation-based types that need delivery order. Every type here reads a
  set; the DAG already carries causality.
- Merging values a type does not order. Incomparable concurrent values are a
  conflict, shown, and a later write that descends from both settles it.
