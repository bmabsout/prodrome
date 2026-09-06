//! The Prodrome core, as a Python extension module.
//!
//! THIS FILE HAS NO SEMANTICS. Every function below parses its arguments out
//! of Python's types, calls exactly one thing in `prodrome-core`, and prints
//! the answer back — no fold, no evaluation, no normalisation of its own. That
//! is the whole design: SPEC §1 says fulfillment is computed in exactly one
//! place, and a binding that decided anything would be a second place. The
//! boundary is checkable by eye — if a body here contains a `match` over a
//! `TermF` or an `if` about trust, it is a bug.
//!
//! The types on the wire are Python's own, because the callers are Python
//! layers that already speak them:
//!
//! - a stored object, an event and a term all travel as their CANONICAL §2
//!   PRINT, a `str`. It is the identity of the value (§3: the name is the
//!   sha256 of the print), so it is the only representation that cannot drift
//!   between the two implementations, and comparing two of them is the
//!   differential test's sharpest instrument.
//! - an instant travels as `datetime.isoformat()`, microseconds only when
//!   non-zero — CPython's spelling, which `fpl::iso` reproduces.
//! - an environment travels as `{todo: {"kind": "Completed"|"Cancelled",
//!   "at": iso}}`, the shape `scripts/conformance.py` already writes.
//! - `explain`/`to_json` travel as the wire dicts §7 defines.
//!
//! Every refusal — a grammar error, a smart constructor's bounds, a store's
//! missing parent — arrives as `ValueError`, which is what the reference
//! raises and what `verify` and the fuzzer already catch.

use std::collections::BTreeMap;
use std::path::PathBuf;

use prodrome_core::breaks;
use prodrome_core::event::{self, Actor, Hash, TodoEvent};
use prodrome_core::fold::{self, Untrusted};
use prodrome_core::fpl::{self, Env as FplEnv, Instant, Outcome, Term};
use prodrome_core::literal;
use prodrome_core::registers::{self, Node};
use prodrome_core::store::EventStore;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyList, PyTuple};
use pyo3::IntoPyObjectExt;
use serde_json::Value as Json;

// --- refusals ----------------------------------------------------------------

/// Every core error, as the `ValueError` the reference raises.
fn refuse(error: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(error.to_string())
}

// --- the wire shapes ---------------------------------------------------------

/// `{todo: {"kind": "Completed" | "Cancelled", "at": iso}}` — §6.1's answer as
/// JSON-shaped Python, in and out.
type EnvWire = BTreeMap<String, BTreeMap<String, String>>;

/// One DAG node as the registers take it: `(hash, parents, event print | None)`
/// — a merge carries no event, which is what the `None` is.
type NodeWire = (String, Vec<String>, Option<String>);

fn untrusted_of(actors: Vec<String>) -> PyResult<Untrusted> {
    actors
        .into_iter()
        .map(Actor::new)
        .collect::<Result<Vec<_>, _>>()
        .map(Untrusted::of)
        .map_err(refuse)
}

fn instant_of(text: &str) -> PyResult<Instant> {
    fpl::parse_iso(text).map_err(refuse)
}

fn moment_of(text: &str) -> PyResult<literal::Datetime> {
    fpl::datetime_of(instant_of(text)?).map_err(refuse)
}

fn hash_of(text: &str) -> PyResult<Hash> {
    Hash::new(text).map_err(refuse)
}

fn event_of(text: &str) -> PyResult<TodoEvent> {
    event::parse_event(text).map_err(refuse)
}

fn events_of(prints: &[String]) -> PyResult<Vec<TodoEvent>> {
    prints.iter().map(|text| event_of(text)).collect()
}

fn term_of(text: &str) -> PyResult<Term> {
    fpl::parse_term(text).map_err(refuse)
}

fn print_term(term: &Term) -> String {
    fpl::print_term(term)
}

fn nodes_of(wire: &[NodeWire]) -> PyResult<Vec<Node>> {
    wire.iter()
        .map(|(name, parents, event)| {
            Ok(Node {
                name: hash_of(name)?,
                parents: parents
                    .iter()
                    .map(|parent| hash_of(parent))
                    .collect::<PyResult<Vec<_>>>()?,
                event: event.as_deref().map(event_of).transpose()?,
            })
        })
        .collect()
}

fn env_wire(env: &fold::Env) -> EnvWire {
    env.iter()
        .map(|(todo, binding)| {
            (
                todo.as_str().to_owned(),
                BTreeMap::from([
                    ("kind".to_owned(), binding.kind().to_owned()),
                    ("at".to_owned(), fpl::iso(binding.at())),
                ]),
            )
        })
        .collect()
}

fn fpl_env(wire: Option<&EnvWire>) -> PyResult<FplEnv> {
    let mut env = FplEnv::new();
    let Some(wire) = wire else { return Ok(env) };
    for (name, binding) in wire {
        let field = |key: &str| {
            binding
                .get(key)
                .ok_or_else(|| refuse(format!("env[{name:?}] is missing {key:?}")))
        };
        let at = instant_of(field("at")?)?;
        env.insert(
            name.clone(),
            match field("kind")?.as_str() {
                "Completed" => Outcome::Completed(at),
                "Cancelled" => Outcome::Cancelled(at),
                other => return Err(refuse(format!("unknown env kind {other:?}"))),
            },
        );
    }
    Ok(env)
}

fn prints_of<T>(
    map: &BTreeMap<prodrome_core::event::TodoId, T>,
    each: impl Fn(&T) -> String,
) -> BTreeMap<String, String> {
    map.iter()
        .map(|(todo, value)| (todo.as_str().to_owned(), each(value)))
        .collect()
}

// --- JSON, both directions ---------------------------------------------------

fn json_to_py(py: Python<'_>, value: &Json) -> PyResult<Py<PyAny>> {
    match value {
        Json::Null => Ok(py.None()),
        Json::Bool(flag) => flag.into_py_any(py),
        Json::Number(number) => match (number.as_i64(), number.as_f64()) {
            (Some(whole), _) => whole.into_py_any(py),
            (None, Some(real)) => real.into_py_any(py),
            (None, None) => Err(refuse(format!("{number} is not a Python number"))),
        },
        Json::String(text) => text.into_py_any(py),
        Json::Array(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(json_to_py(py, item)?)?;
            }
            list.into_py_any(py)
        }
        Json::Object(fields) => {
            let dict = PyDict::new(py);
            for (key, value) in fields {
                dict.set_item(key, json_to_py(py, value)?)?;
            }
            dict.into_py_any(py)
        }
    }
}

fn json_of_py(object: &Bound<'_, PyAny>) -> PyResult<Json> {
    if object.is_none() {
        return Ok(Json::Null);
    }
    // bool BEFORE int: Python's bool is an int subclass and `True` extracts as 1.
    if let Ok(flag) = object.cast::<PyBool>() {
        return Ok(Json::Bool(flag.is_true()));
    }
    if let Ok(dict) = object.cast::<PyDict>() {
        let mut fields = serde_json::Map::new();
        for (key, value) in dict.iter() {
            fields.insert(key.extract::<String>()?, json_of_py(&value)?);
        }
        return Ok(Json::Object(fields));
    }
    if object.cast::<PyList>().is_ok() || object.cast::<PyTuple>().is_ok() {
        return object
            .try_iter()?
            .map(|item| json_of_py(&item?))
            .collect::<PyResult<Vec<_>>>()
            .map(Json::Array);
    }
    if let Ok(whole) = object.extract::<i64>() {
        return Ok(Json::from(whole));
    }
    if let Ok(real) = object.extract::<f64>() {
        return Ok(Json::from(real));
    }
    if let Ok(text) = object.extract::<String>() {
        return Ok(Json::String(text));
    }
    Err(refuse(format!(
        "not a JSON value: {}",
        object.get_type().name()?
    )))
}

// --- §2: the grammar ---------------------------------------------------------

/// Parse one STORED OBJECT and print it back canonically.
///
/// The reference's `parse_literal(text, EVENT_CONSTRUCTORS)` composed with
/// `print_literal`, and an object is the only thing it is ever handed on real
/// data (`suzatary/prodrome/store.py::_load`). "Object" rather than "any value
/// of the grammar" is what makes the refusals match: the reference's parser
/// DISPATCHES INTO the smart constructors, so `Flat(value=2.0)` is a refusal
/// there, while `literal::parse_literal` alone knows names and arity and would
/// hand back a value §7 forbids. `parse_envelope` is the typed door, so every
/// bound — §7's on a nested spec included — is checked here too.
#[pyfunction]
fn parse_literal(text: &str) -> PyResult<String> {
    event::parse_envelope(text)
        .map(|envelope| event::canonical_envelope(&envelope))
        .map_err(refuse)
}

/// The canonical print of a canonical print — the identity, and the cheapest
/// statement of §2's "print ∘ parse = id" a caller can make.
#[pyfunction]
fn print_literal(text: &str) -> PyResult<String> {
    parse_literal(text)
}

/// §3: an object's name — `sha256(utf8(print(envelope)))`, lowercase hex.
#[pyfunction]
fn seal_hash(text: &str) -> PyResult<String> {
    event::parse_envelope(text)
        .map(|envelope| event::seal_hash(&envelope).as_str().to_owned())
        .map_err(refuse)
}

// --- §3: the store -----------------------------------------------------------

/// A chain (or a DAG) rooted at a directory. `untrusted` is §5's whole policy.
#[pyclass(module = "prodrome")]
struct Store {
    inner: EventStore,
}

#[pymethods]
impl Store {
    #[new]
    #[pyo3(signature = (root, untrusted = Vec::new()))]
    fn new(root: PathBuf, untrusted: Vec<String>) -> PyResult<Self> {
        let policy = untrusted_of(untrusted)?;
        Ok(Store {
            inner: EventStore::new(root, policy.actors().clone()),
        })
    }

    #[getter]
    fn root(&self) -> String {
        self.inner.root().display().to_string()
    }

    /// The single head HEAD names, or `None` where there is no chain.
    fn tip(&self) -> Option<String> {
        self.inner.tip().map(|head| head.as_str().to_owned())
    }

    /// Every head, sorted.
    fn tips(&self) -> Vec<String> {
        self.inner
            .tips()
            .iter()
            .map(|head| head.as_str().to_owned())
            .collect()
    }

    /// Every object from every head in the linearisation's order, as
    /// `(hash, parents, event print | None)` — the shape `fold_registers`
    /// takes, so a DAG reads straight into the registers.
    fn read_dag(&self) -> PyResult<Vec<NodeWire>> {
        Ok(self
            .inner
            .read_dag_named()
            .map_err(refuse)?
            .into_iter()
            .map(|(name, envelope)| {
                (
                    name.as_str().to_owned(),
                    event::parents_of(&envelope)
                        .iter()
                        .map(|parent| parent.as_str().to_owned())
                        .collect(),
                    envelope.event().map(event::canonical),
                )
            })
            .collect())
    }

    /// The same read as `(hash, object print)` — the bytes on disk, reverified
    /// on the way out, for a caller comparing storage and not structure.
    fn objects(&self) -> PyResult<Vec<(String, String)>> {
        Ok(self
            .inner
            .read_dag_named()
            .map_err(refuse)?
            .into_iter()
            .map(|(name, envelope)| {
                (
                    name.as_str().to_owned(),
                    event::canonical_envelope(&envelope),
                )
            })
            .collect())
    }

    /// The event bodies, in the linearisation's order; a merge contributes none.
    fn events(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .events()
            .map_err(refuse)?
            .iter()
            .map(event::canonical)
            .collect())
    }

    /// Full fsck; an empty list means healthy.
    fn verify(&self) -> Vec<String> {
        self.inner.verify()
    }

    /// Seal an event onto `parents` (the current tip when absent) and advance.
    #[pyo3(signature = (event, parents = None))]
    fn append(&self, event: &str, parents: Option<Vec<String>>) -> PyResult<String> {
        let on = parents
            .map(|names| {
                names
                    .iter()
                    .map(|name| hash_of(name))
                    .collect::<PyResult<Vec<_>>>()
            })
            .transpose()?;
        self.inner
            .append(event_of(event)?, on.as_deref())
            .map(|name| name.as_str().to_owned())
            .map_err(refuse)
    }

    /// Join heads into one `Woven`, which becomes the tip.
    #[pyo3(signature = (parents = None, event = None))]
    fn merge(&self, parents: Option<Vec<String>>, event: Option<&str>) -> PyResult<String> {
        let on = parents
            .map(|names| {
                names
                    .iter()
                    .map(|name| hash_of(name))
                    .collect::<PyResult<Vec<_>>>()
            })
            .transpose()?;
        let event = event.map(event_of).transpose()?;
        self.inner
            .merge(on.as_deref(), event)
            .map(|name| name.as_str().to_owned())
            .map_err(refuse)
    }

    /// Take another store's objects in and make `digest` a head here.
    fn adopt(&self, source: PyRef<'_, Store>, digest: &str) -> PyResult<String> {
        self.inner
            .adopt(&source.inner, &hash_of(digest)?)
            .map(|name| name.as_str().to_owned())
            .map_err(refuse)
    }
}

// --- §6.1–6.5: the folds -----------------------------------------------------

#[pyfunction]
#[pyo3(signature = (events, t, untrusted = Vec::new()))]
fn env_at(events: Vec<String>, t: &str, untrusted: Vec<String>) -> PyResult<EnvWire> {
    Ok(env_wire(&fold::env_at(
        &events_of(&events)?,
        moment_of(t)?,
        &untrusted_of(untrusted)?,
    )))
}

#[pyfunction]
#[pyo3(signature = (events, t, untrusted = Vec::new()))]
fn specs_at(
    events: Vec<String>,
    t: &str,
    untrusted: Vec<String>,
) -> PyResult<BTreeMap<String, String>> {
    let specs = fold::specs_at(
        &events_of(&events)?,
        moment_of(t)?,
        &untrusted_of(untrusted)?,
    );
    Ok(prints_of(&specs, print_term))
}

/// §6.3 takes no trust policy, deliberately: content from every actor renders.
#[pyfunction]
fn authored_at(events: Vec<String>, t: &str) -> PyResult<BTreeMap<String, String>> {
    let content = fold::authored_at(&events_of(&events)?, moment_of(t)?);
    Ok(prints_of(&content, |record| {
        literal::print_literal(&record.to_value())
    }))
}

#[pyfunction]
#[pyo3(signature = (events, t, untrusted = Vec::new()))]
fn flatten(
    events: Vec<String>,
    t: &str,
    untrusted: Vec<String>,
) -> PyResult<BTreeMap<String, String>> {
    let flat = fold::flatten(
        &events_of(&events)?,
        moment_of(t)?,
        &untrusted_of(untrusted)?,
    )
    .map_err(refuse)?;
    Ok(prints_of(&flat, print_term))
}

#[pyfunction]
#[pyo3(signature = (events, t, untrusted = Vec::new()))]
fn history_at(events: Vec<String>, t: &str, untrusted: Vec<String>) -> PyResult<EnvWire> {
    let past = fold::history(&events_of(&events)?, &untrusted_of(untrusted)?);
    Ok(env_wire(&past.at(moment_of(t)?)))
}

/// §6.5 — the environment as a FUNCTION OF TIME, folded once and asked many
/// times. `series_knots` takes one of these, because a curve reads the
/// environment as of every instant it draws.
#[pyclass(module = "prodrome", name = "History")]
struct PyHistory {
    inner: fold::History,
}

#[pymethods]
impl PyHistory {
    #[new]
    #[pyo3(signature = (events, untrusted = Vec::new()))]
    fn new(events: Vec<String>, untrusted: Vec<String>) -> PyResult<Self> {
        Ok(PyHistory {
            inner: fold::history(&events_of(&events)?, &untrusted_of(untrusted)?),
        })
    }

    fn at(&self, t: &str) -> PyResult<EnvWire> {
        Ok(env_wire(&self.inner.at(moment_of(t)?)))
    }

    /// Per todo, the `(instant, kind | None)` transitions the chain records —
    /// `None` is a `Reopened` clearing the binding.
    fn bindings(&self) -> BTreeMap<String, Vec<(String, Option<String>)>> {
        self.inner
            .bindings()
            .iter()
            .map(|(todo, timeline)| {
                (
                    todo.as_str().to_owned(),
                    timeline
                        .iter()
                        .map(|(at, binding)| {
                            (fpl::iso(*at), binding.map(|held| held.kind().to_owned()))
                        })
                        .collect(),
                )
            })
            .collect()
    }
}

// --- §6.6: the registers -----------------------------------------------------

/// The register state over a DAG — a monoid action's carrier, so `extend`
/// returns a new one rather than mutating this.
#[pyclass(module = "prodrome", name = "Folded")]
struct PyFolded {
    inner: registers::Folded,
}

#[pymethods]
impl PyFolded {
    fn env_of(&self) -> EnvWire {
        env_wire(&registers::env_of(&self.inner))
    }

    fn specs_of(&self) -> BTreeMap<String, String> {
        prints_of(&registers::specs_of(&self.inner), print_term)
    }

    fn content_of(&self) -> BTreeMap<String, String> {
        prints_of(&registers::content_of(&self.inner), |record| {
            literal::print_literal(&record.to_value())
        })
    }

    /// Every register with more than one live write, by todo and register name,
    /// each write named by the object that made it.
    fn conflicts_of(&self) -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
        registers::conflicts_of(&self.inner)
            .iter()
            .map(|(todo, by_kind)| {
                (
                    todo.as_str().to_owned(),
                    by_kind
                        .iter()
                        .map(|(kind, frontier)| {
                            (
                                kind.as_str().to_owned(),
                                frontier
                                    .writes()
                                    .iter()
                                    .map(|write| write.at.as_str().to_owned())
                                    .collect(),
                            )
                        })
                        .collect(),
                )
            })
            .collect()
    }

    #[pyo3(signature = (nodes, t = None, untrusted = Vec::new()))]
    fn extend(
        &self,
        nodes: Vec<NodeWire>,
        t: Option<&str>,
        untrusted: Vec<String>,
    ) -> PyResult<PyFolded> {
        Ok(PyFolded {
            inner: registers::extend(
                &self.inner,
                &nodes_of(&nodes)?,
                t.map(moment_of).transpose()?,
                &untrusted_of(untrusted)?,
            ),
        })
    }
}

#[pyfunction]
#[pyo3(signature = (nodes, t = None, untrusted = Vec::new()))]
fn fold_registers(
    nodes: Vec<NodeWire>,
    t: Option<&str>,
    untrusted: Vec<String>,
) -> PyResult<PyFolded> {
    Ok(PyFolded {
        inner: registers::fold(
            &nodes_of(&nodes)?,
            t.map(moment_of).transpose()?,
            &untrusted_of(untrusted)?,
        ),
    })
}

// --- §7: FPL -----------------------------------------------------------------

#[pyfunction]
#[pyo3(signature = (term, now, env = None))]
fn fulfillment(term: &str, now: &str, env: Option<EnvWire>) -> PyResult<f64> {
    Ok(fpl::fulfillment(
        &term_of(term)?,
        instant_of(now)?,
        &fpl_env(env.as_ref())?,
    ))
}

#[pyfunction]
#[pyo3(signature = (term, now, env = None))]
fn explain(py: Python<'_>, term: &str, now: &str, env: Option<EnvWire>) -> PyResult<Py<PyAny>> {
    let tree = fpl::explain(&term_of(term)?, instant_of(now)?, &fpl_env(env.as_ref())?);
    json_to_py(py, &tree)
}

#[pyfunction]
fn normalize(term: &str) -> PyResult<String> {
    Ok(print_term(&fpl::normalize(&term_of(term)?)))
}

#[pyfunction]
fn to_json(py: Python<'_>, term: &str) -> PyResult<Py<PyAny>> {
    json_to_py(py, &fpl::to_json(&term_of(term)?))
}

#[pyfunction]
fn from_json(wire: &Bound<'_, PyAny>) -> PyResult<String> {
    fpl::from_json(&json_of_py(wire)?)
        .map(|term| print_term(&term))
        .map_err(refuse)
}

/// §6.4's head: `own` when there are no items, a conjunction of `items` halves
/// when there is no own price, and `own` offsetting that conjunction otherwise.
/// `None` when a todo has neither.
#[pyfunction]
#[pyo3(signature = (own, items))]
fn checklist(own: Option<&str>, items: usize) -> PyResult<Option<String>> {
    let own = own.map(term_of).transpose()?;
    Ok(fpl::checklist(own, items)
        .map_err(refuse)?
        .as_ref()
        .map(print_term))
}

/// §7's knots: the curve a graph draws for one term over `[from, to]`, and
/// whether those knots ARE the curve. Every knot is evaluated against the
/// environment AS OF its own instant, which is what `history` is for.
#[pyfunction]
fn series_knots(
    term: &str,
    r#from: &str,
    to: &str,
    history: PyRef<'_, PyHistory>,
) -> PyResult<(Vec<(String, f64)>, bool)> {
    let past = &history.inner;
    let series = breaks::series_knots(
        &term_of(term)?,
        instant_of(r#from)?,
        instant_of(to)?,
        |at| {
            fpl::datetime_of(at)
                .map(|moment| fold::evaluation_env(&past.at(moment)))
                .unwrap_or_default()
        },
    );
    Ok((
        series
            .knots
            .iter()
            .map(|knot| (fpl::iso(knot.at), knot.value))
            .collect(),
        series.exact,
    ))
}

// --- the module --------------------------------------------------------------

#[pymodule]
fn prodrome(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("PRIORITY_POWER", fpl::PRIORITY_POWER)?;
    m.add("WITHIN_SAMPLES", fpl::WITHIN_SAMPLES)?;
    m.add("SAMPLES", breaks::SAMPLES)?;
    m.add_class::<Store>()?;
    m.add_class::<PyHistory>()?;
    m.add_class::<PyFolded>()?;
    m.add_function(wrap_pyfunction!(parse_literal, m)?)?;
    m.add_function(wrap_pyfunction!(print_literal, m)?)?;
    m.add_function(wrap_pyfunction!(seal_hash, m)?)?;
    m.add_function(wrap_pyfunction!(env_at, m)?)?;
    m.add_function(wrap_pyfunction!(specs_at, m)?)?;
    m.add_function(wrap_pyfunction!(authored_at, m)?)?;
    m.add_function(wrap_pyfunction!(flatten, m)?)?;
    m.add_function(wrap_pyfunction!(history_at, m)?)?;
    m.add_function(wrap_pyfunction!(fold_registers, m)?)?;
    m.add_function(wrap_pyfunction!(fulfillment, m)?)?;
    m.add_function(wrap_pyfunction!(explain, m)?)?;
    m.add_function(wrap_pyfunction!(normalize, m)?)?;
    m.add_function(wrap_pyfunction!(to_json, m)?)?;
    m.add_function(wrap_pyfunction!(from_json, m)?)?;
    m.add_function(wrap_pyfunction!(checklist, m)?)?;
    m.add_function(wrap_pyfunction!(series_knots, m)?)?;
    Ok(())
}
