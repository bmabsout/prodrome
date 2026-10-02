# Register types: the history is free, a schema is its reading

Status: design, 2026-10-01; stages 1 to 3, 7 and 8 built (§8). Successor to
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

**What landed.** A valuation is `schema::Price`, one method:

    fn terms(registers: &Self::Registers<'_>) -> Result<Vec<Term>, FplError>;

the term each candidate of the reading prices as, and `fold::price` is
`Least` over them, defined once. `Least` is the meet in the fulfillment
order: its value at every instant is the least of its members', with `∅` as
the top, so a reading none of whose candidates prices is `∅` and an
unpriced candidate never lowers a priced one (law 5). For the todo, a
candidate is a WORLD, one candidate from each register.

What only the todo has is in traits only the schemas that have it
implement, none of them required of a `Price` (§5):

- `History: Price`, a price that changes with the entity's history:
  `moment(now, first, head)`, the terms of one moment, a register unwritten
  yet read as first written and a world that prices nothing priced as the
  head. `fold::flatten` makes each moment a piece of §6.4's function, and a
  moment with nothing first written and no head is the reading's `terms`.
- `Bind`, what FPL's `Ref`s read of an entity in this store
  (`fold::env`), its key a string. A `Ref` to an entity in ANOTHER store is
  a host's environment, passed in, never a method of a schema.
- `Row`, what a list row shows (`Reading`) and when it is provisional
  (`disputes`, `unconfirmed`); `view::entries` asks for `Row + History +
  Bind`.

A schema whose price does not change with its history, that nothing
refers to, and whose store trusts every writer implements `Price` alone.

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
- optionally a VALUATION (§4): `Price`, and where it has them, `History`,
  `Bind` and `Row`.

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

### 6.0 The whole application is a reading

Not only a card: the whole client is one function,

    screen(t) = view(reading(history ∪ intents), ui, t)

over exactly four sources, each a semilattice or a time:

- **history**: the objects synced from the host, a grow-only set (§1);
- **intents**: what the page asked for and the history does not yet hold, a
  grow-only set retracted by subsumption (§6);
- **ui**: the client's own registers (below);
- **t**: the clock. A price is already a behaviour, a function of time (§7):
  the list is the reading sampled at `t`, so a ticking clock re-sorts it and
  nothing refreshes.

**Navigation is a register.** The route is a register of the ui tier;
switching pages or opening an entity is an event that writes it, and the
screen is a different projection of readings the replica already holds:
nothing mounts or loads. History is a log of route events, so "back" moves a
pointer and reads again; a deep link is a route value printed. Selection,
focus, an open sheet, scroll and an unsent draft are registers of the same
kind, folded the same way, and a key binding or a palette entry is an event
mapped to an intent. A transition is a behaviour of time.

**Sync is chosen per register.** Every register is the same type whatever
tier it lives in, so whether it syncs is a property of the register, not of
the mechanism: a route stays on its device, a draft may follow its writer
from one device to another, and "what this device has open" may be a
per-device register another device follows. One writer per device means no
conflict within it; across devices, two opens are two candidates like any
register's.

**Effects are intents, all of them.** A monotone act (complete, edit, add) is
sealed into the client's replica and synced (§6.1); a non-monotone one (send
this message) is a request the host answers with a `Decision` under its lock
(§6.1). Either way the page shows it pending until the history subsumes it,
so no pending state is ever set by hand and none can be left behind.

**The host is the same shape.** Its background work is rules over registers:
"when a reading has property P, perform E under a `Decision`, and record the
outcome" (an answer owed in a conversation, a proposal standing and tapped, a
queue of new work). The rules that need a lock are exactly the non-monotone
ones, as CALM says.

What this removes: per-page loads (a page reads the replica), polling (sync
drives the reading), the class of races between a load and a reload (there
are no loads, only a set that grows), and every pending flag.

**A view is a document over a typed value, so views are data.** Once the
reading is one value of a declared schema (the module's `schema!` answers it),
`view` need not be code in the client's language: it can be a Typst function
of that value, compiled live by `typst-wasm` to HTML. Three things make that
safe whoever wrote the view:

- the compile's world holds only the view's modules and the state, as data
  (text in the state is a string, never markup, and nothing evaluates it);
- the output passes a tripwire that refuses a script, an inline handler or a
  `javascript:` link, so a view cannot act by itself;
- a control declares its intent as data (`data-intent`), and the client parses
  it at the boundary against a CLOSED vocabulary of intents; the non-monotone
  ones still go through a `Decision`.

So a view can do nothing a person's tap could not, and adding a page or a
popup is adding a module, not a build. Which party may write a module (the
application, its user, an agent working for them) is the application's
choice; the structure allows each and prescribes none. Keystroke-rate state
(text being typed, scroll, a stream) stays in client-owned islands, so a
keystroke never recompiles a view.

**Open, for after views are pure: state that is data too.** Views as data
give an agent new pages over the schemas that exist. The next step is an agent
adding STATE of its own: a new register type (its order, whether it is
inflationary) or a new store, appended live and run as a module (wasm, perhaps
a WASI component) beside the schemas the application ships. Admission would
then be a law check rather than a review: the order really is a partial
order, an inflationary register really only climbs, the reading is a function
of the object set, its intents fall in the closed vocabulary. Which laws can
be checked mechanically (by construction, by property test at admission, by a
proof the module carries), and what a module that fails one later costs the
stores built on it, is not yet thought through. Recorded so the pure-view work
does not close the door on it.

### 6.1 Replicas: memory and disk

A store in memory and a store on disk are not two kinds of thing. Each is a
set of objects closed under their parents (§1), so each is a REPLICA of the
same free semilattice, and moving state between them is the join:

    sync(from, to) = to ∪ from

SYNC adopts into `to` every object of `from` that `to` does not hold, each
verified on adoption as any received object is (it hashes to its name, its
parents are held). It is idempotent (an object adopted twice is one object),
order-free (union commutes), and safe to interrupt and repeat, since a
partial sync is a smaller union and the next one completes it. Nothing about
it is a type's business: it never reads a register.

An OVERLAY is a store that writes to one replica (memory) and reads the union
of that replica and another (disk): its readings are the readings of
`memory ∪ disk`, by §1, without copying either. Flushing an overlay is
`sync(memory, disk)`. The replicas that already exist are instances of it: a
browser's replica of a host's store (writes sealed locally, adopted by the
host), and an agent's staging area (a batch is an overlay over its target,
and accepting it is a sync of exactly the objects picked).

**Durability follows the CALM line (§6).** A MONOTONE write (a proposal
asked, a transcript's line, an edit, a page's intent) may stay in memory and
sync lazily: a crash before the sync loses that write and never a reading's
consistency, since the replica that survived is still a down-set. A
NON-MONOTONE decision may not: "is it still standing, so I may perform?" and
the record of the effect must be answered and written by the replica of
record, under its lock, before the effect, or a crash between them lets the
effect run twice. So an overlay is never the store a coordinated decision is
asked of: the type of a store that may answer one is the type of a store
that writes through, and an overlay does not have it.

A third tier never syncs: in-memory registers of the same types over events
too local or too frequent to be objects (a keystroke, a streaming answer).

### 6.2 Caches are tabulations of pure functions

Once a view is a pure function of a reading (§6.0), every cache in the
application is one structure, and it is a prodrome of a particular type.

**A cache has no meaning of its own.** The meaning of `memo f` is `f`
(Elliott's denotational discipline: observing through the cache equals
observing the function). A function `a → b` is isomorphic to a table indexed
by `a`, so memoising is tabulating `f` and indexing the table, filled on
demand (Elliott's functional memo tries). Everything below follows from that
one equation.

**The cache is a partial function, ordered by information.** A finite cache
is an approximation of `f`, ordered by inclusion of graphs: the more pairs it
holds, the more of `f` it knows, and `f` itself is the top (Scott's order). So
a cache is a store whose register per key has the FLAT order (unknown, then
one value) and whose join is union. Because `f` is pure, the union of two
caches is always consistent: two replicas that computed `f(a)` hold the same
`b`. Two different values at one key are not a conflict to show but a
FINDING: `f` was not pure, or its key left out something it read.

**A key names the whole computation.** The key is the hash of the function
(its code and everything that code reads: the view's modules, the
typesetter's version, the fonts) together with the hash of its argument,
which is what a build system's derivation is. The argument is itself a
content-addressed value, so a sub-value has a name and equality is a
comparison of names.

**What it may do that no other store may.**

- An entry may be DELETED. Nothing rests on it (it has no deps, and nothing
  takes it as one), and anything dropped is recomputed on demand. Eviction
  moves the cache down the information order and leaves every meaning where
  it was.
- An entry is VERIFIED by recomputing it, as an object is verified by
  rehashing it, so an entry from an untrusted writer is a claim to check, and
  a trusted one is a shortcut (a binary cache's signature is the same choice).
- A cache stays out of a checkpoint. A checkpoint is the data and the code
  that reads it, and a cache is a function of the two.
- It syncs like any replica (§6.1): what the host computed reaches the
  client by union, and the other way.

**Hierarchical, always, and generically so.** Nothing names levels. Any
value a view reads is a tree (a fixed point of some functor), and made
content-addressed it is a Merkle tree, every subtree named by its bytes, as
the history itself is. A view over it is a fold, and the cache is ONE
combinator, the memoised catamorphism:

    memoCata alg (Fix f) = memo (name (Fix f)) (alg (fmap (memoCata alg) f))

the algebra memoised at every node, keyed by that node's name. So the cache
has the data's own shape, whatever the data is: a change recomputes the
spine from the changed subtree to the root and every other node is a hit,
with no change tracked and no level chosen. A cache only at the root is all
or nothing, and a cache only at the leaves rebuilds everything above them;
this is neither, and it is the same for a list, a page, a conversation or a
term. Where a node holds a long sequence, the sequence is itself a balanced
tree (a measured finger tree), so the spine through it is logarithmic. The
output is a tree of the same names (a keyed DOM, a node per name), so
placing a redraw also touches only the spine.

So a cost is EXPLAINED, not only measured: a redraw reports, per node and by
depth, what hit and what was recomputed, and its cost is the work along the
spines that moved. A number that does not follow from those counts is a
finding (a name that covers too much, so too much misses, or too little, so a
hit is wrong).

**Time is an input like any other, keyed by what is observed.** A cache keyed
by the instant never hits. But a view shows a behaviour's value at a
precision (a whole percent, a pie's drawn angle), so it is a function of the
OBSERVED value, `view ∘ observe ∘ v`, and is keyed by that. A behaviour that
is an explicit function of time (a fulfillment term) can also answer when its
observed value next changes (push-pull reactivity: a behaviour knows its next
discontinuity), so a client redraws exactly at those instants instead of on a
ticking clock.

**What landed.** `prodrome::memo` (stage 8, §8), which depends on the hash
type and on nothing else of the crate.

- **The cache as a store.** `memo::Cache` holds (key, value) pairs, a
  `memo::Key` being a function's name and its argument's. A key reads as a
  miss, a hit or a FINDING (`Lookup::Finding`, every value it holds), so
  the cache is the free structure on its pairs, read as a partial function
  where it is one. `join` is union and answers each finding it makes;
  `insert` does the same for one pair. Any entry may be removed, and a
  `memo::Evict` policy decides which go: `Keep` drops none, `Lru` keeps a
  bounded number. It is not an `EventStore` and holds no objects, on
  purpose: an object is never deleted, is verified by rehashing and
  belongs in a checkpoint, an entry is none of these, and with no type in
  common no entry can be received into a history or an object evicted from
  one. It syncs as a replica does, by `join`, and a host sends
  `Cache::pairs`.
- **The memoised fold.** A `memo::Tree` is a node with a content name and
  children; a `memo::Algebra` has a name and a step from one layer (the
  node, its children's results) to a result. `memo::fold` runs the step
  bottom-up, memoised at every node by (the algebra's name, the node's
  name), and never enters a subtree whose top hits. Its `memo::Report`
  says, per node and by depth, what hit, what was computed and which keys
  held two values (computed, and neither used). `memo::name` is the name a
  node must have: its own bytes, length-prefixed, and its children's
  names in order, so a change renames exactly its spine.
- **A long sequence.** `memo::balance` builds a sequence into a balanced
  tree of the host's own chunk nodes, each named over its children and
  holding their measure. A run of nodes closes after a node whose name
  ends a chunk (one in four) or at sixteen, so the shape is a function of
  the elements alone: two replicas holding one sequence build one tree and
  share its cache, and an edit renames a logarithmic spine.

No wasm export. The algebra is a host's code, so a fold in the module
would call back across the boundary at every node it computes, and what
the boundary would carry (a tree of JSON and a name per algebra) is the
host's to define. The first host is a client's view, in the host's own
language or in `typst-wasm`'s workspace, and an export is added when one
reads a cache through the module.

### 6.3 A prodrome of prodromes is a prodrome

A register's value may itself be a prodrome: a conversation inside the store
of conversations, a card's or a popup's local state inside the page's, an
agent's batch beside the chain it targets, a plugin's store inside its user's.
Histories nest, and a nest of histories FLATTENS into one history. That is a
monad, and it is the shape local state takes.

- **Unit.** An event is the history of one object.
- **Join**, `P (P e) → P e`. An outer history holds an inner one by its heads
  (a pointer, as a git tree holds a subtree by its name). Join replaces the
  pointer with the objects it names, each keyed by the PATH of the register
  that held it. Content names are global, so no inner object is renamed and
  no dep is rewritten; only the key gains a prefix.
- **Why the laws hold.** Keys are paths, and paths under concatenation are the
  free monoid; the objects form the finite down-closed powerset (§1), whose
  join is union of unions. Unit and associativity are those two structures'
  laws: flattening three levels gives the same history in either order, as
  paths `a/(b/c)` and `(a/b)/c` are one path.
- **Reading commutes with join.** Reading the flattened history at a path is
  reading the inner history it came from, so nothing nested reads
  differently once flattened, and a reader never needs to know which form it
  holds.
- **Bind** gives every value of a history its own sub-history: `h >>= f` is
  local state per entity (each todo's draft, each conversation's lines),
  flattened into one history keyed by entity.

**Local state is a nested prodrome.** A component of a client (a page, a card,
a popup, a view an agent wrote) owns a sub-history in the in-memory tier
(§6.1). It is read like any store, its registers sync or not as each declares
(§6.0), and it is undone and travelled through in time like the rest,
because after join it IS the rest. Nothing is kept beside the reading at any
level.

**Accepting is joining; discarding is not.** Taking a sub-history into its
parent is join restricted to the objects chosen: an agent's batch accepted
or rejected object by object, an overlay flushed to the replica of record
(§6.1), a plugin's store admitted after its laws pass, a draft committed.
Discarding an in-memory sub-history removes objects nothing outside rests on,
which is why it may.

**Causality crosses levels only through pointers.** An inner object's deps are
inner. An outer write that depends on inner state names the inner heads it
saw, as a change names its entity's heads (§3's change identity), so
happens-before between levels is exactly what the pointers say.

**And the rest composes.** A prodrome of prodromes is a tree of
content-addressed histories, so the memoised fold (§6.2) folds it with no
further mechanism, and a client built this way is Elm's architecture with
three changes: the model is the reading of a history that only grows (time
travel, undo and sync are not extras), views are data loaded at run time
(§6.0), and nested components compose by join instead of by hand-written
message plumbing.

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
7. **Sync.** `sync` is idempotent and order-free, an interrupted sync
   completes on the next, and a reading after `sync(a, b)` equals the
   reading of `a ∪ b`; an overlay reads as its union, and its flush leaves
   the replica of record reading that union.
8. **Memo.** Reading through a cache equals computing (`memo f = f` on
   generated arguments, with entries evicted at random between reads); a
   key's two values are reported, never chosen; a fold memoised at every
   node (`memoCata`) equals the plain fold, over generated trees of any
   shape; and a redraw's reported recomputes and hits are exactly the nodes
   whose names did and did not move.
9. **Next change.** For a behaviour that answers its next observed change at
   `t₁` after `t₀`, the observed value is constant on `[t₀, t₁)` and differs
   at `t₁`.
10. **Nesting.** Over generated nested histories: `join ∘ unit = id` and
    `join ∘ fmap unit = id`; `join ∘ join = join ∘ fmap join`; reading the
    joined history at a path equals reading the inner history there; and a
    join restricted to chosen objects reads as the parent plus exactly those
    objects.

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
   `fold::RegisterType`; a valuation, since narrowed to `schema::Price`, its
   price `Least` over a reading's candidates by construction (`fold::price`),
   beside `History`, `Bind` and `Row` for what only the todo has (§4);
   `EventStore`, `Dag`,
   `Envelope`, `Change`, the fold, `registers`, `verify`, `Policy` and
   `view::entries` generic over a schema, the append calling `grows` through
   `Product::grows`. A review schema in the tests, priced by a flat term per phase, runs
   law 1 beside the todo's and law 4 through the append (SPEC laws 32 and
   33); law 5 is SPEC law 34 over any `Price`, the review schema priced by a
   flat term per phase, and law 24 for the todo's entry; law 6 is law 22,
   every vector unchanged.
3. **The wasm.** The exports read a store at a schema. BUILT: the exports
   are generic over a schema (`prodrome-wasm-exports`, a library): `verify`, `tips`
   and each prodrome's heads, `since` a replica's tips, and `readings`, each
   register's maximal writes through `fold::Product::reading`, for any
   schema; `entries` and `prices` for one with a `Row`, a `History` and a
   `Bind`. A schema
   crosses the boundary by `Json` beside it (its key's field, its
   registers' names, its values' JSON, a function of the value) and, priced,
   `RowJson` (its row's reading's JSON); not a bound on `Schema`, since
   JSON is the boundary's and a schema read natively owes it nothing. A host
   says `prodrome_wasm_exports::schema!(Name = Schema)`, with `, priced`
   for those three, once per schema, each a JS class in ONE module, its own
   `cdylib` crate. `prodrome-wasm` is the reference module, built the same
   way: the todo schema at the reference payload, `Todos`, beside its old
   exports, byte for byte; the tests' review schema runs through the
   same macro over a small store.

In a host (the first one), after stage 2:

4. **The inbox as a proposal schema**, its machine ordered as in §3,
   inflationary, valued by what it serves; the existing objects migrated
   once, on a copy first, with every proposal's reading pinned.
5. **Conversations as a conversation schema** (a transcript as a grow-only
   set in causal order; owed as a reading).
6. **The whole client as a reading** (§6.0): every store a replica on the
   client, intents as one set, the ui (the route first) as registers, the
   clock as a source; pages as pure views with no loads; then the host's
   background work as rules over registers.

In this repository, after stage 3:

7. **Replicas.** `sync(from, to)` between any two stores at one schema, an
   in-memory store, and an OVERLAY that writes to memory and reads
   `memory ∪ disk`, with its flush. A store that answers a coordinated
   decision (a lock, a write-through append) is a type an overlay is not.
   Laws: sync is idempotent and order-free, an interrupted sync completes on
   the next, a reading after sync equals the reading of the union, and an
   overlay reads as its union. An agent's staging area is its first host
   use (a batch as an overlay over its target, accepted by a sync of the
   objects picked), replacing a copy of the target and a deletion.

   BUILT. First the in-memory replica behind a store: an
   `EventStore` holds every object it has read, each verified once, with
   their tips and their fold (`store::memory`), and each look at the disk
   replica is a sync into it of exactly the objects it lacks: a name
   listed and not held is read and verified, a held name no longer listed
   is forgotten, and a held file whose `stat` changed is read again. The
   fold follows by `registers::Folded::insert`, which puts an object where
   the linearisation of the union puts it, so the memory is the fold of
   the object set whatever order the objects arrive in, never rebuilt from
   the files. Laws, `core/tests/all/memory.rs`: a store read
   incrementally reads as the same objects read cold, and an append
   through the memory writes byte for byte what a cold one does.

   Then the replicas. `store::Replica` is what every store is: it reads
   (`Held`: its objects, their fold, its tips), appends, gives the print
   held under a name, and receives, verifying every object before it takes
   any, parents first (`memory::receive`, the walk every adoption shares).
   `store::sync(from, to)` is `to.receive(from.tips())`, one function for
   every pair. What an append writes is `Memory::change`, decided on what
   is held, so `store::MemoryStore`, objects and prints in memory, writes
   byte for byte what an `EventStore` holding the same objects writes.
   `store::Overlay` writes to memory and reads the base's memory (its
   objects and fold, shared) with its own objects admitted over it, taken
   again when the base changes; `Overlay::flush` is `sync(overlay, base)`.
   The coordinated decision is `store::Decision`, which only
   `EventStore::decide` makes: the lock held, the memory level with the
   directory, and an append written through. Two compile-fail doctests pin
   that an overlay and a store in memory have none. The wasm exports'
   classes gain `append(event, genesis)`, which seals an event into the
   page's replica through a `MemoryStore` and answers the object to send
   the host. Laws, `core/tests/all/replica.rs` (SPEC law 35): sync between
   disk and memory in all four pairings is idempotent and order-free,
   completes when cut short, and reads as the union; a store in memory
   appends as a disk store does over any history; an overlay reads as its
   union and its flush leaves the base reading it.
8. **Caches** (§6.2). A cache as a store of (key, value) pairs in the flat
   order per key, joined by union, a key's two values a finding; eviction
   by any policy, one bounded LRU shipped; and the memoised fold over a
   content-addressed tree of any shape, memoised at every node and
   reporting what hit and what was computed; a long sequence as a
   balanced, measured tree. Law 8.

   BUILT: `prodrome::memo` (§6.2's "What landed"). Laws,
   `core/tests/all/memo.rs`: a join is union (commutative, associative,
   idempotent), a key holding two values reads as a finding with both and
   never as either, an LRU cache holds its bound. `memo_fold.rs`, over
   lists, binary trees and rose trees and generated edits (a node
   relabelled, a subtree grafted): the memoised fold is the plain fold,
   two algebras sharing one cache, entries dropped at random between folds
   and under a small LRU; after an edit the fold computes exactly the spine
   from the edited subtrees to the root, each node once, and hits exactly at
   the top of each unchanged subtree beside it; with subtrees shared, it
   computes exactly the names it had not seen; two caches joined fold as
   either; a forged second value is reported and the node computed.
   `memo_balance.rs`: a balanced sequence folds to the sequence and its
   measures index it, its shape is its elements, and an edit computes a
   logarithmic number of nodes where a list computes all before it.

## 9. Non-goals

- A last-writer-wins register. Time is data (§1): no value order is a clock.
- Operation-based types that need delivery order. Every type here reads a
  set; the DAG already carries causality.
- Merging values a type does not order. Incomparable concurrent values are a
  conflict, shown, and a later write that descends from both settles it.
