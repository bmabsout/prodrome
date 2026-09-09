//! §7 AS JSON — the term codec, at the boundary JSON is for.
//!
//! JSON is JavaScript's literal grammar, and this crate is where JavaScript
//! is. The core has ONE serialization for a term and it is §2's literal print;
//! it kept a second one — `fpl::to_json`, `from_json`, `explanation_json` —
//! for a browser to read, which meant a crate whose whole claim is "one
//! grammar, one printer, one parser" shipped two of each. Since 0.4 they live
//! here, built on the core's public `TermF`/`Explanation` and its smart
//! constructors, and the core has no JSON in it at all.
//!
//! THE WIRE IS UNCHANGED. Every key, every tag, every unit is what
//! `fpl::to_json` emitted: lowercase kind tags (`offsetBy` the one camel one),
//! ISO instants, and spans in HOURS (`leadUpHours`, `windowHours`,
//! `deltaHours`) because that is what a page does arithmetic in.
//! `conformance/term-json.json` — the `json` and `explain` halves of what was
//! `conformance/fpl.json` in the core — is frozen against exactly these
//! answers, and `tests` below replays all 150.
//!
//! PARSE, DON'T VALIDATE: every arm of [`from_json`] ends in one of the core's
//! `mk_*` constructors, so a term that comes off the wire is a term the core
//! would have built, or a refusal.

use prodrome::fpl::{
    delta_from_hours, explained, iso, mk_after, mk_conj, mk_curve, mk_decay, mk_flat, mk_gate,
    mk_importance, mk_offset, mk_offset_by, mk_piecewise, mk_shift, mk_within, parse_iso,
    total_seconds, CurvePoint, Env, Explanation, FplError, Instant, Note, Scalar, Term, TermF,
    PRIORITY_POWER,
};
use serde_json::{Map, Value};

/// The core's own refusal, spelled here because `fpl::err` is private to it —
/// this module is a second reader of §7, not a part of it.
fn err<T>(msg: impl Into<String>) -> Result<T, FplError> {
    Err(FplError(msg.into()))
}

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
        Note::Maps(ms) => Value::Array(
            ms.iter()
                .map(|m| {
                    Value::Object(m.iter().map(|(k, v)| (k.clone(), scalar_json(v))).collect())
                })
                .collect(),
        ),
    }
}

/// The wire: `to_json`'s shape with a `value` on every node, the notes beside
/// it, and each part explained in place of its printed subterm.
pub fn explanation_json(node: &Explanation) -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), Value::String(node.node.kind().to_string()));
    out.insert("value".into(), Value::from(node.value));
    for (k, n) in &node.notes {
        out.insert(k.clone(), note_json(n));
    }
    match &*node.node {
        TermF::Flat { .. } | TermF::Decay { .. } | TermF::Curve { .. } => {}
        TermF::Conj { terms, .. } => {
            out.insert(
                "terms".into(),
                Value::Array(terms.iter().map(explanation_json).collect()),
            );
        }
        TermF::Offset { term, .. }
        | TermF::Importance { term, .. }
        | TermF::Shift { term, .. }
        | TermF::Within { term, .. } => {
            out.insert("term".into(), explanation_json(term));
        }
        TermF::Gate { gate, body } => {
            out.insert("gate".into(), explanation_json(gate));
            out.insert("body".into(), explanation_json(body));
        }
        TermF::OffsetBy { delta, term } => {
            out.insert("delta".into(), explanation_json(delta));
            out.insert("term".into(), explanation_json(term));
        }
        TermF::After { term, pending, .. } => {
            out.insert("term".into(), explanation_json(term));
            out.insert("pending".into(), explanation_json(pending));
        }
        // The piece in force rides in the `head` slot; it prints as "term".
        TermF::Piecewise { head, .. } => {
            out.insert("term".into(), explanation_json(head));
        }
    }
    Value::Object(out)
}

/// `explained`, printed — the name every consumer already reads.
pub fn explain(term: &Term, now: Instant, env: &Env) -> Value {
    explanation_json(&explained(term, now, env))
}

pub fn to_json(term: &Term) -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), Value::String(term.out().kind().to_string()));
    match term.out() {
        TermF::Flat { value } => {
            out.insert("value".into(), Value::from(*value));
        }
        TermF::Decay {
            start,
            end,
            end_date,
            lead_up,
            start_date,
        } => {
            out.insert("start".into(), Value::from(*start));
            out.insert("end".into(), Value::from(*end));
            out.insert("endDate".into(), Value::String(iso(*end_date)));
            out.insert(
                "leadUpHours".into(),
                Value::from(total_seconds(*lead_up) / 3600.0),
            );
            if let Some(sd) = start_date {
                out.insert("startDate".into(), Value::String(iso(*sd)));
            }
        }
        TermF::Curve { points } => {
            out.insert(
                "points".into(),
                Value::Array(
                    points
                        .iter()
                        .map(|pt| {
                            let mut m = Map::new();
                            m.insert("at".into(), Value::String(iso(pt.at)));
                            m.insert("value".into(), Value::from(pt.value));
                            if !pt.label.is_empty() {
                                m.insert("label".into(), Value::String(pt.label.clone()));
                            }
                            Value::Object(m)
                        })
                        .collect(),
                ),
            );
        }
        TermF::Conj { terms, p } => {
            out.insert("p".into(), Value::from(*p));
            out.insert(
                "terms".into(),
                Value::Array(terms.iter().map(to_json).collect()),
            );
        }
        TermF::Offset { delta, term } => {
            out.insert("delta".into(), Value::from(*delta));
            out.insert("term".into(), to_json(term));
        }
        TermF::Gate { gate, body } => {
            out.insert("gate".into(), to_json(gate));
            out.insert("body".into(), to_json(body));
        }
        TermF::Shift { delta, term } => {
            out.insert(
                "deltaHours".into(),
                Value::from(total_seconds(*delta) / 3600.0),
            );
            out.insert("term".into(), to_json(term));
        }
        TermF::Within { window, p, term } => {
            out.insert(
                "windowHours".into(),
                Value::from(total_seconds(*window) / 3600.0),
            );
            out.insert("p".into(), Value::from(*p));
            out.insert("term".into(), to_json(term));
        }
        TermF::Importance { w, term } => {
            out.insert("w".into(), Value::from(*w));
            out.insert("term".into(), to_json(term));
        }
        TermF::After {
            event,
            anchor,
            term,
            pending,
            needs,
        } => {
            out.insert("event".into(), Value::String(event.clone()));
            out.insert("anchor".into(), Value::String(iso(*anchor)));
            out.insert("term".into(), to_json(term));
            out.insert("pending".into(), to_json(pending));
            if let Some(n) = needs {
                out.insert("needsHours".into(), Value::from(total_seconds(*n) / 3600.0));
            }
        }
        TermF::OffsetBy { delta, term } => {
            out.insert("delta".into(), to_json(delta));
            out.insert("term".into(), to_json(term));
        }
        TermF::Piecewise { head, pieces } => {
            out.insert("head".into(), to_json(head));
            out.insert(
                "pieces".into(),
                Value::Array(
                    pieces
                        .iter()
                        .map(|(at, t)| {
                            let mut m = Map::new();
                            m.insert("at".into(), Value::String(iso(*at)));
                            m.insert("term".into(), to_json(t));
                            Value::Object(m)
                        })
                        .collect(),
                ),
            );
        }
    }
    Value::Object(out)
}

fn field<'a>(d: &'a Value, key: &str) -> Result<&'a Value, FplError> {
    d.get(key)
        .ok_or_else(|| FplError(format!("term json missing {key:?}")))
}

fn num(d: &Value, key: &str) -> Result<f64, FplError> {
    field(d, key)?
        .as_f64()
        .ok_or_else(|| FplError(format!("term json {key:?} is not a number")))
}

fn text(d: &Value, key: &str) -> Result<String, FplError> {
    Ok(field(d, key)?
        .as_str()
        .ok_or_else(|| FplError(format!("term json {key:?} is not a string")))?
        .to_string())
}

fn list<'a>(d: &'a Value, key: &str) -> Result<&'a Vec<Value>, FplError> {
    field(d, key)?
        .as_array()
        .ok_or_else(|| FplError(format!("term json {key:?} is not a list")))
}

pub fn from_json(d: &Value) -> Result<Term, FplError> {
    match d.get("kind").and_then(Value::as_str).unwrap_or("") {
        "flat" => mk_flat(num(d, "value")?),
        "decay" => mk_decay(
            num(d, "start")?,
            num(d, "end")?,
            parse_iso(&text(d, "endDate")?)?,
            delta_from_hours(num(d, "leadUpHours")?),
            match d.get("startDate") {
                Some(_) => Some(parse_iso(&text(d, "startDate")?)?),
                None => None,
            },
        ),
        "curve" => {
            let mut points = vec![];
            for pt in list(d, "points")? {
                points.push(CurvePoint {
                    at: parse_iso(&text(pt, "at")?)?,
                    value: num(pt, "value")?,
                    label: pt
                        .get("label")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                });
            }
            mk_curve(points)
        }
        "conj" => {
            let mut terms = vec![];
            for t in list(d, "terms")? {
                terms.push(from_json(t)?);
            }
            mk_conj(
                terms,
                d.get("p").and_then(Value::as_f64).unwrap_or(PRIORITY_POWER),
            )
        }
        "offset" => mk_offset(num(d, "delta")?, from_json(field(d, "term")?)?),
        "gate" => mk_gate(from_json(field(d, "gate")?)?, from_json(field(d, "body")?)?),
        "shift" => mk_shift(
            delta_from_hours(num(d, "deltaHours")?),
            from_json(field(d, "term")?)?,
        ),
        "within" => mk_within(
            delta_from_hours(num(d, "windowHours")?),
            num(d, "p")?,
            from_json(field(d, "term")?)?,
        ),
        "importance" => mk_importance(num(d, "w")?, from_json(field(d, "term")?)?),
        "after" => mk_after(
            text(d, "event")?,
            parse_iso(&text(d, "anchor")?)?,
            from_json(field(d, "term")?)?,
            from_json(field(d, "pending")?)?,
            match d.get("needsHours") {
                Some(_) => Some(delta_from_hours(num(d, "needsHours")?)),
                None => None,
            },
        ),
        "offsetBy" => mk_offset_by(
            from_json(field(d, "delta")?)?,
            from_json(field(d, "term")?)?,
        ),
        "piecewise" => {
            let mut pieces = vec![];
            for p in list(d, "pieces")? {
                pieces.push((parse_iso(&text(p, "at")?)?, from_json(field(p, "term")?)?));
            }
            mk_piecewise(from_json(field(d, "head")?)?, pieces)
        }
        other => err(format!("unknown term kind: {other:?}")),
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
    const VECTORS: &str = include_str!("../conformance/term-json.json");

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
        v.as_object()
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
                (name.clone(), outcome)
            })
            .collect()
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
            agrees("explain", &explain(&term, at, &env), &case["explain"])
                .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        }
    }
}
