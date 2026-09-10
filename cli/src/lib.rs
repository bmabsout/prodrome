//! `prodrome` — a command line over a Prodrome store.
//!
//! THE HOST IS HERE AND NOWHERE ELSE. The core is generic over what a record
//! holds (§4) and over whose word binds (§5); this crate answers both, with
//! the reference payload ([`prodrome::reference::Todo`]) and the reference
//! policy ([`prodrome::policy::Untrusted`]). It also owns the one thing the
//! core refuses to have: a clock. An event's `at` is data, and a fold is a
//! query at an instant the caller names — so `--at` is how a caller names one
//! and the machine's clock is only the default.
//!
//! Every verb answers with an [`Outcome`]: the text to print and whether the
//! store was found healthy. Nothing here writes to a stream, so a test drives
//! a verb and reads what it said.

#![forbid(unsafe_code)]

pub mod command;
pub mod price;
pub mod render;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use prodrome::event::{
    mk_cancelled, mk_completed, mk_created, mk_reopened, mk_spec_revised, Actor, TodoEvent, TodoId,
};
use prodrome::fold::authored_at;
use prodrome::fpl::FplError;
use prodrome::literal::{Datetime, ProdromeError};
use prodrome::policy::Untrusted;
use prodrome::reference::{mk_authored, Todo};
use prodrome::registers::nodes_of;
use prodrome::store::EventStore;
use prodrome::view::{entries, Entry};

use command::{Cli, Command, Price, Stamp};

/// The store this binary reads and writes: the reference payload, under the
/// reference policy.
pub type Store = EventStore<Todo, Untrusted>;

/// A refusal, as a value. The three the core has, plus the one a command line
/// adds: an argument that means nothing.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Store(#[from] ProdromeError),
    #[error("{0}")]
    Fpl(#[from] FplError),
    #[error("{0}")]
    Usage(String),
}

impl Error {
    fn usage(message: impl Into<String>) -> Error {
        Error::Usage(message.into())
    }
}

/// What a verb answers: what to print, and whether it found what it looked at
/// to be in order. `ok` is false only where a verb makes a JUDGEMENT that
/// failed — `verify` with findings — and never for a refusal, which is an
/// [`Error`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub text: String,
    pub ok: bool,
}

impl Outcome {
    fn said(text: impl Into<String>) -> Outcome {
        Outcome {
            text: text.into(),
            ok: true,
        }
    }
}

/// Where the store is.
///
/// `./roadmap` before `.`, because a repository that keeps its todos in a
/// subdirectory is the case this binary was written for and typing `--store`
/// at every invocation would be the tax for it. Nothing is created here: a
/// path that is not a store is a store with no objects, and `verify` says so.
pub fn store_root(named: Option<&Path>) -> PathBuf {
    match named {
        Some(path) => path.to_path_buf(),
        None => {
            let roadmap = PathBuf::from("roadmap");
            if roadmap.is_dir() {
                roadmap
            } else {
                PathBuf::from(".")
            }
        }
    }
}

/// The reader's policy: the roster named on the command line or in
/// `PRODROME_UNTRUSTED`, and `Untrusted::none()` — standing behind every
/// writer — when neither named one.
fn policy_of(untrusted: &[String]) -> Result<Untrusted, Error> {
    let mut actors = Vec::with_capacity(untrusted.len());
    for name in untrusted {
        actors.push(Actor::new(name.clone())?);
    }
    Ok(Untrusted::of(actors))
}

/// Who is writing: `--actor`, then `$PRODROME_ACTOR` (clap reads it into the
/// same field), then `$USER`. There is no fourth fallback: an event without an
/// actor is not an event this format can hold, so a machine with no `$USER`
/// gets a refusal rather than a name this program invented.
fn actor_of(stamp: &Stamp) -> Result<String, Error> {
    stamp
        .actor
        .clone()
        .or_else(|| std::env::var("USER").ok().filter(|name| !name.is_empty()))
        .ok_or_else(|| Error::usage("no actor: pass --actor, or set PRODROME_ACTOR or USER"))
}

/// One store, folded at one instant, with the two things a §6.7 row does not
/// carry and a reader wants: when each todo was created, and what it says.
pub struct Reading {
    pub at: Datetime,
    pub untrusted: Vec<String>,
    pub entries: Vec<Entry>,
    created: BTreeMap<TodoId, Datetime>,
    bodies: BTreeMap<TodoId, String>,
    details: BTreeMap<TodoId, String>,
}

impl Reading {
    /// Did the chain know about this todo at the instant read?
    ///
    /// A §6.7 row exists because the DAG mentions the todo AT ALL — the
    /// future's included, deliberately — so this is the question `list` has to
    /// ask on top, and its answer is the `Created` event's own `at`.
    pub fn existed(&self, todo: &TodoId) -> bool {
        self.created.get(todo).is_some_and(|at| *at <= self.at)
    }

    pub fn created_at(&self, todo: &TodoId) -> String {
        self.created
            .get(todo)
            .copied()
            .map(price::iso)
            .unwrap_or_default()
    }

    /// What the todo asks for: the winning record's body, and the `Created`
    /// event's text for a todo with no record.
    pub fn body(&self, todo: &TodoId) -> &str {
        self.bodies.get(todo).map_or("", String::as_str)
    }

    pub fn detail(&self, todo: &TodoId) -> &str {
        self.details.get(todo).map_or("", String::as_str)
    }
}

/// Fold the store at `at` under its own policy, and gather what the renderers
/// read.
///
/// The roster comes off the STORE's policy rather than off the command line
/// again: one reading is taken under one policy, and a summary line that
/// quoted its own copy of the argument could say something the fold did not
/// do.
pub fn read(store: &Store, at: Datetime) -> Result<Reading, Error> {
    let objects = store.read_dag_named()?;
    let nodes = nodes_of(&objects);
    let events: Vec<TodoEvent<Todo>> = nodes.iter().filter_map(|node| node.event.clone()).collect();

    let mut created = BTreeMap::new();
    let mut bodies = BTreeMap::new();
    for event in &events {
        if let TodoEvent::Created(e) = event {
            created.entry(e.todo.clone()).or_insert(e.at);
            bodies.insert(e.todo.clone(), e.text.clone());
        }
    }
    let mut details = BTreeMap::new();
    for (todo, record) in authored_at(&events, at) {
        if !record.payload.body.as_str().is_empty() {
            bodies.insert(todo.clone(), record.payload.body.as_str().to_owned());
        }
        details.insert(todo, record.payload.detail.as_str().to_owned());
    }

    Ok(Reading {
        at,
        untrusted: store
            .policy()
            .actors()
            .iter()
            .map(|actor| actor.as_str().to_owned())
            .collect(),
        entries: entries(&nodes, at, store.policy())?,
        created,
        bodies,
        details,
    })
}

/// The todos the chain has ever named — what an id is checked against before a
/// write, so a typo appends nothing.
fn known(store: &Store) -> Result<Vec<TodoId>, Error> {
    Ok(store
        .events()?
        .iter()
        .filter_map(|event| match event {
            TodoEvent::Created(e) => Some(e.todo.clone()),
            _ => None,
        })
        .collect())
}

fn require_known(store: &Store, id: &TodoId) -> Result<(), Error> {
    if known(store)?.contains(id) {
        Ok(())
    } else {
        Err(Error::usage(format!(
            "no todo {:?} in this store",
            id.as_str()
        )))
    }
}

/// The shape `mk_completed`, `mk_cancelled` and `mk_reopened` share — §4's
/// three lifecycle kinds are one record type in the core for the same reason.
type MkLifecycle = fn(&str, Datetime, &str, &str) -> Result<TodoEvent<Todo>, ProdromeError>;

/// A lifecycle write — `done`, `cancel`, `reopen` — which differ only in the
/// constructor.
fn lifecycle(store: &Store, id: &str, stamp: &Stamp, make: MkLifecycle) -> Result<Outcome, Error> {
    let todo = TodoId::new(id)?;
    require_known(store, &todo)?;
    let at = price::instant(stamp.at.as_deref())?;
    let actor = actor_of(stamp)?;
    let digest = store.append(make(id, at, &actor, &stamp.note)?, None)?;
    Ok(Outcome::said(digest.as_str()))
}

/// Run one verb.
///
/// # Errors
///
/// Every refusal is an [`Error`]: an argument that means nothing, a todo the
/// store does not hold, or the store itself refusing.
pub fn run(cli: &Cli) -> Result<Outcome, Error> {
    // Opening a store touches no disk — it is a root and a policy — so `init`
    // sits in the same match as everything else even though its store is the
    // one that does not exist yet.
    let root = store_root(cli.store.as_deref());
    let store = Store::new(&root, policy_of(&cli.untrusted)?);

    match &cli.command {
        Command::Init { dir } => init(dir),

        Command::Add {
            id,
            body,
            detail,
            price: spec,
            stamp,
        } => add(&store, id, body, detail, spec, stamp),

        Command::Done { id, stamp } => lifecycle(&store, id, stamp, mk_completed),
        Command::Cancel { id, stamp } => lifecycle(&store, id, stamp, mk_cancelled),
        Command::Reopen { id, stamp } => lifecycle(&store, id, stamp, mk_reopened),

        Command::Revise { id, price, stamp } => {
            let todo = TodoId::new(id)?;
            require_known(&store, &todo)?;
            let spec = price::term_of(price)?
                .ok_or_else(|| Error::usage("revise needs a new spec: --priority or --deadline"))?;
            let at = price::instant(stamp.at.as_deref())?;
            let actor = actor_of(stamp)?;
            let digest = store.append(mk_spec_revised(id, at, &actor, spec, &stamp.note)?, None)?;
            Ok(Outcome::said(digest.as_str()))
        }

        Command::List { at } => {
            let reading = read(&store, price::instant(at.as_deref())?)?;
            Ok(Outcome::said(render::list(&reading)))
        }

        Command::Show { id, at } => {
            let todo = TodoId::new(id)?;
            let reading = read(&store, price::instant(at.as_deref())?)?;
            render::show(&reading, &todo)
                .map(Outcome::said)
                .ok_or_else(|| Error::usage(format!("no todo {id:?} in this store")))
        }

        Command::Verify => {
            let problems = store.verify();
            if problems.is_empty() {
                let objects = store.read_dag_named()?.len();
                let heads = store.tips().len();
                Ok(Outcome::said(format!(
                    "ok: {objects} objects, {heads} head{}",
                    if heads == 1 { "" } else { "s" }
                )))
            } else {
                Ok(Outcome {
                    text: problems.join("\n"),
                    ok: false,
                })
            }
        }

        Command::Weave => {
            let heads = store.tips();
            if heads.len() < 2 {
                return Ok(Outcome::said(format!(
                    "{} head: nothing to weave",
                    heads.len()
                )));
            }
            // NO EVENT AND NO ACTOR. A merge asserts structure, not a fact
            // about a todo (§3), so there is nothing here for a `--actor` to
            // be the author of.
            let digest = store.merge(None, None)?;
            Ok(Outcome::said(digest.as_str()))
        }
    }
}

/// `init`: an `objects/` directory, and nothing else.
///
/// No HEAD and no `refs/`: HEAD appears with the first object, and `refs/`
/// exists only while there is more than one head (§3). A store this creates is
/// therefore indistinguishable from an empty one somebody made with `mkdir`,
/// which is the point — the layout is the format, not this program's doing.
fn init(dir: &Path) -> Result<Outcome, Error> {
    if dir.join("objects").is_dir() {
        return Err(Error::usage(format!(
            "{} is already a store",
            dir.display()
        )));
    }
    std::fs::create_dir_all(dir.join("objects")).map_err(|e| ProdromeError::Io {
        path: dir.display().to_string(),
        message: e.to_string(),
    })?;
    Ok(Outcome::said(format!("initialised {}", dir.display())))
}

/// `add`: the `Created` that names the todo, the `SpecRevised` that prices it
/// when a price was given, and a record when there is a `--detail` to hold.
///
/// THREE OBJECTS AT MOST, AND THE PRICE LIVES IN ONE PLACE. A record can carry
/// a spec too (§6.2 reads one from either), and writing it in both would be
/// two spellings of one price to keep in step.
fn add(
    store: &Store,
    id: &str,
    body: &str,
    detail: &str,
    price_args: &Price,
    stamp: &Stamp,
) -> Result<Outcome, Error> {
    let todo = TodoId::new(id)?;
    if known(store)?.contains(&todo) {
        return Err(Error::usage(format!(
            "todo {id:?} is already in this store"
        )));
    }
    if body.trim().is_empty() {
        return Err(Error::usage("--body cannot be empty"));
    }
    let at = price::instant(stamp.at.as_deref())?;
    let actor = actor_of(stamp)?;

    let mut written = vec![store.append(mk_created(id, at, &actor, body, &stamp.note)?, None)?];
    if let Some(spec) = price::term_of(price_args)? {
        written.push(store.append(mk_spec_revised(id, at, &actor, spec, "")?, None)?);
    }
    if !detail.is_empty() {
        let record = mk_authored(
            id,
            at,
            &actor,
            "todo",
            at,
            body,
            None,
            Vec::new(),
            "",
            "",
            detail,
            None,
            Vec::new(),
            Vec::new(),
            "",
        )?;
        written.push(store.append(record, None)?);
    }
    Ok(Outcome::said(
        written
            .iter()
            .map(|digest| digest.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    ))
}
