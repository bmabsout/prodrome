//! The exports, generic over a schema: what a page asks of a set of objects
//! whatever its events are.
//!
//! A schema says what its events are ([`prodrome::schema::Schema`]); this
//! module reads a set of objects at one, and [`Json`] is the one thing more a
//! schema says to cross this boundary. Every function here answers JSON
//! text, or a [`Refusal`] the exports throw.

use std::collections::{BTreeMap, BTreeSet};

use prodrome::dag::{Dag, Finding, Unread};
use prodrome::event::{parents_of, Envelope, Hash, TodoEvent};
use prodrome::fpl::{datetime_of, instant_of, iso, Instant};
use prodrome::literal::{Datetime, Value as Literal};
use prodrome::payload::Payload;
use prodrome::policy::Everything;
use prodrome::registers::Node;
use prodrome::schema::Schema;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::wire::{object, parse_names, parse_objects, strings, ObjectIn, Refusal};

/// What a schema says to cross this boundary, beside what it says to be
/// stored. Not a bound on [`Schema`]: JSON is this boundary's business, and a
/// schema a host only stores and reads natively owes it nothing.
///
/// An entity's key reads as a string (`Key: AsRef<str>`), since it is a key
/// of a JSON object.
pub trait Json: Schema<Key: AsRef<str>> {
    /// The field an event names its entity by, and so the field a row names
    /// it under: `"todo"` for the todo schema.
    const KEY: &'static str;
}

/// The todo schema, at any payload, crosses as it always has: each row names
/// its todo. Here and not in a host, which could not implement this crate's
/// trait for the core's type.
impl<P: Payload> Json for TodoEvent<P> {
    const KEY: &'static str = "todo";
}

/// The name these bytes have: sha256 of the canonical print, exactly what
/// `seal_hash` takes and what `events/objects/<name>.py` holds — UTF-8, no
/// trailing newline.
pub fn name_of(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// An answer as an export returns it: a refusal is the exception a JS caller
/// catches, never a panic, which would leave the module poisoned for the rest
/// of the session.
pub fn thrown(answer: Result<String, Refusal>) -> Result<String, wasm_bindgen::JsError> {
    answer.map_err(|refusal| wasm_bindgen::JsError::new(&refusal))
}

pub fn printed(value: &Value) -> Result<String, Refusal> {
    serde_json::to_string(value).map_err(|e| format!("could not print the answer: {e}"))
}

/// One object as it was sent: the name it claims, the name its bytes have,
/// and whether the claim is a name at all.
struct Sent {
    hash: String,
    computed: String,
    named: bool,
    twice: bool,
}

/// A set of objects, read once at the schema `E`: what every export below
/// asks its question of. The bytes cross from JavaScript once, into the
/// [`Dag`]; nothing here holds them twice.
pub struct Replica<E> {
    sent: Vec<Sent>,
    dag: Dag<E>,
    /// The objects in causal order, or why this set has no honest reading.
    nodes: Result<Vec<Node<E>>, Refusal>,
}

impl<E: Schema> Replica<E> {
    /// `objects` is `[{hash, text}]`: every object's claimed name beside the
    /// exact canonical print that name is a hash of. Refused only where it is
    /// not that shape; what the objects are is [`verify`]'s to say.
    pub fn of(objects: &str) -> Result<Replica<E>, Refusal> {
        let mut seen = BTreeSet::new();
        let mut sent = Vec::new();
        let mut prints = Vec::new();
        for ObjectIn { hash, text } in parse_objects(objects)? {
            let computed = name_of(&text);
            let twice = !seen.insert(hash.clone());
            let named = match Hash::new(hash.clone()) {
                Ok(name) => {
                    if !twice {
                        prints.push((name, Ok(text.into_bytes())));
                    }
                    true
                }
                Err(_) => false,
            };
            sent.push(Sent {
                hash,
                computed,
                named,
                twice,
            });
        }
        let dag = Dag::from_prints(prints);
        let nodes = Replica::order(&sent, &dag);
        Ok(Replica { sent, dag, nodes })
    }

    /// The objects in causal order. It REFUSES where [`verify`] REPORTS, and
    /// the difference is the question each is asked: one is "is this store
    /// healthy", whose answer is a list of findings; the other is "what does
    /// this store believe", which has no honest answer over objects that do
    /// not hash to their names. `EventStore::dag` draws the same line.
    fn order(sent: &[Sent], dag: &Dag<E>) -> Result<Vec<Node<E>>, Refusal> {
        if let Some(Sent { hash, .. }) = sent.iter().find(|sent| !sent.named) {
            return Err(format!(
                "object {hash} does not hash to its own name (tampered/corrupt)"
            ));
        }
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
        dag.nodes().map_err(|e| e.to_string())
    }

    pub fn read(&self) -> Result<Read<'_, E>, Refusal> {
        self.nodes.as_ref().map_err(Clone::clone).map(|nodes| Read {
            dag: &self.dag,
            nodes,
        })
    }
}

/// The objects, rehashed, parsed and put in causal order — what every fold
/// reads.
pub struct Read<'r, E> {
    pub dag: &'r Dag<E>,
    pub nodes: &'r [Node<E>],
}

impl<E: Schema> Read<'_, E> {
    /// The events, in the linearisation's order — merges dropped, since a
    /// merge is structure and carries no event to fold.
    pub fn events(&self) -> impl Iterator<Item = &E> {
        self.nodes.iter().filter_map(|node| node.event.as_ref())
    }

    /// The moment to fold at. `None` means "everything the chain holds", which
    /// is the LATEST INSTANT THE CHAIN ITSELF STAMPS and never the host's
    /// clock: a core that read a clock would let one decide who wins, which is
    /// the thing §1 forbids. A caller that means "now" says so.
    pub fn moment(&self, at: Option<Instant>) -> Result<Datetime, Refusal> {
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

// --- §3: verify --------------------------------------------------------------

/// One object's row on the chain page: what it claims, what this core
/// computed, and where the walk found it.
struct Row {
    hash: String,
    computed: String,
    parents: Vec<Hash>,
    kind: String,
    key: String,
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
            key: String::new(),
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

    /// The four fields an event gives its row, read from the object's own
    /// bytes: its constructor, its entity, its writer and its instant. An
    /// object with no event leaves them "" and is kinded by its name.
    fn describe<E: Json>(&mut self, envelope: &Envelope<E>) {
        self.parents = parents_of(envelope);
        self.root = matches!(
            envelope,
            Envelope::Genesis(_) | Envelope::Sealed { prev: None, .. }
        );
        match envelope.event() {
            Some(event) => {
                self.kind = match event.to_value() {
                    Literal::Call(call) => call.name,
                    _ => String::new(),
                };
                event.key().as_ref().clone_into(&mut self.key);
                event.actor().as_str().clone_into(&mut self.actor);
                self.at = iso(instant_of(event.at()));
            }
            None => envelope.name().clone_into(&mut self.kind),
        }
    }

    fn json<E: Json>(&self) -> Value {
        object(vec![
            ("hash", json!(self.hash)),
            ("computed", json!(self.computed)),
            ("hashOk", json!(self.hash_ok())),
            ("reachable", json!(self.reachable)),
            ("depth", json!(self.depth)),
            ("kind", json!(self.kind)),
            (E::KEY, json!(self.key)),
            ("actor", json!(self.actor)),
            ("at", json!(self.at)),
            ("problem", json!(self.problem)),
        ])
    }
}

/// §3's `verify`, for the set of objects a browser was handed.
///
/// The objects are read as one [`prodrome::dag::Dag`], and the answer's `tips`
/// are its tips: nobody has to tell a replica where its heads are.
///
/// The findings are the `Dag`'s, less the ones about a DIRECTORY, which a set
/// in memory has none of. A page's own wording stands where it had one: an
/// object whose bytes do not hash to its name, and a parent that was not sent.
/// The walk from the tips gives each row its depth, and what it does not
/// reach (only an object that is not one, or one caught in a cycle) is said.
pub fn verify<E: Json>(replica: &Replica<E>) -> Result<String, Refusal> {
    let dag = &replica.dag;
    let mut problems: Vec<String> = Vec::new();
    let mut rows: Vec<Row> = Vec::new();
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for sent in &replica.sent {
        if sent.twice {
            problems.push(format!("object {} was sent twice", sent.hash));
            continue;
        }
        index.insert(sent.hash.clone(), rows.len());
        let mut row = Row::blank(sent.hash.clone(), sent.computed.clone());
        if !sent.named {
            row.problem = row.tampered();
            problems.push(format!("object {} {}", sent.hash, row.problem));
        }
        rows.push(row);
    }
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
            Value::Array(listed.iter().map(|at| rows[*at].json::<E>()).collect()),
        ),
        ("walked", Value::from(order.len())),
        ("verified", Value::from(verified)),
        ("genesis", strings(genesis)),
        ("ok", Value::Bool(problems.is_empty())),
        ("problems", strings(problems)),
        ("linearisation", strings(linearisation)),
    ]))
}

// --- §3: where a replica stands ------------------------------------------------

fn names<'n>(names: impl IntoIterator<Item = &'n Hash>) -> Value {
    strings(names.into_iter().map(|name| name.as_str().to_owned()))
}

/// The objects' TIPS, those no object names as a parent, and each prodrome's
/// HEADS, its own tips, by the name of its genesis (a legacy store's by its
/// least root). Refused, like every reading, over objects that are not.
pub fn tips<E: Schema>(replica: &Replica<E>) -> Result<String, Refusal> {
    let dag = replica.read()?.dag;
    let heads = dag
        .geneses()
        .into_iter()
        .map(|genesis| {
            let heads = names(&dag.tips_in(&dag.key(&genesis)));
            (genesis.into_string(), heads)
        })
        .collect();
    printed(&object(vec![
        ("tips", names(&dag.tips())),
        ("heads", Value::Object(heads)),
    ]))
}

/// The objects a replica that holds `tips` (a JSON array of names) and
/// everything they rest on does not, in causal order: what to send it. A tip
/// these objects lack names nothing they hold, so it holds back nothing.
/// Names only: the caller holds the bytes it sent.
pub fn since<E: Schema>(replica: &Replica<E>, tips: &str) -> Result<String, Refusal> {
    let read = replica.read()?;
    let held = read.dag.closure(parse_names("tips", tips)?);
    let since = read
        .nodes
        .iter()
        .map(|node| &node.name)
        .filter(|name| !held.contains(*name));
    printed(&object(vec![("since", names(since))]))
}
