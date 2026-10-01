//! Prodrome in the browser — the same core, on the reader's machine.
//!
//! The web app's claim has always been that its numbers are the server's,
//! computed once by one evaluator (SPEC §1). That left the browser doing two
//! semantic things of its own: rehashing the chain objects, and drawing
//! straight lines between knots. This crate takes the FIRST of those out of
//! TypeScript and makes the second checkable, by compiling `prodrome-core`
//! itself to WebAssembly. The browser now runs the core; it still never has a
//! second evaluator.
//!
//! THIN ON PURPOSE. Every function here is: parse the arguments, call one
//! thing in `prodrome`, print the answer. No arithmetic, no policy, and no
//! shape that `view.py` does not already put on the wire. Anything
//! that needs a decision belongs in the core, where the conformance vectors
//! can see it.
//!
//! Strings and JSON at the boundary (see `wire.rs` for why), so the whole
//! surface is:
//!
//! | function         | asks                                                    |
//! | ---------------- | ------------------------------------------------------- |
//! | `verify_objects` | §3: do these bytes hash to these names, form one DAG, and end at which tips |
//! | `lifecycle`      | §4: spell one lifecycle event, through its own `mk_*`    |
//! | `seal`           | §3: a legacy store's write onto these heads — the name and the bytes |
//! | `merge_object`   | §3: a legacy store's join of these heads — no event, no actor |
//! | `fold`           | §6.1–6.5: what does the chain believe at an instant      |
//! | `registers`      | §6.6: which registers have more than one live write      |
//! | `entries`        | §6.7: every todo as the folds see it, composed ONCE      |
//! | `fulfillment`    | §7: what is this term worth now                          |
//! | `explain`        | §7: what is that number made of                          |
//! | `series_knots`   | §7 knots: what is that term's curve over a window        |
//! | `link`           | §7.2: every ref in a term bound to the todo it names     |
//! | `term_json`      | §2 → §7: a stored term's print, as the JSON shape above  |
//!
//! `store` is file-backed and a browser has no `events/` directory, so the
//! objects arrive over `/api/chain` instead and are read as a
//! [`prodrome::dag::Dag`]: [`verify_objects`] is its findings, asked of a set
//! in memory.
//!
//! NO CLOCK, anywhere below. §1 forbids one in the core, and a browser's clock
//! is the least trustworthy in the system; every moment is an argument — the
//! `at` a replica seals included, which is why [`lifecycle`] takes one rather
//! than reading one.
//!
//! ⚠️ SINCE STAGE 3 THIS CRATE ALSO WRITES, and it is worth saying why that is
//! not a widening. `seal` and `merge_object` build an ENVELOPE and hand back
//! its name and its bytes; they touch no store, because a browser has none.
//! Every rule about what an object may be is still the core's `mk_*`
//! constructors, and the box rehashes and re-verifies everything it is given
//! anyway on the way in. What the browser gains is the ability to
//! name a value the same way the box would — which is the one thing a replica
//! cannot do without §2's printer, and the one thing a second printer in
//! TypeScript would have got subtly wrong.

use std::collections::{BTreeMap, BTreeSet};

use prodrome::dag::{Dag, Finding, Unread};
use prodrome::event::{parents_of, Envelope, Hash, TodoEvent};
use prodrome::fold::{self, Product};
use prodrome::fpl::{datetime_of, instant_of, iso, print_term, scalars, Candidates, Instant};
use prodrome::literal::Datetime;
use prodrome::policy::{Everything, Policy};
use prodrome::registers;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;

mod json;
#[cfg(test)]
mod snapshot;
mod wire;

use wire::{
    json_candidates, json_entry, json_env, json_marker, json_reading, json_record, json_tended,
    json_terms, object, parse_closed, parse_env, parse_instant, parse_moment, parse_objects,
    parse_specs, parse_term, parse_untrusted, strings, History, ObjectIn, Refusal,
};

/// THE RECORD SHAPE THIS MODULE WAS BUILT WITH.
///
/// §4's record kind is the host's (`prodrome::payload::Payload`), and a store
/// is parsed against one closed vocabulary — so a `.wasm` is built for one
/// payload, and this is the choice. `prodrome::reference::Todo` is the shape
/// `conformance/*.py` was taken with; a host with its own payload compiles
/// its own wasm from this crate with the type swapped, and every export below
/// is written so that is the ONLY line that changes.
type Record = prodrome::reference::Todo;

type Event = TodoEvent<Record>;
type Object = Envelope<Event>;
type Node = registers::Node<Event>;

/// A refusal, as the exception a JS caller catches. Every entry point returns
/// one rather than panicking: a browser that aborts inside the Wasm leaves the
/// module poisoned for the rest of the session, and the SubtleCrypto fallback
/// can only run if the failure arrived as a value.
fn refused(reason: Refusal) -> JsError {
    JsError::new(&reason)
}

fn printed(value: &Value) -> Result<String, JsError> {
    serde_json::to_string(value).map_err(|e| refused(format!("could not print the answer: {e}")))
}

/// The name these bytes have: sha256 of the canonical print, exactly what
/// `seal_hash` takes and what `events/objects/<name>.py` holds — UTF-8, no
/// trailing newline.
fn name_of(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The crate version, so a page can say WHICH core answered it.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

// --- §3: verify --------------------------------------------------------------

/// One object's row on the chain page: what it claims, what this core
/// computed, and where the walk found it.
struct Row {
    hash: String,
    computed: String,
    parents: Vec<Hash>,
    kind: String,
    todo: String,
    actor: String,
    at: String,
    problem: String,
    reachable: bool,
    depth: Option<usize>,
    root: bool,
}

impl Row {
    fn blank(hash: String, computed: String) -> Row {
        Row {
            hash,
            computed,
            parents: Vec::new(),
            kind: String::new(),
            todo: String::new(),
            actor: String::new(),
            at: String::new(),
            problem: String::new(),
            reachable: false,
            depth: None,
            root: false,
        }
    }

    fn hash_ok(&self) -> bool {
        self.hash == self.computed
    }

    fn tampered(&self) -> String {
        format!(
            "does not hash to its own name — this browser computed {}",
            self.computed
        )
    }

    /// The four fields `json_link` puts on the wire, read from the object's own
    /// bytes; an object with no event leaves them "" and is kinded by its name.
    fn describe(&mut self, envelope: &Object) {
        self.parents = parents_of(envelope);
        self.root = matches!(
            envelope,
            Envelope::Genesis(_) | Envelope::Sealed { prev: None, .. }
        );
        match envelope.event() {
            Some(event) => {
                self.kind = event.kind_name().to_owned();
                self.todo = event.todo().as_str().to_owned();
                self.actor = event.actor().as_str().to_owned();
                self.at = iso(instant_of(event.at()));
            }
            None => self.kind = envelope.name().to_owned(),
        }
    }

    fn json(&self) -> Value {
        json!({
            "hash": self.hash,
            "computed": self.computed,
            "hashOk": self.hash_ok(),
            "reachable": self.reachable,
            "depth": self.depth,
            "kind": self.kind,
            "todo": self.todo,
            "actor": self.actor,
            "at": self.at,
            "problem": self.problem,
        })
    }
}

/// §3's `verify`, for the set of objects a browser was handed.
///
/// `objects` is `[{hash, text}]` — every object's claimed name beside the exact
/// canonical print that name is a hash of. They are read as one
/// [`prodrome::dag::Dag`], and the answer's `tips` are its tips: nobody has to
/// tell a replica where its heads are.
///
/// The findings are the `Dag`'s, less the ones about a DIRECTORY, which a set
/// in memory has none of. A page's own wording stands where it had one: an
/// object whose bytes do not hash to its name, and a parent that was not sent.
/// The walk from the tips gives each row its depth, and what it does not
/// reach (only an object that is not one, or one caught in a cycle) is said.
#[wasm_bindgen]
pub fn verify_objects(objects: &str) -> Result<String, JsError> {
    let sent = parse_objects(objects).map_err(refused)?;

    let mut problems: Vec<String> = Vec::new();
    let mut rows: Vec<Row> = Vec::new();
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    let mut prints = Vec::new();
    for ObjectIn { hash, text } in &sent {
        if index.contains_key(hash) {
            problems.push(format!("object {hash} was sent twice"));
            continue;
        }
        index.insert(hash.clone(), rows.len());
        let mut row = Row::blank(hash.clone(), name_of(text));
        match Hash::new(hash.clone()) {
            Ok(name) => prints.push((name, Ok(text.as_bytes().to_vec()))),
            Err(_) => {
                row.problem = row.tampered();
                problems.push(format!("object {hash} {}", row.problem));
            }
        }
        rows.push(row);
    }
    let dag: Dag<Event> = Dag::from_prints(prints);
    for (name, object) in dag.objects() {
        rows[index[name.as_str()]].describe(object);
    }
    for finding in dag.verify(&Everything) {
        match finding {
            Finding::Unread { name, why } => {
                let row = &mut rows[index[name.as_str()]];
                row.problem = match why {
                    Unread::Tampered(_) => row.tampered(),
                    _ => format!("failed to parse: {}", why.refusal(&name)),
                };
                problems.push(format!("object {} {}", name.as_str(), row.problem));
            }
            Finding::Broken { at, why: None } => problems.push(format!(
                "object {} is named as a parent but was not sent",
                at.as_str()
            )),
            Finding::Broken { .. } => {}
            other => problems.push(other.to_string()),
        }
    }

    let tips: Vec<String> = dag
        .tips()
        .into_iter()
        .map(|tip| tip.as_str().to_owned())
        .collect();
    // Breadth first from every tip at once, so `depth` is the distance from
    // the nearest tip and the order stays tips-first the way the page reads
    // it.
    let mut order: Vec<usize> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut queue: Vec<(String, usize)> = tips.iter().map(|name| (name.clone(), 0)).collect();
    let mut cursor = 0;
    while cursor < queue.len() {
        let (name, depth) = queue[cursor].clone();
        cursor += 1;
        let Some(at) = index.get(&name).copied().filter(|_| seen.insert(name)) else {
            continue;
        };
        rows[at].reachable = true;
        rows[at].depth = Some(depth);
        order.push(at);
        for parent in rows[at].parents.clone() {
            queue.push((parent.as_str().to_owned(), depth + 1));
        }
    }

    // A genesis object says so itself: an object that did not parse names no
    // parents either, and calling that a beginning would turn a tampered tip
    // into a clean chain of one. A change with no deps is no beginning.
    let genesis: Vec<String> = order
        .iter()
        .filter(|at| rows[**at].root && rows[**at].problem.is_empty())
        .map(|at| rows[*at].hash.clone())
        .collect();
    if genesis.is_empty() && problems.is_empty() && !rows.is_empty() {
        problems.push(
            "the walk ended without reaching a genesis object (an object with no parents)"
                .to_owned(),
        );
    }

    let linearisation: Vec<String> = dag
        .linearise()
        .unwrap_or_default()
        .iter()
        .map(|name| name.as_str().to_owned())
        .collect();

    let orphans: Vec<usize> = (0..rows.len()).filter(|at| !rows[*at].reachable).collect();
    for at in &orphans {
        problems.push(format!(
            "object {} is not reachable from any head",
            rows[*at].hash
        ));
        if rows[*at].problem.is_empty() {
            rows[*at].problem = "not reachable from any head by its parents".to_owned();
        }
    }

    let listed: Vec<usize> = order.iter().copied().chain(orphans).collect();
    let verified = listed.iter().filter(|at| rows[**at].hash_ok()).count();
    printed(&object(vec![
        ("tips", strings(tips)),
        (
            "objects",
            Value::Array(listed.iter().map(|at| rows[*at].json()).collect()),
        ),
        ("walked", Value::from(order.len())),
        ("verified", Value::from(verified)),
        ("genesis", strings(genesis)),
        ("ok", Value::Bool(problems.is_empty())),
        ("problems", strings(problems)),
        ("linearisation", strings(linearisation)),
    ]))
}

// --- the objects, read -------------------------------------------------------

/// The objects, rehashed, parsed and put in causal order — what every fold
/// below reads.
///
/// It REFUSES where [`verify_objects`] REPORTS, and the difference is the
/// question each is asked: one is "is this store healthy", whose answer is a
/// list of findings; the other is "what does this store believe", which has no
/// honest answer over objects that do not hash to their names.
/// `EventStore::dag` draws the same line.
struct Read {
    dag: Dag<Event>,
    nodes: Vec<Node>,
}

impl Read {
    fn of(objects: &str) -> Result<Read, Refusal> {
        let mut prints = Vec::new();
        for ObjectIn { hash, text } in parse_objects(objects)? {
            let tampered =
                || format!("object {hash} does not hash to its own name (tampered/corrupt)");
            let name = Hash::new(hash.clone()).map_err(|_| tampered())?;
            prints.push((name, Ok(text.into_bytes())));
        }
        let dag = Dag::from_prints(prints);
        if let Some((name, why)) = dag.unread().iter().next() {
            return Err(match why {
                Unread::Tampered(_) => format!(
                    "object {} does not hash to its own name (tampered/corrupt)",
                    name.as_str()
                ),
                _ => format!(
                    "object {} failed to parse: {}",
                    name.as_str(),
                    why.refusal(name)
                ),
            });
        }
        let nodes = dag.nodes().map_err(|e| e.to_string())?;
        Ok(Read { dag, nodes })
    }

    /// The events, in the linearisation's order — merges dropped, since a
    /// merge is structure and carries no event to fold.
    fn events(&self) -> impl Iterator<Item = &Event> {
        self.nodes.iter().filter_map(|node| node.event.as_ref())
    }

    /// Per todo, its events in causal order with the object that carries each —
    /// `json_entry`'s `stream` and `json_series`'s `markers`, from
    /// one pass.
    fn streams(&self) -> BTreeMap<String, Vec<(Hash, Event)>> {
        let mut out: BTreeMap<String, Vec<(Hash, Event)>> = BTreeMap::new();
        for node in &self.nodes {
            if let Some(event) = &node.event {
                out.entry(event.todo().as_str().to_owned())
                    .or_default()
                    .push((node.name.clone(), event.clone()));
            }
        }
        out
    }

    /// The moment to fold at. `None` means "everything the chain holds", which
    /// is the LATEST INSTANT THE CHAIN ITSELF STAMPS and never the host's
    /// clock: a core that read a clock would let one decide who wins, which is
    /// the thing §1 forbids. A caller that means "now" says so.
    fn moment(&self, at: Option<Instant>) -> Result<Datetime, Refusal> {
        let instant = match at {
            Some(instant) => instant,
            None => self
                .events()
                .map(|event| instant_of(event.at()))
                .max()
                .unwrap_or_else(|| {
                    prodrome::fpl::parse_iso("0001-01-01T00:00:00").expect("a real instant")
                }),
        };
        datetime_of(instant).map_err(|e| format!("at: {e}"))
    }
}

// --- §6: the folds -----------------------------------------------------------

/// §6.1–6.5 at `at` (ISO, or `null` for everything the chain holds), under
/// `untrusted` — a list of actor names, read as the REFERENCE policy (§5),
/// which arrives from the server because WHICH actors a host stands behind is
/// instance knowledge and a core that guessed it would fold a different chain
/// while claiming to fold the same one. The wire is what it always was; since
/// 0.3 the core takes a `policy::Policy` and this array becomes one.
///
/// The answer is the five folds, each keyed by todo id and shaped the way
/// `view.py` already puts it on the wire:
///
/// - `env`      — §6.1, as `{outcomes, tended}` — §7's environment shape, so
///                it is also a legal argument to [`fulfillment`] and [`explain`]
/// - `specs`    — §6.2, each a [`json::to_json`] term
/// - `content`  — §6.3, each the reference's `json_authored` MINUS `rich`
/// - `flatten`  — §6.4, each a [`json::to_json`] term: the todo's fulfillment
///                FUNCTION, which is what a `value` and a series are read off
/// - `functions`— every todo the objects mention, each its `flatten` function
///                or `absent` where it has none: what a `ref` links against
///                (§7.2), and so [`link`]'s `specs` for a chain
/// - `history`  — §6.5, as `{bindings, tended}`, and the argument
///                [`series_knots`] wants, so that a knot in the past is
///                evaluated against the environment as of that past instant
///
/// `stream` rides beside them — per todo, its events in causal order with the
/// object carrying each — because an `Entry` on the wire has one, and a locally
/// folded list that dropped it would be a DIFFERENT list rather than the same
/// one computed here.
#[wasm_bindgen]
pub fn fold(objects: &str, at: Option<String>, untrusted: &str) -> Result<String, JsError> {
    let read = Read::of(objects).map_err(refused)?;
    let policy = parse_untrusted(untrusted).map_err(refused)?;
    let moment = read
        .moment(parse_moment("at", at).map_err(refused)?)
        .map_err(refused)?;
    let state = registers::fold(&read.nodes);
    let now = instant_of(moment);
    let mut env = prodrome::fpl::Env::new();
    let (mut specs, mut content, mut flat) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    for prodrome in state.prodromes().values() {
        let one = fold::env(prodrome, now, &policy);
        env.outcomes.extend(one.outcomes);
        env.tended.extend(one.tended);
        specs.extend(fold::specs(prodrome, now, &policy));
        content.extend(fold::content(prodrome, now));
        flat.extend(fold::flatten(prodrome, now, &policy).map_err(|e| refused(e.0))?);
    }
    let functions = Value::Object(
        fold::link_specs(&flat, state.entities().map(|(todo, _)| todo))
            .into_iter()
            .map(|(todo, term)| (todo, crate::json::to_json(&term)))
            .collect(),
    );

    // Each todo's state register read at every instant a binding write is
    // dated, where the reading changes, and every binding tending: what
    // `series_knots` reads back.
    let mut bindings = serde_json::Map::new();
    let mut tended = BTreeMap::new();
    for (todo, stream) in state.entities() {
        let mut instants = BTreeSet::new();
        for stamp in stream.iter().filter(|s| policy.standing(&*s.event).binds()) {
            for write in fold::Write::of(&stamp.event) {
                match write {
                    fold::Write::State(_) => {
                        instants.insert(instant_of(stamp.event.at()));
                    }
                    fold::Write::Tend(at) => {
                        tended
                            .entry(todo.as_str().to_owned())
                            .or_insert_with(BTreeSet::new)
                            .insert(at);
                    }
                    _ => {}
                }
            }
        }
        let mut timeline = Vec::new();
        let mut before = Candidates::from([None]);
        for at in instants {
            let reading = fold::read(stream, Some(at), &policy).outcomes();
            if reading != before {
                timeline.push(json!({ "at": iso(at), "binding": json_reading(&reading) }));
                before = reading;
            }
        }
        if !timeline.is_empty() {
            bindings.insert(todo.as_str().to_owned(), Value::Array(timeline));
        }
    }
    let history = json!({ "bindings": bindings, "tended": json_tended(&tended) });
    let stream = Value::Object(
        read.streams()
            .iter()
            .map(|(todo, events)| {
                (
                    todo.clone(),
                    Value::Array(
                        events
                            .iter()
                            .map(|(name, event)| json_marker(name, event))
                            .collect(),
                    ),
                )
            })
            .collect(),
    );

    printed(&object(vec![
        ("at", Value::String(iso(instant_of(moment)))),
        ("env", json_env(&env)),
        ("specs", json_candidates(&specs, crate::json::to_json)),
        ("content", json_candidates(&content, json_record)),
        ("flatten", json_terms(&flat)),
        ("functions", functions),
        ("history", history),
        ("stream", stream),
    ]))
}

/// §6.6 — the registers whose frontier holds more than one write: a DAG's
/// concurrent writes that no later write has settled, by todo and by register
/// kind, each named by the object that made it.
///
/// `{}` on a chain, which is why this is a separate call and not a sixth key
/// on [`fold`]: the answer is empty on today's store, and an ancestry bitset
/// per object is not a cost to pay on every fold to learn that again.
#[wasm_bindgen]
pub fn registers(objects: &str, at: Option<String>, untrusted: &str) -> Result<String, JsError> {
    let read = Read::of(objects).map_err(refused)?;
    let policy = parse_untrusted(untrusted).map_err(refused)?;
    let moment = parse_moment("at", at)
        .map_err(refused)?
        .map(datetime_of)
        .transpose()
        .map_err(|e| refused(format!("at: {e}")))?;
    let state = registers::fold(&read.nodes);
    let conflicts = Value::Object(
        state
            .entities()
            .filter_map(|(todo, stream)| {
                let found = fold::read(stream, moment.map(instant_of), &policy).conflicts();
                (!found.is_empty()).then(|| {
                    let by_kind = found
                        .into_iter()
                        .map(|(kind, names)| {
                            let names = names.iter().map(|name| name.as_str().to_owned());
                            (kind.as_str().to_owned(), strings(names))
                        })
                        .collect();
                    (todo.as_str().to_owned(), Value::Object(by_kind))
                })
            })
            .collect(),
    );
    printed(&object(vec![("conflicts", conflicts)]))
}

/// §6.7 — every todo the chain has ever mentioned, as the folds see it at `at`
/// (ISO, or `null` for everything the chain holds), under `untrusted`.
///
/// THE COMPOSITION IS THE CORE'S. Until this existed the tab performed it
/// itself — two [`fold`]s, a [`registers`] call, and the page's TypeScript deciding
/// what a claim is, when an entry is provisional, and which environment prices
/// it. That was a second reading of §6.7 in a second language,
/// which is exactly what this crate exists to prevent, and it is gone.
///
/// `records` is the lookup an entry's `content` NAME implies: an entry carries
/// the name of the winning content object and never the record, and a browser
/// holding those objects as TEXT cannot open one without the §2 parser — which
/// is here. So the answer carries the records the rows actually name, and
/// nothing else: it is the caller's own lookup done on the caller's behalf,
/// not the Prodrome deciding what a body is for. The shape is
/// [`wire::json_record`] — `todo`, `at`, `actor` and the payload's own fields
/// under the names it stores them by.
///
/// `order` is the rows' todo ids in §6.7's list order ([`prodrome::view::list_order`]):
/// most urgent first, then every row with no number — `absent`, or not
/// linking — by id. A page lists in it and does not sort for itself.
///
/// `created` is the other half of that lookup, for a todo with NO record: per
/// todo, the instant of its first `Created` and the text of its last — what
/// `prodrome list` shows as the body when there is no record to read one
/// from, and when it asks whether a todo existed yet at `at`. Every `Created`
/// in the chain counts, before `at` or after, exactly as the command line
/// gathers them; the rows say what `at` believes.
#[wasm_bindgen]
pub fn entries(objects: &str, at: Option<String>, untrusted: &str) -> Result<String, JsError> {
    let read = Read::of(objects).map_err(refused)?;
    let policy = parse_untrusted(untrusted).map_err(refused)?;
    let moment = read
        .moment(parse_moment("at", at).map_err(refused)?)
        .map_err(refused)?;
    let rows = prodrome::view::entries(&read.nodes, moment, &policy)
        .map_err(|e| refused(e.to_string()))?;
    let mut listed: Vec<&prodrome::view::Entry<Event>> = rows.iter().collect();
    listed.sort_by(|a, b| prodrome::view::list_order(a, b));
    let records: serde_json::Map<String, Value> = rows
        .iter()
        .flat_map(|row| row.content())
        .filter_map(|name| match read.dag.get(name).and_then(Envelope::event) {
            Some(TodoEvent::Authored(record)) => {
                Some((name.as_str().to_owned(), json_record(record)))
            }
            _ => None,
        })
        .collect();
    let mut created: BTreeMap<String, (String, String)> = BTreeMap::new();
    for event in read.events() {
        if let TodoEvent::Created(event) = event {
            let at = iso(instant_of(event.at));
            created
                .entry(event.todo.as_str().to_owned())
                .and_modify(|(_, text)| text.clone_from(&event.text))
                .or_insert((at, event.text.clone()));
        }
    }
    let created: serde_json::Map<String, Value> = created
        .into_iter()
        .map(|(todo, (at, text))| (todo, json!({ "at": at, "text": text })))
        .collect();
    printed(&object(vec![
        ("at", Value::String(iso(instant_of(moment)))),
        (
            "entries",
            Value::Array(rows.iter().map(json_entry).collect()),
        ),
        (
            "order",
            strings(listed.iter().map(|row| row.key.as_str().to_owned())),
        ),
        ("records", Value::Object(records)),
        ("created", Value::Object(created)),
    ]))
}

// --- §3 and §4: SEALING, so a replica can append ------------------------------

/// A lifecycle event, as its CANONICAL PRINT — §4's five same-shaped kinds
/// (`Created`, `Completed`, `Cancelled`, `Reopened`, `Tended`), through the very `mk_*`
/// constructors that are their parse boundary.
///
/// This exists because a replica has to be able to append WITH NO NETWORK, and
/// the only alternative was a printer written in TypeScript — a second
/// implementation of §2's grammar, which is exactly what this crate exists to
/// prevent. The caller supplies four strings; the core decides whether they are
/// a value, spells it, and refuses if they are not.
///
/// The answer is a print rather than JSON because the print IS the value's
/// identity (§3): it is what [`seal`] hashes, what the file holds, and what
/// crosses every other boundary in this system.
///
/// ⚠️ `at` IS THE CALLER'S CLAIM AND NOTHING CLAMPS IT (§1). A device with a
/// wrong clock produces an event dated in the future; `verify` reports what it
/// must and no layer here rewrites it.
#[wasm_bindgen]
pub fn lifecycle(
    kind: &str,
    todo: &str,
    at: &str,
    actor: &str,
    note: &str,
) -> Result<String, JsError> {
    let at = parse_instant("at", at).map_err(refused)?;
    let at = datetime_of(at).map_err(|e| refused(format!("at: {e}")))?;
    let event: Event = match kind {
        "Created" => prodrome::event::mk_created(todo, at, actor, "", note),
        "Completed" => prodrome::event::mk_completed(todo, at, actor, note),
        "Cancelled" => prodrome::event::mk_cancelled(todo, at, actor, note),
        "Reopened" => prodrome::event::mk_reopened(todo, at, actor, note),
        "Tended" => prodrome::event::mk_tended(todo, at, actor, note),
        other => {
            return Err(refused(format!(
                "kind must be one of Created, Completed, Cancelled, Reopened, Tended, got {other:?}"
            )));
        }
    }
    .map_err(|e| refused(e.to_string()))?;
    Ok(prodrome::event::canonical(&event))
}

/// One object's name and its bytes, the way `/api/objects` carries them.
fn object_of(envelope: &Object) -> Result<String, JsError> {
    let literal = prodrome::event::canonical_envelope(envelope);
    printed(&object(vec![
        ("name", Value::String(name_of(&literal))),
        ("literal", Value::String(literal)),
    ]))
}

/// The event a sealing call was given, or `None` — a print, read back through
/// the closed vocabulary and every `mk_*` rule, so a caller cannot seal a
/// record the constructors would have refused.
fn event_of(event: Option<String>) -> Result<Option<Event>, JsError> {
    event
        .map(|print| {
            prodrome::event::parse_event(&print).map_err(|e| refused(format!("event: {e}")))
        })
        .transpose()
}

/// §3 — SEAL AN EVENT ONTO THE HEADS A REPLICA HOLDS, and answer with the name
/// and the bytes.
///
/// `prev` is a JSON array of head names: none is genesis (`Sealed(prev='')`),
/// one is an ordinary `Sealed`, and two or more is a `Woven` that writes and
/// joins at once. `event` is a lifecycle print from [`lifecycle`] or an
/// `Authored` print the box authored (`POST /api/author`), and `null` is only
/// legal with two parents or more, where it is a plain merge — see
/// [`merge_object`], which is the door that says so in its name.
///
/// THIS IS THE WHOLE OF WHAT A PHONE NEEDS TO WRITE. Print through the core's
/// printer, hash the print: the name is `sha256(utf8(print))`, which is what
/// the store's filename is and what the box rehashes on receipt. So a replica
/// and a box independently agree on what an object is CALLED, with nothing in
/// between them but bytes.
#[wasm_bindgen]
pub fn seal(prev: &str, event: Option<String>) -> Result<String, JsError> {
    let parents = parse_names("prev", prev).map_err(refused)?;
    let event = event_of(event)?;
    let envelope = match (parents.len(), event) {
        (0, Some(event)) => prodrome::event::mk_sealed(None, event),
        (1, Some(event)) => prodrome::event::mk_sealed(parents.into_iter().next(), event),
        (_, event) => {
            prodrome::event::mk_woven(parents, event).map_err(|e| refused(e.to_string()))?
        }
    };
    object_of(&envelope)
}

/// §3 — A MERGE: `Woven(parents, event)` over two heads or more.
///
/// `event` is `null` for the ordinary case, and that is the design rather than
/// an omission: a merge asserts STRUCTURE, not a fact about a todo, so it
/// carries no author, no instant and no id. Two replicas joining the same
/// heads therefore produce the same bytes — `mk_woven` sorts the parents — and
/// the union of two histories stays a union however many times it is taken.
///
/// A caller that wants the join ATTRIBUTED passes an ordinary event print, and
/// the object is a write and a join at once.
#[wasm_bindgen]
pub fn merge_object(parents: &str, event: Option<String>) -> Result<String, JsError> {
    let parents = parse_names("parents", parents).map_err(refused)?;
    let event = event_of(event)?;
    let envelope = prodrome::event::mk_woven(parents, event).map_err(|e| refused(e.to_string()))?;
    object_of(&envelope)
}

/// A JSON array of object names, each checked as one.
fn parse_names(field: &str, json_text: &str) -> Result<Vec<Hash>, Refusal> {
    let names: Vec<String> = serde_json::from_str(json_text)
        .map_err(|e| format!("{field}: expected a list of object names ({e})"))?;
    names
        .into_iter()
        .map(|name| Hash::new(name).map_err(|e| format!("{field}: {e}")))
        .collect()
}

// --- §7: the evaluator -------------------------------------------------------

/// §7 — what a term is worth at an instant, under an environment.
///
/// A `f64` and not a printed number: this is the one answer a caller does
/// arithmetic on (the graph page compares it with the server's knot to 1e-9),
/// and a decimal string in between would be a rounding nobody asked for.
/// `undefined` is `∅`: the term has no value there, which is not a number.
#[wasm_bindgen]
pub fn fulfillment(term: &str, now: &str, env: &str) -> Result<Option<f64>, JsError> {
    let term = parse_closed("term", term).map_err(refused)?;
    let now = parse_instant("now", now).map_err(refused)?;
    let env = parse_env(env).map_err(refused)?;
    Ok(prodrome::fpl::fulfillment(&term, now, &env))
}

/// §2 → §7: a term as the CHAIN stores it — its canonical print, the text
/// inside a `SpecRevised` or an `Authored` — read back as the JSON shape
/// everything else here speaks.
///
/// The one place the literal grammar is reachable from JavaScript, and it
/// exists because the grammar is the STORED form: `conformance/series.py`
/// gives each term as a print, an object's literal carries its spec as a
/// print, and without this the two would have to be re-parsed on the JS side —
/// which is the second implementation of §2 that this crate exists to prevent.
#[wasm_bindgen]
pub fn term_json(literal: &str) -> Result<String, JsError> {
    let term = prodrome::fpl::parse_term(literal).map_err(|e| refused(format!("term: {}", e.0)))?;
    printed(&crate::json::to_json(&term))
}

/// §7's explain tree — `Cofree TermF Annotation` as [`json::explain`] writes it: the
/// term's own shape, each node carrying the value it had at the moment its
/// parent used it. A DECORATION, never a second reading.
#[wasm_bindgen]
pub fn explain(term: &str, now: &str, env: &str) -> Result<String, JsError> {
    let term = parse_closed("term", term).map_err(refused)?;
    let now = parse_instant("now", now).map_err(refused)?;
    let env = parse_env(env).map_err(refused)?;
    printed(&crate::json::explain(&term, now, &env))
}

/// §7.1 — a term with every `After` resolved against a snapshot, so a page
/// that redraws a curve stops paying for a lookup per node per sample.
///
/// The compiled term comes back as its canonical PRINT and not as the JSON
/// shape, because a compiled term is a term and [`term_json`] already reads
/// one — and because the print is what a caller would cache. Evaluate it with
/// `fulfillment(compiled, now, "{}")`: the empty environment, which is the
/// whole claim, since nothing is left in it that could read one.
///
/// `links` is not decoration. A cancelled upstream compiles to a 1.0 that no
/// reader could otherwise tell from a demand met (§7), and this is where the
/// boundary is told which is which.
#[wasm_bindgen]
pub fn compile(term: &str, env: &str) -> Result<String, JsError> {
    let term = parse_closed("term", term).map_err(refused)?;
    let env = parse_env(env).map_err(refused)?;
    let compiled = prodrome::chain::compile(&term, &env);
    printed(&object(vec![
        ("term", Value::String(print_term(compiled.term().term()))),
        (
            "links",
            Value::Array(
                compiled
                    .links()
                    .iter()
                    .map(|link| json::scalars_json(&scalars(&link.fields())))
                    .collect(),
            ),
        ),
    ]))
}

/// §7.2 — every `ref` in `term` bound to the todo it names, recursively.
///
/// `specs` is `{"<todo>": <term>}`, each todo's own function in the JSON shape
/// (a `fold`'s `flatten`, for a chain). The answer is the linked term in that
/// same shape, CLOSED, so it goes straight back into [`fulfillment`],
/// [`explain`], [`compile`] and [`series_knots`], which refuse a term that
/// still holds a `ref`. An unknown todo or a loop is the thrown refusal, the
/// loop named.
#[wasm_bindgen]
pub fn link(term: &str, specs: &str) -> Result<String, JsError> {
    let term = parse_term("term", term).map_err(refused)?;
    let specs = parse_specs(specs).map_err(refused)?;
    let linked = prodrome::fpl::link(&term, &specs).map_err(|e| refused(format!("link: {e}")))?;
    printed(&crate::json::to_json(linked.term()))
}

/// §7's knots — the curve a graph draws over `[from, to]`, and whether the
/// knots ARE the curve. A knot's value is `null` where the term reads `∅`.
///
/// `history` is the environment as a function of time ([`fold`]'s key of that
/// name), because every knot is evaluated against the environment AS OF its own
/// instant: a dependency an `After` reads is unbound before it completed and
/// bound from then, exactly as `view.series_of` does it.
#[wasm_bindgen]
pub fn series_knots(term: &str, from: &str, to: &str, history: &str) -> Result<String, JsError> {
    let term = parse_closed("term", term).map_err(refused)?;
    let from = parse_instant("from", from).map_err(refused)?;
    let to = parse_instant("to", to).map_err(refused)?;
    let past = History::parse(history).map_err(refused)?;
    let series = prodrome::breaks::series_knots(&term, from, to, |at| past.at(at));
    printed(&object(vec![
        (
            "knots",
            Value::Array(
                series
                    .knots
                    .iter()
                    .map(|knot| {
                        Value::Array(vec![Value::String(iso(knot.at)), Value::from(knot.value)])
                    })
                    .collect(),
            ),
        ),
        ("exact", Value::Bool(series.exact)),
    ]))
}

#[cfg(test)]
mod tests {
    use prodrome::change::mk_change;
    use prodrome::event::{canonical_envelope, mk_created, seal_hash, Envelope};
    use prodrome::genesis::mk_genesis;
    use prodrome::literal::Datetime;
    use serde_json::{json, Value};

    /// A store of changes begins at its `Genesis`: a change with no deps
    /// names no parents, and is no beginning.
    #[test]
    fn a_store_of_changes_begins_at_its_genesis() {
        let at = Datetime::new(2026, 9, 10, 9, 0, 0, 0).expect("an instant");
        let genesis: super::Object =
            Envelope::Genesis(mk_genesis("roadmap", &"0".repeat(32)).expect("a genesis"));
        let root = seal_hash(&genesis);
        let created = mk_created("a", at, "bassel", "a todo", "").expect("an event");
        let created: super::Object =
            Envelope::Change(mk_change(root.clone(), vec![], created).expect("a change"));
        let sent: Vec<Value> = [&genesis, &created]
            .into_iter()
            .map(|object| {
                json!({ "hash": seal_hash(object).as_str(), "text": canonical_envelope(object) })
            })
            .collect();
        let answer = super::verify_objects(&Value::from(sent).to_string())
            .unwrap_or_else(|_| panic!("verify answers"));
        let answer: Value = serde_json::from_str(&answer).expect("JSON");
        assert_eq!(answer["genesis"], json!([root.as_str()]));
        assert_eq!(answer["ok"], json!(true), "{answer}");
    }
}
