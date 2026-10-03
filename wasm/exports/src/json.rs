//! §7 as JSON, rendered from the term schema: lowercase kind tags, ISO
//! instants and spans in hours, frozen by `conformance/term-json.json`.

use std::collections::BTreeMap;

use prodrome::fpl::{
    delta_from_hours, explained, parse_iso, scalar, wire_key, Closed, Delta, Env, Explanation,
    FplError, Instant, Note, Scalar,
};
use prodrome::term::schema::{build, fields, Field, FieldSource, Fields, Slot};
use prodrome::term::Term;
use serde_json::{Map, Value};

fn scalar_json(s: &Scalar) -> Value {
    match s {
        Scalar::Text(t) => Value::String(t.clone()),
        Scalar::Float(f) => Value::from(*f),
        Scalar::Int(i) => Value::from(*i),
        Scalar::Bool(b) => Value::Bool(*b),
    }
}

fn note_json(n: &Note) -> Value {
    match n {
        Note::One(s) => scalar_json(s),
        Note::Many(xs) => Value::Array(xs.iter().map(scalar_json).collect()),
        Note::Maps(ms) => Value::Array(ms.iter().map(scalars_json).collect()),
    }
}

/// `to_json`'s shape with each node's `value` (`null` for `∅`) and notes, its
/// scalars read from the notes.
pub fn explanation_json(node: &Explanation) -> Value {
    let (kind, fields) = fields(&*node.node);
    let mut out = Map::new();
    out.insert("kind".into(), Value::String(node.node.kind().to_owned()));
    out.insert("value".into(), Value::from(node.value));
    for (k, n) in &node.notes {
        out.insert(k.clone(), note_json(n));
    }
    for (name, slot) in fields {
        // The piece in force rides in `Piecewise`'s `head` slot, keyed "term".
        let name = if kind == "Piecewise" { "term" } else { name };
        match slot {
            Slot::Child(child) => {
                out.insert(name.into(), explanation_json(child));
            }
            Slot::Children(children) => {
                out.insert(
                    name.into(),
                    children.into_iter().map(explanation_json).collect(),
                );
            }
            _ => {}
        }
    }
    Value::Object(out)
}

pub fn explain(term: &Closed, now: Instant, env: &Env) -> Value {
    explanation_json(&explained(term, now, env))
}

pub fn to_json(term: &Term) -> Value {
    let mut out = object(fields(term.out()).1);
    out.insert("kind".into(), Value::String(term.out().kind().to_owned()));
    Value::Object(out)
}

/// Every field, its leaves as their notes are; an unset optional or empty text omitted.
fn object(fields: Fields<&Term>) -> Map<String, Value> {
    fields
        .into_iter()
        .filter_map(|(name, slot)| {
            let key = wire_key(name, matches!(slot, Slot::Span(_)));
            let value = match slot {
                Slot::Child(term) => to_json(term),
                Slot::Children(terms) => terms.into_iter().map(to_json).collect(),
                Slot::Rows(_, rows) => rows
                    .into_iter()
                    .map(|row| Value::Object(object(row)))
                    .collect(),
                leaf => scalar_json(&scalar(&leaf)?),
            };
            Some((key, value))
        })
        .collect()
}

/// A map of scalars as a JSON object.
pub fn scalars_json(scalars: &BTreeMap<String, Scalar>) -> Value {
    Value::Object(
        scalars
            .iter()
            .map(|(k, v)| (k.clone(), scalar_json(v)))
            .collect(),
    )
}

pub fn from_json(d: &Value) -> Result<Term, FplError> {
    let tag = d.get("kind").and_then(Value::as_str).unwrap_or("");
    let mut chars = tag.chars();
    match chars.next() {
        Some(initial) if initial.is_ascii_lowercase() => build(
            &format!("{}{}", initial.to_ascii_uppercase(), chars.as_str()),
            &mut JsonSource(d),
        ),
        _ => Err(FplError(format!("unknown term kind: {tag:?}"))),
    }
}

/// An object's fields, `null` or left out being absent.
struct JsonSource<'a>(&'a Value);

impl<'a> JsonSource<'a> {
    fn read<T>(
        &self,
        name: &'static str,
        span: bool,
        decode: impl FnOnce(&'a Value) -> Option<T>,
    ) -> Field<T> {
        let key = wire_key(name, span);
        Field::new(
            name,
            match self.0.get(&key) {
                None | Some(Value::Null) => Ok(None),
                Some(value) => decode(value)
                    .map(Some)
                    .ok_or_else(|| FplError(format!("term json {key:?} is malformed"))),
            },
        )
    }
}

impl FieldSource for JsonSource<'_> {
    fn real(&mut self, name: &'static str) -> Field<f64> {
        self.read(name, false, Value::as_f64)
    }

    fn at(&mut self, name: &'static str) -> Field<Instant> {
        self.read(name, false, Value::as_str).and_then(parse_iso)
    }

    fn span(&mut self, name: &'static str) -> Field<Delta> {
        self.read(name, true, |value| value.as_f64().map(delta_from_hours))
    }

    fn text(&mut self, name: &'static str) -> Field<String> {
        self.read(name, false, |value| value.as_str().map(str::to_owned))
    }

    fn child(&mut self, name: &'static str) -> Field<Term> {
        self.read(name, false, Some).and_then(from_json)
    }

    fn children(&mut self, name: &'static str) -> Field<Vec<Term>> {
        self.read(name, false, Value::as_array)
            .and_then(|items| items.iter().map(from_json).collect())
    }

    fn rows<T>(
        &mut self,
        name: &'static str,
        _row: &'static str,
        mut read: impl FnMut(&mut Self) -> Result<T, FplError>,
    ) -> Field<Vec<T>> {
        self.read(name, false, Value::as_array).and_then(|items| {
            items
                .iter()
                .map(|item| read(&mut JsonSource(item)))
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prodrome::fpl::{parse_term, Outcome};

    /// THE FROZEN ANSWERS. `conformance/fpl.json` held a `json` and an
    /// `explain` beside every term until 0.4; when the codec moved here its
    /// evidence moved with it, unchanged — same 150 terms, same bytes, checked
    /// the same way. The core's own vectors (`conformance/fpl.py`) keep the
    /// rest: the print, the normal form, the samples and the explanation's
    /// values.
    ///
    /// `include_str!`, inside `#[cfg(test)]`, so none of it reaches the `.wasm`.
    const VECTORS: &str = include_str!("../../conformance/term-json.json");

    /// SPEC §9.8: shapes and strings exactly, floats to 1e-9.
    fn agrees(path: &str, mine: &Value, theirs: &Value) -> Result<(), String> {
        match (mine, theirs) {
            (Value::Number(a), Value::Number(b)) => {
                let (a, b) = (
                    a.as_f64().unwrap_or(f64::NAN),
                    b.as_f64().unwrap_or(f64::NAN),
                );
                let delta = (a - b).abs();
                if delta <= 1e-9 * a.abs().max(b.abs()).max(1.0) {
                    Ok(())
                } else {
                    Err(format!("{path}: {a} != {b} (off by {delta:e})"))
                }
            }
            (Value::Array(a), Value::Array(b)) => {
                if a.len() != b.len() {
                    return Err(format!("{path}: {} items, expected {}", a.len(), b.len()));
                }
                for (i, (x, y)) in a.iter().zip(b).enumerate() {
                    agrees(&format!("{path}[{i}]"), x, y)?;
                }
                Ok(())
            }
            (Value::Object(a), Value::Object(b)) => {
                let (mine, theirs): (Vec<&String>, Vec<&String>) =
                    (a.keys().collect(), b.keys().collect());
                if mine != theirs {
                    return Err(format!("{path}: keys {mine:?} != {theirs:?}"));
                }
                for (k, x) in a {
                    agrees(&format!("{path}.{k}"), x, &b[k])?;
                }
                Ok(())
            }
            (x, y) if x == y => Ok(()),
            (x, y) => Err(format!("{path}: {x} != {y}")),
        }
    }

    fn env_of(v: &Value) -> Env {
        Env {
            outcomes: v
                .as_object()
                .expect("env is an object")
                .iter()
                .map(|(name, binding)| {
                    let at =
                        parse_iso(binding["at"].as_str().expect("an instant")).expect("an instant");
                    let outcome = match binding["kind"].as_str().expect("a kind") {
                        "Completed" => Outcome::Completed(at),
                        "Cancelled" => Outcome::Cancelled(at),
                        other => panic!("unknown env kind {other:?}"),
                    };
                    (name.clone(), [Some(outcome)].into())
                })
                .collect(),
            tended: Default::default(),
        }
    }

    /// `Absent` crosses as its bare kind, and `∅` as `null` wherever a node
    /// has no value — never as a number.
    #[test]
    fn absent_crosses_as_its_kind_and_its_value_as_null() {
        let term = parse_term("Conj(terms=(Flat(value=0.25), Absent()), p=-4.0)").expect("a term");
        let json = to_json(&term);
        assert_eq!(
            json,
            serde_json::json!({"kind": "conj", "p": -4.0, "terms": [
                {"kind": "flat", "value": 0.25}, {"kind": "absent"}
            ]})
        );
        assert_eq!(from_json(&json).expect("a term"), term);
        let at = parse_iso("2026-09-01T00:00:00").expect("an instant");
        let tree = explain(&Closed::of(term).expect("closed"), at, &Env::new());
        assert_eq!(tree["value"], 0.25);
        assert_eq!(tree["shares"], serde_json::json!([1.0, 0.0]));
        assert_eq!(
            tree["terms"][1],
            serde_json::json!({"kind": "absent", "value": null})
        );
        let alone = explain(
            &Closed::of(parse_term("Absent()").expect("a term")).expect("closed"),
            at,
            &Env::new(),
        );
        assert_eq!(alone, serde_json::json!({"kind": "absent", "value": null}));
    }

    #[test]
    fn least_crosses_as_its_kind_and_its_members() {
        let term = parse_term("Least(terms=(Flat(value=0.25), Absent()))").expect("a term");
        let json = to_json(&term);
        assert_eq!(
            json,
            serde_json::json!({"kind": "least", "terms": [
                {"kind": "flat", "value": 0.25}, {"kind": "absent"}
            ]})
        );
        assert_eq!(from_json(&json).expect("a term"), term);
        let at = parse_iso("2026-09-01T00:00:00").expect("an instant");
        let tree = explain(&Closed::of(term).expect("closed"), at, &Env::new());
        assert_eq!(tree["value"], 0.25);
        assert_eq!(tree["terms"][0]["value"], 0.25);
    }

    /// A qualified reference crosses as `refIn`, beside `ref`.
    #[test]
    fn a_qualified_ref_crosses_as_its_kind() {
        let term =
            parse_term("Conj(terms=(Ref(todo='a'), RefIn(store='todos', entity='b')), p=-4.0)")
                .expect("a term");
        let json = to_json(&term);
        assert_eq!(
            json,
            serde_json::json!({"kind": "conj", "p": -4.0, "terms": [
                {"kind": "ref", "todo": "a"},
                {"kind": "refIn", "store": "todos", "entity": "b"}
            ]})
        );
        assert_eq!(from_json(&json).expect("a term"), term);
    }

    #[test]
    fn every_term_has_the_json_and_explain_shape_the_vectors_froze() {
        let doc: Value = serde_json::from_str(VECTORS).expect("the fixture is JSON");
        let terms = doc["terms"].as_array().expect("a terms array");
        assert_eq!(terms.len(), 150, "the fixture lost cases");
        for (seed, case) in terms.iter().enumerate() {
            let print = case["term"].as_str().expect("a print");
            // The core parses the term; only the JSON is this module's.
            let term = parse_term(print).unwrap_or_else(|e| panic!("seed {seed}: {e}"));

            agrees("json", &to_json(&term), &case["json"])
                .unwrap_or_else(|e| panic!("seed {seed}: to_json {e}"));

            // `from_json` is the other door into the same term, and the print
            // is what says it arrived at the same one.
            let back = from_json(&case["json"]).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            assert_eq!(
                prodrome::fpl::print_term(&back),
                print,
                "seed {seed}: from_json ∘ to_json"
            );

            let at = parse_iso(case["explain_at"].as_str().expect("an instant")).expect("valid");
            let env = env_of(&case["env"]);
            let closed = Closed::of(term).expect("the frozen terms hold no ref");
            agrees("explain", &explain(&closed, at, &env), &case["explain"])
                .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        }
    }
}
