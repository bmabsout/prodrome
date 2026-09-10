//! THE CHECKED MIGRATION: `conformance/*.json` → `conformance/*.py`.
//!
//! The vectors are frozen evidence with no generator left to re-run — the
//! reference they were taken from is gone (see `CHANGELOG.md` 0.2.0) — so
//! moving them into this crate's own grammar (SPEC §2) is a TRANSLATION that
//! has to be proved, not a refresh. For every vector file this example:
//!
//! 1. reads the JSON;
//! 2. builds the literal `Value` the vector grammar says it is
//!    (`tests/common/vectors.rs`'s `VECTORS`);
//! 3. PRINTS it with the crate's own printer and PARSES it back with the
//!    crate's own parser, and asserts the two literals are equal — which is
//!    §9.1's round trip, applied to the evidence instead of to a stored
//!    object;
//! 4. maps the reparsed literal BACK to JSON and asserts it equals, exactly,
//!    the JSON it started from — every key, every string, every float bit for
//!    bit. Nothing is compared to a tolerance here: a translation that lost a
//!    digit is a translation that lost the evidence.
//!
//! [`rendered`] is that whole procedure and returns the bytes; nothing here
//! writes a file. `examples/migrate_vectors.rs` calls it and writes, once, by
//! hand; `tests/migration.rs` calls it and compares with what is on disk, so
//! the translation is checked by `cargo test` for as long as both formats are
//! in the tree.
//!
//! This file and the JSON go together, in the commit that switches the suites
//! over to the literals. Its parent commit is where anyone re-runs it.

#[path = "vectors.rs"]
mod vectors;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use prodrome::fpl::{self, instant_of, iso};
use prodrome::literal::{parse_literal, print_literal, Datetime, Value};
use serde_json::{Map, Value as Json};
use vectors::VECTORS;

// --- the small conversions ---------------------------------------------------

pub fn s(text: &str) -> Value {
    Value::Str(text.to_owned())
}

pub fn tuple(items: Vec<Value>) -> Value {
    Value::Tuple(items)
}

pub fn call(name: &str, fields: Vec<(&str, Value)>) -> Value {
    Value::call(
        name,
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

/// An ISO instant, as §2's `datetime(...)`. `isoformat(" ")` — what the view
/// vectors' `at` carries — is the same instant with a space, so both arrive
/// here.
pub fn moment(text: &str) -> Value {
    let parsed = fpl::parse_iso(&text.replace(' ', "T"))
        .unwrap_or_else(|e| panic!("not an instant: {text:?} ({e})"));
    Value::Datetime(fpl::datetime_of(parsed).expect("inside the grammar's years"))
}

pub fn number(n: &Json) -> Value {
    match n {
        Json::Number(num) => {
            if let Some(i) = num.as_i64() {
                if !num.is_f64() {
                    return Value::int(i);
                }
            }
            Value::float(num.as_f64().expect("a JSON number")).expect("finite")
        }
        other => panic!("expected a number, got {other}"),
    }
}

fn text_of<'a>(v: &'a Json, key: &str) -> &'a str {
    v[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} is not a string in {v}"))
}

fn array_of<'a>(v: &'a Json, key: &str) -> &'a Vec<Json> {
    v[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} is not an array in {v}"))
}

fn object_of<'a>(v: &'a Json, key: &str) -> &'a Map<String, Json> {
    v[key]
        .as_object()
        .unwrap_or_else(|| panic!("{key} is not an object in {v}"))
}

pub fn strings(v: &Json, key: &str) -> Value {
    tuple(
        array_of(v, key)
            .iter()
            .map(|x| s(x.as_str().expect("a string")))
            .collect(),
    )
}

/// `{todo: {kind, at}}` — §6.1's environment — as a tuple of `Bound(...)`.
pub fn bounds(v: &Json, key: &str) -> Value {
    tuple(
        object_of(v, key)
            .iter()
            .map(|(todo, outcome)| {
                call(
                    "Bound",
                    vec![
                        ("todo", s(todo)),
                        ("kind", s(text_of(outcome, "kind"))),
                        ("at", moment(text_of(outcome, "at"))),
                    ],
                )
            })
            .collect(),
    )
}

/// `{todo: "<print>"}` as a tuple of one two-field constructor.
pub fn by_todo(v: &Json, key: &str, name: &str, field: &str) -> Value {
    tuple(
        object_of(v, key)
            .iter()
            .map(|(todo, print)| {
                call(
                    name,
                    vec![
                        ("todo", s(todo)),
                        (field, s(print.as_str().expect("a print"))),
                    ],
                )
            })
            .collect(),
    )
}

// --- forward: one file at a time ---------------------------------------------

pub fn folds(doc: &Json) -> Value {
    let logs = array_of(doc, "logs")
        .iter()
        .map(|log| {
            call(
                "FoldCase",
                vec![
                    ("seed", number(&log["seed"])),
                    ("untrusted", strings(log, "untrusted")),
                    ("events", strings(log, "events")),
                    ("at", moment(text_of(log, "at"))),
                    ("env", bounds(log, "env")),
                    ("specs", by_todo(log, "specs", "Spec", "term")),
                    ("content", by_todo(log, "content", "Content", "event")),
                    ("flatten", by_todo(log, "flatten", "Spec", "term")),
                    ("history_at", bounds(log, "history_at")),
                ],
            )
        })
        .collect();
    call("Folds", vec![("logs", tuple(logs))])
}

pub fn dags(doc: &Json) -> Value {
    let dags = array_of(doc, "dags")
        .iter()
        .map(|dag| {
            let objects = tuple(
                object_of(dag, "objects")
                    .iter()
                    .map(|(name, literal)| {
                        call(
                            "Object",
                            vec![
                                ("name", s(name)),
                                ("literal", s(literal.as_str().expect("a print"))),
                            ],
                        )
                    })
                    .collect(),
            );
            let parents = tuple(
                object_of(dag, "parents")
                    .iter()
                    .map(|(name, parents)| {
                        call(
                            "Parents",
                            vec![
                                ("name", s(name)),
                                (
                                    "parents",
                                    tuple(
                                        parents
                                            .as_array()
                                            .expect("an array")
                                            .iter()
                                            .map(|p| s(p.as_str().expect("a name")))
                                            .collect(),
                                    ),
                                ),
                            ],
                        )
                    })
                    .collect(),
            );
            let conflicts = tuple(
                object_of(dag, "conflicts")
                    .iter()
                    .flat_map(|(todo, by_kind)| {
                        by_kind
                            .as_object()
                            .expect("an object")
                            .iter()
                            .map(|(kind, writes)| {
                                call(
                                    "Conflict",
                                    vec![
                                        ("todo", s(todo)),
                                        ("kind", s(kind)),
                                        (
                                            "writes",
                                            tuple(
                                                writes
                                                    .as_array()
                                                    .expect("an array")
                                                    .iter()
                                                    .map(|w| s(w.as_str().expect("a name")))
                                                    .collect(),
                                            ),
                                        ),
                                    ],
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect(),
            );
            call(
                "DagCase",
                vec![
                    ("seed", number(&dag["seed"])),
                    ("objects", objects),
                    ("tips", strings(dag, "tips")),
                    ("linearisation", strings(dag, "linearisation")),
                    ("parents", parents),
                    ("verify", strings(dag, "verify")),
                    ("env", bounds(dag, "env")),
                    ("conflicts", conflicts),
                ],
            )
        })
        .collect();
    call("Dags", vec![("dags", tuple(dags))])
}

/// One `explain` JSON node as the vector grammar's `Node(...)`: the kind tag,
/// the value, the notes, and the children in the order `TermF::children`
/// yields them. The keys that are CHILDREN are named per kind; everything else
/// on the node is a note, which is exactly the split `fpl::explanation_json`
/// made when it wrote the tree.
pub fn node(v: &Json) -> Value {
    let kind = text_of(v, "kind");
    let object = v.as_object().expect("a node is an object");
    let child_keys: &[&str] = match kind {
        "conj" => &["terms"],
        "offset" | "shift" | "within" | "importance" => &["term"],
        "gate" => &["gate", "body"],
        "after" => &["term", "pending"],
        "offsetBy" => &["delta", "term"],
        // The piece in force rides in the `head` slot and PRINTS as "term"
        // (`fpl::explanation_json`), which is the one place the explanation's
        // shape bends away from the term's.
        "piecewise" => &["term"],
        _ => &[],
    };
    let mut children: Vec<Value> = Vec::new();
    for key in child_keys {
        match &object[*key] {
            Json::Array(items) => children.extend(items.iter().map(node)),
            one => children.push(node(one)),
        }
    }
    // A `flat` node KEEPS its `value` note. `explanation_json` writes the
    // annotation and the notes into one map, so Flat's own `value` field and
    // the node's fulfillment land on the same key — and they are the same
    // number by definition (`fulfillment(Flat(v)) == v`), so reading the key
    // back as both loses nothing and invents nothing. Every other kind names
    // its scalars differently and has no such collision.
    let notes: Vec<Value> = object
        .iter()
        .filter(|(key, _)| {
            key.as_str() != "kind"
                && (kind == "flat" || key.as_str() != "value")
                && !child_keys.contains(&key.as_str())
        })
        .map(|(key, note)| {
            call(
                "NoteEntry",
                vec![("key", s(key)), ("note", note_value(note))],
            )
        })
        .collect();
    call(
        "Node",
        vec![
            ("kind", s(kind)),
            ("value", number(&object["value"])),
            ("notes", tuple(notes)),
            ("terms", tuple(children)),
        ],
    )
}

/// `fpl::Note`'s three arms, as the JSON wrote them: a scalar, a list of
/// scalars, or a list of maps of scalars.
pub fn note_value(note: &Json) -> Value {
    match note {
        Json::Array(items) => match items.first() {
            Some(Json::Object(_)) => call(
                "Maps",
                vec![(
                    "maps",
                    tuple(
                        items
                            .iter()
                            .map(|m| {
                                call(
                                    "Fields",
                                    vec![(
                                        "pairs",
                                        tuple(
                                            m.as_object()
                                                .expect("a map")
                                                .iter()
                                                .map(|(k, v)| {
                                                    call(
                                                        "Pair",
                                                        vec![("key", s(k)), ("value", scalar(v))],
                                                    )
                                                })
                                                .collect(),
                                        ),
                                    )],
                                )
                            })
                            .collect(),
                    ),
                )],
            ),
            _ => call(
                "Many",
                vec![("values", tuple(items.iter().map(scalar).collect()))],
            ),
        },
        one => call("One", vec![("value", scalar(one))]),
    }
}

pub fn scalar(v: &Json) -> Value {
    match v {
        Json::String(text) => s(text),
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(_) => number(v),
        other => panic!("a note scalar is a string, a number or a bool, got {other}"),
    }
}

pub fn fpl_terms(doc: &Json) -> Value {
    let terms = array_of(doc, "terms")
        .iter()
        .map(|case| {
            let samples = tuple(
                array_of(case, "samples")
                    .iter()
                    .map(|sample| {
                        call(
                            "Sample",
                            vec![
                                ("now", moment(text_of(sample, "now"))),
                                ("value", number(&sample["value"])),
                            ],
                        )
                    })
                    .collect(),
            );
            call(
                "FplCase",
                vec![
                    ("seed", number(&case["seed"])),
                    ("term", s(text_of(case, "term"))),
                    ("normalized", s(text_of(case, "normalized"))),
                    ("env", bounds(case, "env")),
                    ("samples", samples),
                    ("explain_at", moment(text_of(case, "explain_at"))),
                    ("explain", node(&case["explain"])),
                ],
            )
        })
        .collect();
    call("Fpl", vec![("terms", tuple(terms))])
}

pub fn series(doc: &Json) -> Value {
    let cases = array_of(doc, "series")
        .iter()
        .map(|case| {
            let knots = tuple(
                array_of(case, "knots")
                    .iter()
                    .map(|knot| {
                        let pair = knot.as_array().expect("a knot is a pair");
                        call(
                            "Knot",
                            vec![
                                ("at", moment(pair[0].as_str().expect("an instant"))),
                                ("value", number(&pair[1])),
                            ],
                        )
                    })
                    .collect(),
            );
            call(
                "SeriesCase",
                vec![
                    ("seed", number(&case["seed"])),
                    ("term", s(text_of(case, "term"))),
                    ("from", moment(text_of(case, "from"))),
                    ("to", moment(text_of(case, "to"))),
                    (
                        "exact",
                        Value::Bool(case["exact"].as_bool().expect("a bool")),
                    ),
                    ("knots", knots),
                ],
            )
        })
        .collect();
    call("Series", vec![("series", tuple(cases))])
}

pub fn row(entry: &Json) -> Value {
    let conflicts = tuple(
        object_of(entry, "conflicts")
            .iter()
            .map(|(kind, writes)| {
                call(
                    "RowConflict",
                    vec![
                        ("kind", s(kind)),
                        (
                            "writes",
                            tuple(
                                writes
                                    .as_array()
                                    .expect("an array")
                                    .iter()
                                    .map(|w| s(w.as_str().expect("a name")))
                                    .collect(),
                            ),
                        ),
                    ],
                )
            })
            .collect(),
    );
    let optional = |key: &str| match &entry[key] {
        Json::Null => Value::None,
        other => s(other.as_str().expect("a string or null")),
    };
    call(
        "Row",
        vec![
            ("todo", s(text_of(entry, "todo"))),
            ("state", s(text_of(entry, "state"))),
            ("at", s(text_of(entry, "at"))),
            ("claimed", s(text_of(entry, "claimed"))),
            (
                "value",
                match &entry["value"] {
                    Json::Null => Value::None,
                    other => number(other),
                },
            ),
            (
                "unconfirmed",
                Value::Bool(entry["unconfirmed"].as_bool().expect("a bool")),
            ),
            ("conflicts", conflicts),
            ("content", optional("content")),
            ("spec", optional("spec")),
            ("stream", strings(entry, "stream")),
        ],
    )
}

pub fn view(doc: &Json) -> Value {
    let cases = array_of(doc, "cases")
        .iter()
        .map(|case| {
            let instants = tuple(
                array_of(case, "instants")
                    .iter()
                    .map(|asked| {
                        call(
                            "Asked",
                            vec![
                                ("at", moment(text_of(asked, "at"))),
                                (
                                    "entries",
                                    tuple(array_of(asked, "entries").iter().map(row).collect()),
                                ),
                            ],
                        )
                    })
                    .collect(),
            );
            call(
                "ViewCase",
                vec![
                    ("seed", number(&case["seed"])),
                    ("events", strings(case, "events")),
                    ("instants", instants),
                ],
            )
        })
        .collect();
    call(
        "View",
        vec![
            ("policy", s(text_of(doc, "policy"))),
            ("untrusted", strings(doc, "untrusted")),
            ("cases", tuple(cases)),
        ],
    )
}

// --- backward: the literal, as the JSON it came from -------------------------
//
// The half that makes step 4 a proof rather than a hope. It is written from
// the vector grammar alone — one match on the constructor name — and never
// looks at the JSON it is compared against.

pub fn iso_of(at: Datetime, spaced: bool) -> Json {
    let text = iso(instant_of(at));
    Json::String(if spaced { text.replace('T', " ") } else { text })
}

pub fn back_scalar(value: &Value) -> Json {
    match value {
        Value::None => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Str(text) => Json::String(text.clone()),
        Value::Int(i) => Json::from(i.as_i64().expect("fits an i64")),
        Value::Float(f) => Json::from(f.get()),
        other => panic!("not a scalar: {other:?}"),
    }
}

fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value
        .as_call()
        .and_then(|c| c.field(key))
        .unwrap_or_else(|| panic!("no {key} on {value:?}"))
}

pub fn items(value: &Value) -> &[Value] {
    value.as_tuple().expect("a tuple")
}

pub fn name_of(value: &Value) -> &str {
    &value.as_call().expect("a call").name
}

pub fn back_strings(value: &Value) -> Json {
    Json::Array(items(value).iter().map(back_scalar).collect())
}

/// A tuple of two- or three-field constructors, as the JSON object it was.
/// `keyed_by` names the field that was the key; the rest are the value, and a
/// single remaining field is the value itself rather than an object of one.
pub fn back_map(value: &Value, keyed_by: &str) -> Json {
    let mut out = Map::new();
    for item in items(value) {
        let call = item.as_call().expect("a call");
        let key = field(item, keyed_by).as_str().expect("a string key");
        let rest: Vec<&(String, Value)> =
            call.fields.iter().filter(|(k, _)| k != keyed_by).collect();
        let json = match rest.as_slice() {
            [(_, only)] => match only {
                Value::Tuple(_) => back_strings(only),
                other => back_scalar(other),
            },
            fields => Json::Object(
                fields
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            match v {
                                Value::Datetime(at) => iso_of(*at, false),
                                Value::Tuple(_) => back_strings(v),
                                other => back_scalar(other),
                            },
                        )
                    })
                    .collect(),
            ),
        };
        out.insert(key.to_owned(), json);
    }
    Json::Object(out)
}

pub fn back_conflicts(value: &Value) -> Json {
    let mut out: BTreeMap<String, Map<String, Json>> = BTreeMap::new();
    for item in items(value) {
        let todo = field(item, "todo").as_str().expect("a todo").to_owned();
        let kind = field(item, "kind").as_str().expect("a kind").to_owned();
        out.entry(todo)
            .or_default()
            .insert(kind, back_strings(field(item, "writes")));
    }
    Json::Object(
        out.into_iter()
            .map(|(todo, by_kind)| (todo, Json::Object(by_kind)))
            .collect(),
    )
}

pub fn back_note(value: &Value) -> Json {
    match name_of(value) {
        "One" => back_scalar(field(value, "value")),
        "Many" => Json::Array(
            items(field(value, "values"))
                .iter()
                .map(back_scalar)
                .collect(),
        ),
        "Maps" => Json::Array(
            items(field(value, "maps"))
                .iter()
                .map(|m| {
                    Json::Object(
                        items(field(m, "pairs"))
                            .iter()
                            .map(|p| {
                                (
                                    field(p, "key").as_str().expect("a key").to_owned(),
                                    back_scalar(field(p, "value")),
                                )
                            })
                            .collect(),
                    )
                })
                .collect(),
        ),
        other => panic!("not a note: {other}"),
    }
}

pub fn back_node(value: &Value) -> Json {
    let kind = field(value, "kind").as_str().expect("a kind");
    let mut out = Map::new();
    out.insert("kind".into(), Json::String(kind.to_owned()));
    out.insert("value".into(), back_scalar(field(value, "value")));
    for note in items(field(value, "notes")) {
        out.insert(
            field(note, "key").as_str().expect("a key").to_owned(),
            back_note(field(note, "note")),
        );
    }
    let children: Vec<Json> = items(field(value, "terms")).iter().map(back_node).collect();
    match kind {
        "conj" => {
            out.insert("terms".into(), Json::Array(children));
        }
        "offset" | "shift" | "within" | "importance" => {
            out.insert("term".into(), children[0].clone());
        }
        "gate" => {
            out.insert("gate".into(), children[0].clone());
            out.insert("body".into(), children[1].clone());
        }
        "after" => {
            out.insert("term".into(), children[0].clone());
            out.insert("pending".into(), children[1].clone());
        }
        "offsetBy" => {
            out.insert("delta".into(), children[0].clone());
            out.insert("term".into(), children[1].clone());
        }
        "piecewise" => {
            out.insert("term".into(), children[0].clone());
        }
        _ => assert!(children.is_empty(), "{kind} has no children"),
    }
    Json::Object(out)
}

pub fn back_row(value: &Value) -> Json {
    let mut out = Map::new();
    for key in ["todo", "state", "at", "claimed"] {
        out.insert(key.into(), back_scalar(field(value, key)));
    }
    out.insert("value".into(), back_scalar(field(value, "value")));
    out.insert(
        "unconfirmed".into(),
        back_scalar(field(value, "unconfirmed")),
    );
    out.insert("conflicts".into(), {
        let mut by_kind = Map::new();
        for held in items(field(value, "conflicts")) {
            by_kind.insert(
                field(held, "kind").as_str().expect("a kind").to_owned(),
                back_strings(field(held, "writes")),
            );
        }
        Json::Object(by_kind)
    });
    out.insert("content".into(), back_scalar(field(value, "content")));
    out.insert("spec".into(), back_scalar(field(value, "spec")));
    out.insert("stream".into(), back_strings(field(value, "stream")));
    Json::Object(out)
}

pub fn back(value: &Value) -> Json {
    let mut out = Map::new();
    match name_of(value) {
        "Folds" => {
            out.insert(
                "logs".into(),
                Json::Array(
                    items(field(value, "logs"))
                        .iter()
                        .map(|log| {
                            let mut m = Map::new();
                            m.insert("seed".into(), back_scalar(field(log, "seed")));
                            m.insert("untrusted".into(), back_strings(field(log, "untrusted")));
                            m.insert("events".into(), back_strings(field(log, "events")));
                            m.insert("at".into(), iso_of(datetime(log, "at"), false));
                            m.insert("env".into(), back_map(field(log, "env"), "todo"));
                            m.insert("specs".into(), back_map(field(log, "specs"), "todo"));
                            m.insert("content".into(), back_map(field(log, "content"), "todo"));
                            m.insert("flatten".into(), back_map(field(log, "flatten"), "todo"));
                            m.insert(
                                "history_at".into(),
                                back_map(field(log, "history_at"), "todo"),
                            );
                            Json::Object(m)
                        })
                        .collect(),
                ),
            );
        }
        "Dags" => {
            out.insert(
                "dags".into(),
                Json::Array(
                    items(field(value, "dags"))
                        .iter()
                        .map(|dag| {
                            let mut m = Map::new();
                            m.insert("seed".into(), back_scalar(field(dag, "seed")));
                            m.insert("objects".into(), back_map(field(dag, "objects"), "name"));
                            m.insert("tips".into(), back_strings(field(dag, "tips")));
                            m.insert(
                                "linearisation".into(),
                                back_strings(field(dag, "linearisation")),
                            );
                            m.insert("parents".into(), back_map(field(dag, "parents"), "name"));
                            m.insert("verify".into(), back_strings(field(dag, "verify")));
                            m.insert("env".into(), back_map(field(dag, "env"), "todo"));
                            m.insert("conflicts".into(), back_conflicts(field(dag, "conflicts")));
                            Json::Object(m)
                        })
                        .collect(),
                ),
            );
        }
        "Fpl" => {
            out.insert(
                "terms".into(),
                Json::Array(
                    items(field(value, "terms"))
                        .iter()
                        .map(|case| {
                            let mut m = Map::new();
                            m.insert("seed".into(), back_scalar(field(case, "seed")));
                            m.insert("term".into(), back_scalar(field(case, "term")));
                            m.insert("normalized".into(), back_scalar(field(case, "normalized")));
                            m.insert("env".into(), back_map(field(case, "env"), "todo"));
                            m.insert(
                                "samples".into(),
                                Json::Array(
                                    items(field(case, "samples"))
                                        .iter()
                                        .map(|sample| {
                                            let mut one = Map::new();
                                            one.insert(
                                                "now".into(),
                                                iso_of(datetime(sample, "now"), false),
                                            );
                                            one.insert(
                                                "value".into(),
                                                back_scalar(field(sample, "value")),
                                            );
                                            Json::Object(one)
                                        })
                                        .collect(),
                                ),
                            );
                            m.insert(
                                "explain_at".into(),
                                iso_of(datetime(case, "explain_at"), false),
                            );
                            m.insert("explain".into(), back_node(field(case, "explain")));
                            Json::Object(m)
                        })
                        .collect(),
                ),
            );
        }
        "Series" => {
            out.insert(
                "series".into(),
                Json::Array(
                    items(field(value, "series"))
                        .iter()
                        .map(|case| {
                            let mut m = Map::new();
                            m.insert("seed".into(), back_scalar(field(case, "seed")));
                            m.insert("term".into(), back_scalar(field(case, "term")));
                            m.insert("from".into(), iso_of(datetime(case, "from"), false));
                            m.insert("to".into(), iso_of(datetime(case, "to"), false));
                            m.insert("exact".into(), back_scalar(field(case, "exact")));
                            m.insert(
                                "knots".into(),
                                Json::Array(
                                    items(field(case, "knots"))
                                        .iter()
                                        .map(|knot| {
                                            Json::Array(vec![
                                                iso_of(datetime(knot, "at"), false),
                                                back_scalar(field(knot, "value")),
                                            ])
                                        })
                                        .collect(),
                                ),
                            );
                            Json::Object(m)
                        })
                        .collect(),
                ),
            );
        }
        "View" => {
            out.insert("policy".into(), back_scalar(field(value, "policy")));
            out.insert("untrusted".into(), back_strings(field(value, "untrusted")));
            out.insert(
                "cases".into(),
                Json::Array(
                    items(field(value, "cases"))
                        .iter()
                        .map(|case| {
                            let mut m = Map::new();
                            m.insert("seed".into(), back_scalar(field(case, "seed")));
                            m.insert("events".into(), back_strings(field(case, "events")));
                            m.insert(
                                "instants".into(),
                                Json::Array(
                                    items(field(case, "instants"))
                                        .iter()
                                        .map(|asked| {
                                            let mut one = Map::new();
                                            one.insert(
                                                "at".into(),
                                                iso_of(datetime(asked, "at"), true),
                                            );
                                            one.insert(
                                                "entries".into(),
                                                Json::Array(
                                                    items(field(asked, "entries"))
                                                        .iter()
                                                        .map(back_row)
                                                        .collect(),
                                                ),
                                            );
                                            Json::Object(one)
                                        })
                                        .collect(),
                                ),
                            );
                            Json::Object(m)
                        })
                        .collect(),
                ),
            );
        }
        other => panic!("not a vector root: {other}"),
    }
    Json::Object(out)
}

pub fn datetime(value: &Value, key: &str) -> Datetime {
    match field(value, key) {
        Value::Datetime(at) => *at,
        other => panic!("{key} is not a datetime: {other:?}"),
    }
}

/// The half of `fpl.json` that belongs to the JSON BOUNDARY rather than to the
/// core's evidence: `fpl::to_json`'s shape per term.
///
/// The codec moves to `prodrome-wasm` in this series — JSON is JavaScript's
/// literal grammar and the wasm crate is where JavaScript is — so its evidence
/// moves with it, to `wasm/conformance/term-json.json`, carrying what a test
/// there needs to rebuild each answer: the term's PRINT (parsed by the core),
/// the environment and the instant `explain` was taken at, and the two frozen
/// answers.
pub fn term_json_fixture(original: &Json) -> Json {
    let terms: Vec<Json> = array_of(original, "terms")
        .iter()
        .map(|case| {
            let mut one = Map::new();
            for key in ["term", "explain_at", "env", "json", "explain"] {
                one.insert(key.to_owned(), case[key].clone());
            }
            Json::Object(one)
        })
        .collect();
    let mut out = Map::new();
    out.insert("terms".into(), Json::Array(terms));
    Json::Object(out)
}

/// `rebuilt` is what the literal says. Put back what deliberately left it, from
/// the fixture that carries it — so the assertion that follows is "the literal
/// and that file together say exactly what the JSON said", and neither half
/// can go missing unnoticed. The fixture comes back beside the answer so the
/// caller can write it or check it.
pub fn restore(name: &str, mut rebuilt: Json, original: &Json) -> (Json, Option<Json>) {
    if name != "fpl" {
        return (rebuilt, None);
    }
    let fixture = term_json_fixture(original);
    let held = array_of(&fixture, "terms").clone();
    let cases = rebuilt["terms"].as_array_mut().expect("terms is an array");
    assert_eq!(cases.len(), held.len(), "the fixture lost a case");
    for (case, kept) in cases.iter_mut().zip(&held) {
        case["json"] = kept["json"].clone();
    }
    (rebuilt, Some(fixture))
}

// --- the run -----------------------------------------------------------------

pub fn conformance(name: &str) -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "..", "conformance", name]
        .iter()
        .collect()
}

/// The file's layout: ONE CASE PER LINE, each line the canonical print of that
/// case's own value.
///
/// A vector file is not a stored object — its bytes are not hashed, and what is
/// frozen is the VALUE, which the parse-back check pins. So the printer is
/// still the only thing that writes a value here, and the only bytes this adds
/// are the newlines between cases and a header comment (§2's parser skips
/// whitespace and `#` to end of line). 400 KB on one line would have been a
/// canonical print nobody could read a diff of, and evidence you cannot read is
/// evidence you cannot check.
pub fn lay_out(header: &str, root: &Value) -> String {
    let call = root.as_call().expect("a vector root is a call");
    let mut out = String::new();
    out.push_str(header);
    out.push_str(&call.name);
    out.push('(');
    for (index, (key, value)) in call.fields.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(key);
        out.push('=');
        match value {
            Value::Tuple(cases)
                if !cases.is_empty()
                    && cases
                        .iter()
                        .all(|c| c.as_call().is_some_and(|c| c.name.ends_with("Case"))) =>
            {
                out.push_str("(\n");
                for case in cases {
                    out.push_str(&print_literal(case));
                    out.push_str(",\n");
                }
                out.push(')');
            }
            inline => out.push_str(&print_literal(inline)),
        }
    }
    out.push_str(")\n");
    out
}

/// The first place two JSON trees differ, as a path — an assertion on 400 KB
/// of `Debug` says only that something is wrong.
pub fn differ(path: &str, a: &Json, b: &Json) -> Option<String> {
    match (a, b) {
        (Json::Object(x), Json::Object(y)) => {
            let (xs, ys): (Vec<&String>, Vec<&String>) = (x.keys().collect(), y.keys().collect());
            if xs != ys {
                return Some(format!("{path}: keys {xs:?} != {ys:?}"));
            }
            x.iter()
                .find_map(|(k, v)| differ(&format!("{path}.{k}"), v, &y[k]))
        }
        (Json::Array(x), Json::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{path}: {} items != {}", x.len(), y.len()));
            }
            x.iter()
                .zip(y)
                .enumerate()
                .find_map(|(i, (v, w))| differ(&format!("{path}[{i}]"), v, w))
        }
        (x, y) if x == y => None,
        (x, y) => Some(format!("{path}: {x} != {y}")),
    }
}

/// THE PROCEDURE, once: read the JSON, build the literal, print it, parse it
/// back, and prove the two agree — and that the literal (plus what left for
/// the JSON boundary) says exactly what the JSON said.
///
/// Returns the bytes of the `.py` and, where a file's evidence splits, the
/// half that goes to `wasm/conformance/`. Writes nothing.
pub fn rendered(name: &str, forward: Forward) -> (String, Option<Json>) {
    let json_path = conformance(&format!("{name}.json"));
    let raw = fs::read_to_string(&json_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", json_path.display()));
    let json: Json =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{name}.json is not JSON: {e}"));

    let literal = forward(&json);
    let header = format!(
        "# conformance/{name}.py — SPEC §9 vectors, in the grammar of §2, read
# through `tests/common/vectors.rs`'s VECTORS vocabulary. Frozen evidence:
# nothing under `cargo test` writes this file. One case per line.\n"
    );
    let text = lay_out(&header, &literal);

    // §9.1's round trip, applied to the evidence: what the printer wrote, the
    // parser reads back as the same value.
    let reparsed = parse_literal(&text, &VECTORS)
        .unwrap_or_else(|e| panic!("{name}.py does not parse back: {e}"));
    assert_eq!(
        literal, reparsed,
        "{name}: print ∘ parse is not the identity"
    );

    // And the translation lost nothing: the literal, plus the half that left
    // for the crate where the JSON codec now lives, IS the JSON — exactly, not
    // to a tolerance.
    let (rebuilt, fixture) = restore(name, back(&reparsed), &json);
    if let Some(disagreement) = differ(name, &json, &rebuilt) {
        panic!("{name}: the literal does not say what the JSON said — {disagreement}");
    }
    (text, fixture)
}

/// The transform that reads one file's JSON into the vector grammar.
pub type Forward = fn(&Json) -> Value;

/// Every vector file, with the transform that reads it.
pub const FILES: &[(&str, Forward)] = &[
    ("folds", folds),
    ("dag", dags),
    ("fpl", fpl_terms),
    ("series", series),
    ("view/triage-untrusted", view),
    ("view/bassel-untrusted", view),
];

/// Where the half of `fpl.json` that is the JSON BOUNDARY's evidence lands.
pub fn fixture_path() -> PathBuf {
    [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "wasm",
        "conformance",
        "term-json.json",
    ]
    .iter()
    .collect()
}
