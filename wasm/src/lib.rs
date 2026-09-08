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
//! | `verify_objects` | §3: do these bytes hash to these names, and do they form one DAG under these heads |
//! | `lifecycle`      | §4: spell one lifecycle event, through its own `mk_*`    |
//! | `seal`           | §3: seal an event onto these heads — the name and the bytes |
//! | `merge_object`   | §3: join these heads — structure, so no event and no actor |
//! | `fold`           | §6.1–6.5: what does the chain believe at an instant      |
//! | `registers`      | §6.6: which registers have more than one live write      |
//! | `entries`        | §6.7: every todo as the folds see it, composed ONCE      |
//! | `fulfillment`    | §7: what is this term worth now                          |
//! | `explain`        | §7: what is that number made of                          |
//! | `series_knots`   | §7 knots: what is that term's curve over a window        |
//! | `term_json`      | §2 → §7: a stored term's print, as the JSON shape above  |
//!
//! `store` is not reachable from here except for [`prodrome::store::linearise`],
//! which is pure: the rest of that module is file-backed and a browser has no
//! `events/` directory. The objects arrive over `/api/chain` instead, and
//! [`verify_objects`] is `EventStore::verify`'s findings asked of a set in
//! memory.
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

use prodrome::event::{parents_of, parse_envelope, Envelope, Hash, TodoEvent};
use prodrome::fpl::{datetime_of, instant_of, iso, Instant};
use prodrome::literal::Datetime;
use prodrome::registers;
use prodrome::store::linearise;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;

mod wire;

use wire::{
    json_authored, json_content, json_entry, json_env, json_marker, json_terms, object, parse_env,
    parse_instant, parse_moment, parse_objects, parse_term, parse_untrusted, strings, History,
    ObjectIn, Refusal,
};

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
        }
    }

    fn hash_ok(&self) -> bool {
        self.hash == self.computed
    }

    /// What the envelope SAYS about itself — the same four fields
    /// `json_link` puts on the wire, read here out of the object's own
    /// bytes rather than out of the server's summary of them. A merge is
    /// structure and not a fact about a todo, so its `todo`, `actor` and `at`
    /// stay "" and its `kind` is the envelope's own name; nothing is invented
    /// to fill them.
    fn describe(&mut self, envelope: &Envelope) {
        self.parents = parents_of(envelope);
        match envelope.event() {
            Some(event) => {
                self.kind = event.kind_name().to_owned();
                self.todo = event.todo().as_str().to_owned();
                self.actor = event.actor().as_str().to_owned();
                self.at = iso(instant_of(event.at()));
            }
            None => self.kind = "Woven".to_owned(),
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
/// canonical print that name is a hash of. `tips` is every head the walk starts
/// from (a DAG has heads, plural; a chain has one).
///
/// What is checked is `EventStore::verify`'s list, less the findings that are
/// about a DIRECTORY and cannot be asked of a set in memory (a file that will
/// not read, HEAD/`refs/` disagreement, a stale head):
///
/// 1. every object rehashes to the name it claims;
/// 2. every object PARSES as an envelope — which the SubtleCrypto path never
///    could, so a malformed `Woven` or an unknown constructor is now caught in
///    the tab;
/// 3. the walk from every head, over the parents the objects THEMSELVES name,
///    reaches genesis without a gap;
/// 4. no cycle, and the objects go into one causal order (`linearise`);
/// 5. nothing sent is left off that walk.
///
/// Each finding's wording is the store's where the store has one and
/// a page's own where the page already had one, so the sentence a
/// reader gets does not depend on which checker produced it — only the line
/// that says which one ran does.
///
/// TAKES THE TIPS, which the brief's signature did not: reachability, depth
/// and orphanhood are all relative to the heads, and a verification split down
/// the middle — the hashes here, the walk in TypeScript — would be exactly the
/// second implementation this crate exists to remove.
#[wasm_bindgen]
pub fn verify_objects(objects: &str, tips: &str) -> Result<String, JsError> {
    let sent = parse_objects(objects).map_err(refused)?;
    let tips: Vec<String> =
        serde_json::from_str(tips).map_err(|e| refused(format!("tips: expected a list ({e})")))?;

    let mut problems: Vec<String> = Vec::new();
    let mut rows: Vec<Row> = Vec::new();
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    let mut parsed: BTreeMap<Hash, Envelope> = BTreeMap::new();

    for ObjectIn { hash, text } in &sent {
        if index.contains_key(hash) {
            problems.push(format!("object {hash} was sent twice"));
            continue;
        }
        let mut row = Row::blank(hash.clone(), name_of(text));
        // The store's order: the bytes first, then the grammar. Reporting a
        // parse failure for a tampered object would name the wrong fault.
        if !row.hash_ok() {
            row.problem = format!(
                "does not hash to its own name — this browser computed {}",
                row.computed
            );
            problems.push(format!("object {hash} {}", row.problem));
        } else {
            match parse_envelope(text) {
                Ok(envelope) => {
                    row.describe(&envelope);
                    if let Ok(name) = Hash::new(hash.clone()) {
                        parsed.insert(name, envelope);
                    }
                }
                Err(error) => {
                    row.problem = format!("failed to parse: {error}");
                    problems.push(format!("object {hash} {}", row.problem));
                }
            }
        }
        index.insert(hash.clone(), rows.len());
        rows.push(row);
    }

    if tips.is_empty() {
        problems.push("the server named no tip".to_owned());
    }
    // Breadth first from every head at once, so `depth` is the distance from
    // the nearest head and the order stays heads-first the way the page reads
    // it.
    let mut order: Vec<usize> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut queue: Vec<(String, usize)> = tips.iter().map(|name| (name.clone(), 0)).collect();
    let mut cursor = 0;
    while cursor < queue.len() {
        let (name, depth) = queue[cursor].clone();
        cursor += 1;
        if seen.contains(&name) {
            continue;
        }
        let Some(at) = index.get(&name).copied() else {
            problems.push(if depth == 0 {
                format!("the tip {name} was not sent with the chain")
            } else {
                format!("object {name} is named as a parent but was not sent")
            });
            continue;
        };
        seen.insert(name);
        rows[at].reachable = true;
        rows[at].depth = Some(depth);
        order.push(at);
        for parent in rows[at].parents.clone() {
            queue.push((parent.as_str().to_owned(), depth + 1));
        }
    }

    // A genesis object has no parents AND said so itself: an object that did
    // not parse names none either, and calling that a beginning would turn a
    // tampered tip into a clean chain of one.
    let genesis: Vec<String> = order
        .iter()
        .filter(|at| rows[**at].parents.is_empty() && rows[**at].problem.is_empty())
        .map(|at| rows[*at].hash.clone())
        .collect();
    if genesis.is_empty() && problems.is_empty() {
        problems.push(
            "the walk ended without reaching a genesis object (an object with no parents)"
                .to_owned(),
        );
    }

    // The causal order, from the core's own `linearise` — which is also where a
    // cycle is found. Asked only when every named parent is present: its
    // missing-object refusal would otherwise repeat, in different words, what
    // the walk above already reported.
    let complete = parsed
        .values()
        .flat_map(parents_of)
        .all(|parent| parsed.contains_key(&parent));
    let mut linearisation: Vec<String> = Vec::new();
    if complete {
        match linearise(&parsed) {
            Ok(names) => linearisation = names.iter().map(|n| n.as_str().to_owned()).collect(),
            Err(error) => problems.push(error.to_string()),
        }
    }

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
/// `EventStore::read_dag_named` draws the same line, in the same words.
struct Read {
    order: Vec<Hash>,
    objects: BTreeMap<Hash, Envelope>,
}

impl Read {
    fn of(objects: &str) -> Result<Read, Refusal> {
        let sent = parse_objects(objects)?;
        let mut parsed: BTreeMap<Hash, Envelope> = BTreeMap::new();
        for ObjectIn { hash, text } in &sent {
            if name_of(text) != *hash {
                return Err(format!(
                    "object {hash} does not hash to its own name (tampered/corrupt)"
                ));
            }
            let name = Hash::new(hash.clone()).map_err(|e| format!("object {hash}: {e}"))?;
            let envelope =
                parse_envelope(text).map_err(|e| format!("object {hash} failed to parse: {e}"))?;
            parsed.insert(name, envelope);
        }
        let order = linearise(&parsed).map_err(|e| e.to_string())?;
        Ok(Read {
            order,
            objects: parsed,
        })
    }

    /// The events, in the linearisation's order — merges dropped, since a
    /// merge is structure and carries no event to fold.
    fn events(&self) -> Vec<TodoEvent> {
        self.order
            .iter()
            .filter_map(|name| self.objects[name].event().cloned())
            .collect()
    }

    fn nodes(&self) -> Vec<registers::Node> {
        self.order
            .iter()
            .map(|name| registers::Node::of(name.clone(), &self.objects[name]))
            .collect()
    }

    /// Per todo, its events in causal order with the object that carries each —
    /// `json_entry`'s `stream` and `json_series`'s `markers`, from
    /// one pass.
    fn streams(&self) -> BTreeMap<String, Vec<(Hash, TodoEvent)>> {
        let mut out: BTreeMap<String, Vec<(Hash, TodoEvent)>> = BTreeMap::new();
        for name in &self.order {
            if let Some(event) = self.objects[name].event() {
                out.entry(event.todo().as_str().to_owned())
                    .or_default()
                    .push((name.clone(), event.clone()));
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
                .iter()
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
/// `untrusted` — a list of actor names, the deployment's whole §5 policy,
/// which arrives from the server because WHICH actors are provisional is
/// instance knowledge and a core that guessed it would fold a different chain
/// while claiming to fold the same one.
///
/// The answer is the five folds, each keyed by todo id and shaped the way
/// `view.py` already puts it on the wire:
///
/// - `env`      — §6.1, as `{kind, at}` — §7's environment shape, so it is
///                also a legal argument to [`fulfillment`] and [`explain`]
/// - `specs`    — §6.2, each an `fpl.to_json` term
/// - `content`  — §6.3, each the reference's `json_authored` MINUS `rich`
/// - `flatten`  — §6.4, each an `fpl.to_json` term: the todo's fulfillment
///                FUNCTION, which is what a `value` and a series are read off
/// - `history`  — §6.5, and the argument [`series_knots`] wants, so that a
///                knot in the past is evaluated against the environment as of
///                that past instant
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
    let events = read.events();

    let env = prodrome::fold::env_at(&events, moment, &policy);
    let specs = prodrome::fold::specs_at(&events, moment, &policy);
    let content = prodrome::fold::authored_at(&events, moment);
    let flat = prodrome::fold::flatten(&events, moment, &policy).map_err(|e| refused(e.0))?;
    let past = prodrome::fold::history(&events, &policy);

    let history = Value::Object(
        past.bindings()
            .iter()
            .map(|(todo, timeline)| {
                (
                    todo.as_str().to_owned(),
                    Value::Array(
                        timeline
                            .iter()
                            .map(|(at, binding)| {
                                json!({
                                    "at": iso(*at),
                                    "binding": binding.map_or(Value::Null, |b| json!({
                                        "kind": b.kind(),
                                        "at": iso(b.at()),
                                    })),
                                })
                            })
                            .collect(),
                    ),
                )
            })
            .collect(),
    );
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
        ("specs", json_terms(&specs)),
        ("content", json_content(&content)),
        ("flatten", json_terms(&flat)),
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
    let state = registers::fold(&read.nodes(), moment, &policy);
    let conflicts = Value::Object(
        registers::conflicts_of(&state)
            .iter()
            .map(|(todo, by_kind)| {
                (
                    todo.as_str().to_owned(),
                    Value::Object(
                        by_kind
                            .iter()
                            .map(|(kind, frontier)| {
                                (
                                    kind.as_str().to_owned(),
                                    strings(
                                        frontier
                                            .writes()
                                            .iter()
                                            .map(|write| write.at.as_str().to_owned()),
                                    ),
                                )
                            })
                            .collect(),
                    ),
                )
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
/// is here. So the answer carries the `Authored` records the rows actually
/// name, and nothing else: it is the caller's own lookup done on the caller's
/// behalf, not the Prodrome deciding what a body is for. The shape is
/// [`wire::json_authored`], which is the reference's `json_authored` MINUS `rich`.
#[wasm_bindgen]
pub fn entries(objects: &str, at: Option<String>, untrusted: &str) -> Result<String, JsError> {
    let read = Read::of(objects).map_err(refused)?;
    let policy = parse_untrusted(untrusted).map_err(refused)?;
    let moment = read
        .moment(parse_moment("at", at).map_err(refused)?)
        .map_err(refused)?;
    let rows = prodrome::view::entries(&read.nodes(), moment, &policy)
        .map_err(|e| refused(e.to_string()))?;
    let records: serde_json::Map<String, Value> = rows
        .iter()
        .filter_map(|row| row.content.as_ref())
        .filter_map(|name| match read.objects[name].event() {
            Some(TodoEvent::Authored(record)) => {
                Some((name.as_str().to_owned(), json_authored(record)))
            }
            _ => None,
        })
        .collect();
    printed(&object(vec![
        ("at", Value::String(iso(instant_of(moment)))),
        (
            "entries",
            Value::Array(rows.iter().map(json_entry).collect()),
        ),
        ("records", Value::Object(records)),
    ]))
}

// --- §3 and §4: SEALING, so a replica can append ------------------------------

/// A lifecycle event, as its CANONICAL PRINT — §4's four same-shaped kinds
/// (`Created`, `Completed`, `Cancelled`, `Reopened`), through the very `mk_*`
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
    let event = match kind {
        "Created" => prodrome::event::mk_created(todo, at, actor, "", note),
        "Completed" => prodrome::event::mk_completed(todo, at, actor, note),
        "Cancelled" => prodrome::event::mk_cancelled(todo, at, actor, note),
        "Reopened" => prodrome::event::mk_reopened(todo, at, actor, note),
        other => {
            return Err(refused(format!(
                "kind must be one of Created, Completed, Cancelled, Reopened, got {other:?}"
            )));
        }
    }
    .map_err(|e| refused(e.to_string()))?;
    Ok(prodrome::event::canonical(&event))
}

/// One object's name and its bytes, the way `/api/objects` carries them.
fn object_of(envelope: &Envelope) -> Result<String, JsError> {
    let literal = prodrome::event::canonical_envelope(envelope);
    printed(&object(vec![
        ("name", Value::String(name_of(&literal))),
        ("literal", Value::String(literal)),
    ]))
}

/// The event a sealing call was given, or `None` — a print, read back through
/// the closed vocabulary and every `mk_*` rule, so a caller cannot seal a
/// record the constructors would have refused.
fn event_of(event: Option<String>) -> Result<Option<TodoEvent>, JsError> {
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
#[wasm_bindgen]
pub fn fulfillment(term: &str, now: &str, env: &str) -> Result<f64, JsError> {
    let term = parse_term("term", term).map_err(refused)?;
    let now = parse_instant("now", now).map_err(refused)?;
    let env = parse_env(env).map_err(refused)?;
    Ok(prodrome::fpl::fulfillment(&term, now, &env))
}

/// §2 → §7: a term as the CHAIN stores it — its canonical print, the text
/// inside a `SpecRevised` or an `Authored` — read back as the JSON shape
/// everything else here speaks.
///
/// The one place the literal grammar is reachable from JavaScript, and it
/// exists because the grammar is the STORED form: `conformance/series.json`
/// gives each term as a print, an object's literal carries its spec as a
/// print, and without this the two would have to be re-parsed on the JS side —
/// which is the second implementation of §2 that this crate exists to prevent.
#[wasm_bindgen]
pub fn term_json(literal: &str) -> Result<String, JsError> {
    let term = prodrome::fpl::parse_term(literal).map_err(|e| refused(format!("term: {}", e.0)))?;
    printed(&prodrome::fpl::to_json(&term))
}

/// §7's explain tree — `Cofree TermF Annotation` as `fpl.explain`'s JSON: the
/// term's own shape, each node carrying the value it had at the moment its
/// parent used it. A DECORATION, never a second reading.
#[wasm_bindgen]
pub fn explain(term: &str, now: &str, env: &str) -> Result<String, JsError> {
    let term = parse_term("term", term).map_err(refused)?;
    let now = parse_instant("now", now).map_err(refused)?;
    let env = parse_env(env).map_err(refused)?;
    printed(&prodrome::fpl::explain(&term, now, &env))
}

/// §7's knots — the curve a graph draws over `[from, to]`, and whether the
/// knots ARE the curve.
///
/// `history` is the environment as a function of time ([`fold`]'s key of that
/// name), because every knot is evaluated against the environment AS OF its own
/// instant: a dependency an `After` reads is unbound before it completed and
/// bound from then, exactly as `view.series_of` does it.
#[wasm_bindgen]
pub fn series_knots(term: &str, from: &str, to: &str, history: &str) -> Result<String, JsError> {
    let term = parse_term("term", term).map_err(refused)?;
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
