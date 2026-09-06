//! §7 — FPL: the term language, one evaluator.
//!
//! See ../../SPEC.md. Implemented against ../../conformance/fpl.json, and
//! ported from the reference `suzatary/fpl.py` — the arithmetic ORDER is part
//! of the port, because the vectors are the reference's floats.
//!
//! Types first (§1, §8). `TermF<A>` is ONE LAYER of the term functor with its
//! child positions as `A`; `Term` is its fixed point and `Explanation` is the
//! same shape carrying an annotation — the Cofree the spec calls "decoration,
//! not evaluation". Invariants live in the `mk_*` smart constructors; a `Term`
//! that exists came through one of them (or through `from_json`/`parse_term`,
//! which call them).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use serde_json::{Map, Value};
use thiserror::Error;

/// Naive local time only (§2): a stored instant never carries a zone.
pub type Instant = NaiveDateTime;
/// A signed span, printed as §2's `timedelta(...)`.
pub type Delta = Duration;

/// The production conjunction exponent — a graded ∀, not a connective.
pub const PRIORITY_POWER: f64 = -4.0;
/// `Within` is 64 intervals, 65 evenly spaced points (§7, and the stated limit).
pub const WITHIN_SAMPLES: i64 = 64;

/// A refusal from a smart constructor or the literal parser. §2: malformed
/// input is a `ValueError`-shaped refusal, never a crash.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct FplError(pub String);

fn err<T>(msg: impl Into<String>) -> Result<T, FplError> {
    Err(FplError(msg.into()))
}

// --- §7 the functor, its fixed point, and the environment -------------------

/// A named landmark on a `Curve`. It carries no invariant of its own — the
/// enclosing `mk_curve` validates it — which is why it is admitted as a record.
#[derive(Debug, Clone, PartialEq)]
pub struct CurvePoint {
    pub at: Instant,
    pub value: f64,
    pub label: String,
}

/// One layer of the term functor: every child position is an `A`.
#[derive(Debug, Clone, PartialEq)]
pub enum TermF<A> {
    Flat {
        value: f64,
    },
    Decay {
        start: f64,
        end: f64,
        end_date: Instant,
        lead_up: Delta,
        start_date: Option<Instant>,
    },
    Curve {
        points: Vec<CurvePoint>,
    },
    Conj {
        terms: Vec<A>,
        p: f64,
    },
    Offset {
        delta: f64,
        term: A,
    },
    Gate {
        gate: A,
        body: A,
    },
    Shift {
        delta: Delta,
        term: A,
    },
    Within {
        window: Delta,
        p: f64,
        term: A,
    },
    Importance {
        w: f64,
        term: A,
    },
    After {
        event: String,
        anchor: Instant,
        term: A,
        pending: A,
        needs: Option<Delta>,
    },
    Piecewise {
        head: A,
        pieces: Vec<(Instant, A)>,
    },
    OffsetBy {
        delta: A,
        term: A,
    },
}

impl<A> TermF<A> {
    /// The functor's action on child positions. `map` plus the fixed point is
    /// the whole of the "types first" claim: every recursion below is either a
    /// fold over this or a decoration of it.
    pub fn map<B>(self, mut f: impl FnMut(A) -> B) -> TermF<B> {
        match self {
            TermF::Flat { value } => TermF::Flat { value },
            TermF::Decay {
                start,
                end,
                end_date,
                lead_up,
                start_date,
            } => TermF::Decay {
                start,
                end,
                end_date,
                lead_up,
                start_date,
            },
            TermF::Curve { points } => TermF::Curve { points },
            TermF::Conj { terms, p } => TermF::Conj {
                terms: terms.into_iter().map(f).collect(),
                p,
            },
            TermF::Offset { delta, term } => TermF::Offset {
                delta,
                term: f(term),
            },
            TermF::Gate { gate, body } => TermF::Gate {
                gate: f(gate),
                body: f(body),
            },
            TermF::Shift { delta, term } => TermF::Shift {
                delta,
                term: f(term),
            },
            TermF::Within { window, p, term } => TermF::Within {
                window,
                p,
                term: f(term),
            },
            TermF::Importance { w, term } => TermF::Importance { w, term: f(term) },
            TermF::After {
                event,
                anchor,
                term,
                pending,
                needs,
            } => TermF::After {
                event,
                anchor,
                term: f(term),
                pending: f(pending),
                needs,
            },
            TermF::Piecewise { head, pieces } => TermF::Piecewise {
                head: f(head),
                pieces: pieces.into_iter().map(|(at, t)| (at, f(t))).collect(),
            },
            TermF::OffsetBy { delta, term } => TermF::OffsetBy {
                delta: f(delta),
                term: f(term),
            },
        }
    }

    /// The child positions, in declaration order.
    pub fn children(&self) -> Vec<&A> {
        match self {
            TermF::Flat { .. } | TermF::Decay { .. } | TermF::Curve { .. } => vec![],
            TermF::Conj { terms, .. } => terms.iter().collect(),
            TermF::Offset { term, .. } | TermF::Shift { term, .. } => vec![term],
            TermF::Within { term, .. } | TermF::Importance { term, .. } => vec![term],
            TermF::Gate { gate, body } => vec![gate, body],
            TermF::After { term, pending, .. } => vec![term, pending],
            TermF::Piecewise { head, pieces } => {
                let mut out = vec![head];
                out.extend(pieces.iter().map(|(_, t)| t));
                out
            }
            TermF::OffsetBy { delta, term } => vec![delta, term],
        }
    }

    /// The `to_json` kind tag.
    pub fn kind(&self) -> &'static str {
        match self {
            TermF::Flat { .. } => "flat",
            TermF::Decay { .. } => "decay",
            TermF::Curve { .. } => "curve",
            TermF::Conj { .. } => "conj",
            TermF::Offset { .. } => "offset",
            TermF::Gate { .. } => "gate",
            TermF::Shift { .. } => "shift",
            TermF::Within { .. } => "within",
            TermF::Importance { .. } => "importance",
            TermF::After { .. } => "after",
            TermF::Piecewise { .. } => "piecewise",
            TermF::OffsetBy { .. } => "offsetBy",
        }
    }
}

/// The fixed point: `Term ≅ TermF Term`. A deep embedding — never a closure —
/// so a term is storable, shippable and evaluated at query time.
#[derive(Debug, Clone, PartialEq)]
pub struct Term(Box<TermF<Term>>);

impl Term {
    /// Wrap one layer. Prefer the `mk_*` constructors: they are the only
    /// sanctioned path from untrusted parameters to a trusted `Term`.
    pub fn new(layer: TermF<Term>) -> Self {
        Term(Box::new(layer))
    }
    /// Unwrap one layer.
    pub fn out(&self) -> &TermF<Term> {
        &self.0
    }
    /// Unwrap one layer, by value.
    pub fn into_out(self) -> TermF<Term> {
        *self.0
    }
}

/// What history says about a named event. The two ways an event can be over
/// mean OPPOSITE things downstream, so they are an ADT and never a boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Completed(Instant),
    Cancelled(Instant),
}

impl Outcome {
    pub fn at(&self) -> Instant {
        match self {
            Outcome::Completed(at) | Outcome::Cancelled(at) => *at,
        }
    }
}

/// `After`'s freeze variables: what history binds, as a snapshot.
pub type Env = BTreeMap<String, Outcome>;

// --- Time arithmetic, in the reference's units ------------------------------

fn us_of(d: Delta) -> i64 {
    d.num_microseconds()
        .expect("timedelta out of microsecond range")
}

fn after(now: Instant, us: i64) -> Instant {
    now.checked_add_signed(Duration::microseconds(us))
        .expect("datetime overflow")
}

/// Python's `_divide_and_round`: floor division with a round-half-to-even fix-up.
fn divide_and_round(a: i64, b: i64) -> i64 {
    // Python's divmod floors toward −∞ for either sign of `b`.
    let mut q = a.div_euclid(b);
    let mut r = a.rem_euclid(b);
    if b < 0 && r != 0 {
        q += 1;
        r += b;
    }
    let r2 = r.checked_mul(2).expect("divide_and_round overflow");
    let greater_than_half = if b > 0 { r2 > b } else { r2 < b };
    if greater_than_half || (r2 == b && q.rem_euclid(2) == 1) {
        q += 1;
    }
    q
}

/// `timedelta / int` — exact microseconds, rounded half to even, as CPython.
pub fn div_delta(d: Delta, n: i64) -> Delta {
    Duration::microseconds(divide_and_round(us_of(d), n))
}

/// `timedelta / timedelta` — the float ratio CPython computes from microseconds.
fn ratio(a: Delta, b: Delta) -> f64 {
    us_of(a) as f64 / us_of(b) as f64
}

/// `timedelta.total_seconds()`: exact microseconds over 1e6.
pub fn total_seconds(d: Delta) -> f64 {
    us_of(d) as f64 / 1e6
}

/// `timedelta(hours=…)` for a float, with CPython's round-half-even microsecond.
pub fn delta_from_hours(hours: f64) -> Delta {
    let seconds = hours * 3600.0;
    let whole = seconds.trunc();
    let frac = seconds - whole;
    Duration::microseconds((whole as i64) * 1_000_000 + round_half_even(frac * 1e6))
}

fn round_half_even(x: f64) -> i64 {
    let away = x.round(); // half away from zero
    if (x - x.trunc()).abs() == 0.5 && away % 2.0 != 0.0 {
        (away - x.signum()) as i64
    } else {
        away as i64
    }
}

// --- §7 semantics -----------------------------------------------------------

/// utility.typ's `powerMean`, ported: a 0.001 floor CLAMP (not the slack
/// form), the geometric branch at `p == 0` taken in log space, and an empty
/// collection reading 0.5. The summation order is the reference's.
pub fn power_mean(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.5;
    }
    let vs: Vec<f64> = values.iter().map(|v| v.max(0.001)).collect();
    let n = vs.len() as f64;
    if p == 0.0 {
        let mut s = 0.0;
        for v in &vs {
            s += v.ln();
        }
        return (s / n).exp();
    }
    let mut s = 0.0;
    for v in &vs {
        s += v.powf(p);
    }
    (s / n).powf(1.0 / p)
}

/// `[φ]_δ`, the CORRECTED form: `x·(1−|δ|) + max(0, δ)`.
pub fn offset(x: f64, delta: f64) -> f64 {
    x * (1.0 - delta.abs()) + 0.0f64.max(delta)
}

fn eval_decay(
    start: f64,
    end: f64,
    end_date: Instant,
    lead_up: Delta,
    start_date: Option<Instant>,
    now: Instant,
) -> f64 {
    if let Some(sd) = start_date {
        if sd > now {
            return 1.0;
        }
    }
    let window_start = end_date - lead_up;
    if window_start > now {
        return 0.98;
    }
    if now > end_date {
        return end;
    }
    let frac = ratio(now - window_start, lead_up);
    (start - end) * (1.0 - frac) + end
}

fn eval_curve(points: &[CurvePoint], now: Instant) -> f64 {
    let first = &points[0];
    if now <= first.at {
        return first.value;
    }
    for pair in points.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if now <= b.at {
            let frac = ratio(now - a.at, b.at - a.at);
            return a.value + (b.value - a.value) * frac;
        }
    }
    points[points.len() - 1].value
}

/// The function governing `now` and since when: the last piece whose instant
/// is not after `now`, or the head, which has no instant.
pub fn in_force<'a>(
    head: &'a Term,
    pieces: &'a [(Instant, Term)],
    now: Instant,
) -> (Option<Instant>, &'a Term) {
    let mut since = None;
    let mut term = head;
    for (at, t) in pieces {
        if *at <= now {
            since = Some(*at);
            term = t;
        } else {
            break;
        }
    }
    (since, term)
}

/// What history says about `event` AS OF `now`. A snapshot taken later may
/// hold a binding dated after `now`; that binding is not in force yet, and
/// reading it would let a later completion rewrite an earlier moment.
pub fn bound(env: &Env, event: &str, now: Instant) -> Option<Outcome> {
    match env.get(event) {
        Some(o) if o.at() <= now => Some(*o),
        _ => None,
    }
}

/// Evaluate a term at a moment against what history says. Total by structure;
/// every branch returns a value in [0, 1].
pub fn fulfillment(term: &Term, now: Instant, env: &Env) -> f64 {
    match term.out() {
        TermF::Flat { value } => *value,
        TermF::Decay {
            start,
            end,
            end_date,
            lead_up,
            start_date,
        } => eval_decay(*start, *end, *end_date, *lead_up, *start_date, now),
        TermF::Curve { points } => eval_curve(points, now),
        TermF::Conj { terms, p } => {
            let vs: Vec<f64> = terms.iter().map(|t| fulfillment(t, now, env)).collect();
            power_mean(&vs, *p)
        }
        TermF::Offset { delta, term } => offset(fulfillment(term, now, env), *delta),
        TermF::Gate { gate, body } => {
            (1.0 - fulfillment(gate, now, env)).max(fulfillment(body, now, env))
        }
        TermF::Shift { delta, term } => fulfillment(term, after(now, us_of(*delta)), env),
        TermF::Within { window, p, term } => {
            let step = us_of(div_delta(*window, WITHIN_SAMPLES));
            let vs: Vec<f64> = (0..=WITHIN_SAMPLES)
                .map(|i| fulfillment(term, after(now, step * i), env))
                .collect();
            power_mean(&vs, *p)
        }
        TermF::Importance { w, term } => fulfillment(term, now, env).powf(*w),
        TermF::After {
            event,
            anchor,
            term,
            pending,
            ..
        } => match bound(env, event, now) {
            None => fulfillment(pending, now, env),
            // The body slides by the slippage: authored against `anchor`,
            // re-anchored to the actual completion.
            Some(Outcome::Completed(done)) => {
                fulfillment(term, after(now, -us_of(done - *anchor)), env)
            }
            // Moot AS PRICING — reporting must surface it (§7, After).
            Some(Outcome::Cancelled(_)) => 1.0,
        },
        TermF::Piecewise { head, pieces } => fulfillment(in_force(head, pieces, now).1, now, env),
        TermF::OffsetBy { delta, term } => {
            offset(fulfillment(term, now, env), fulfillment(delta, now, env))
        }
    }
}

// --- The explanation: Cofree TermF Annotation -------------------------------

/// The Minimum Fulfillment Bound (Theorem IV.1): the floor a composed p-mean
/// score PROVES about its worst DIRECT member. Applied per node — the flat
/// bound is unsound across the levels of a nested composition. The reference
/// refuses `n < 1`; here the formula is simply total (an empty `Conj` reads 1.0).
pub fn min_fulfillment(composed: f64, n: usize, p: f64) -> f64 {
    let nf = n as f64;
    if p == 0.0 {
        return composed.powf(nf);
    }
    let inner = nf * composed.powf(p) - (nf - 1.0);
    if inner > 0.0 {
        inner.powf(1.0 / p)
    } else {
        0.0
    }
}

/// Each member's SHARE of responsibility for the p-mean it composes into:
/// `eᵢ = (1/n)·(xᵢ/M)ᵖ`, summing to exactly 1. Applies power_mean's clamp to
/// the same values, or the shares would not describe the number returned.
pub fn member_shares(values: &[f64], p: f64) -> Vec<f64> {
    if values.is_empty() {
        return vec![];
    }
    let n = values.len();
    if p == 0.0 {
        return vec![1.0 / n as f64; n];
    }
    let m = power_mean(values, p);
    values
        .iter()
        .map(|v| (1.0 / n as f64) * (v.max(0.001) / m).powf(p))
        .collect()
}

/// A note's leaf: the scalars an explanation carries beside its value.
#[derive(Debug, Clone, PartialEq)]
pub enum Scalar {
    Text(String),
    Float(f64),
    Int(i64),
    Bool(bool),
}

/// `Scalar | tuple[Scalar, …] | tuple[Mapping[str, Scalar], …]` — the
/// reference's `Note`, as a closed type.
#[derive(Debug, Clone, PartialEq)]
pub enum Note {
    One(Scalar),
    Many(Vec<Scalar>),
    Maps(Vec<BTreeMap<String, Scalar>>),
}

/// `Cofree TermF Annotation`: the term's OWN shape, each node carrying its
/// fulfillment at the moment the parent actually used it, plus the notes.
///
/// One documented bend in the shape: a `Piecewise` node's decoration is the
/// PIECE IN FORCE, which rides in the `head` slot with `pieces` empty — head
/// and pieces are transitions, not subterms, and the schedule's size and start
/// are notes. Every other constructor's children line up one for one.
///
/// The layer is boxed for the same reason `Term`'s is: `TermF` holds its
/// children by value, so the fixed point needs exactly one indirection.
#[derive(Debug, Clone, PartialEq)]
pub struct Explanation {
    pub value: f64,
    pub notes: BTreeMap<String, Note>,
    pub node: Box<TermF<Explanation>>,
}

fn iso_note(t: Instant) -> Note {
    Note::One(Scalar::Text(iso(t)))
}

/// The term's own fields as `to_json` prints them, minus the subterms.
fn scalar_notes(term: &Term) -> BTreeMap<String, Note> {
    let mut n = BTreeMap::new();
    match term.out() {
        TermF::Flat { value } => {
            n.insert("value".into(), Note::One(Scalar::Float(*value)));
        }
        TermF::Decay {
            start,
            end,
            end_date,
            lead_up,
            start_date,
        } => {
            n.insert("start".into(), Note::One(Scalar::Float(*start)));
            n.insert("end".into(), Note::One(Scalar::Float(*end)));
            n.insert("endDate".into(), iso_note(*end_date));
            n.insert(
                "leadUpHours".into(),
                Note::One(Scalar::Float(total_seconds(*lead_up) / 3600.0)),
            );
            if let Some(sd) = start_date {
                n.insert("startDate".into(), iso_note(*sd));
            }
        }
        TermF::Curve { points } => {
            n.insert(
                "points".into(),
                Note::Maps(
                    points
                        .iter()
                        .map(|pt| {
                            let mut m = BTreeMap::new();
                            m.insert("at".to_string(), Scalar::Text(iso(pt.at)));
                            m.insert("value".to_string(), Scalar::Float(pt.value));
                            if !pt.label.is_empty() {
                                m.insert("label".to_string(), Scalar::Text(pt.label.clone()));
                            }
                            m
                        })
                        .collect(),
                ),
            );
        }
        TermF::Conj { p, .. } => {
            n.insert("p".into(), Note::One(Scalar::Float(*p)));
        }
        TermF::Within { window, p, .. } => {
            n.insert(
                "windowHours".into(),
                Note::One(Scalar::Float(total_seconds(*window) / 3600.0)),
            );
            n.insert("p".into(), Note::One(Scalar::Float(*p)));
        }
        TermF::Offset { delta, .. } => {
            n.insert("delta".into(), Note::One(Scalar::Float(*delta)));
        }
        TermF::Shift { delta, .. } => {
            n.insert(
                "deltaHours".into(),
                Note::One(Scalar::Float(total_seconds(*delta) / 3600.0)),
            );
        }
        TermF::Importance { w, .. } => {
            n.insert("w".into(), Note::One(Scalar::Float(*w)));
        }
        TermF::After {
            event,
            anchor,
            needs,
            ..
        } => {
            n.insert("event".into(), Note::One(Scalar::Text(event.clone())));
            n.insert("anchor".into(), iso_note(*anchor));
            if let Some(nd) = needs {
                n.insert(
                    "needsHours".into(),
                    Note::One(Scalar::Float(total_seconds(*nd) / 3600.0)),
                );
            }
        }
        // Gate, OffsetBy: every field is a subterm. Piecewise: its head and
        // pieces are transitions, and `explained` writes `pieces`/`since`.
        TermF::Gate { .. } | TermF::OffsetBy { .. } | TermF::Piecewise { .. } => {}
    }
    n
}

/// A `to_json`-shaped tree where every node also carries its own fulfillment.
///
/// ⚠️ CHILDREN ARE ANNOTATED AT THE TIME THE PARENT ACTUALLY USED: Shift,
/// Within and After re-anchor time for their subterm, and a child evaluated at
/// `now` would report a number the parent never consumed.
pub fn explained(term: &Term, now: Instant, env: &Env) -> Explanation {
    let value = fulfillment(term, now, env);
    let mut notes = scalar_notes(term);
    let node: TermF<Explanation> = match term.out() {
        TermF::Flat { value } => TermF::Flat { value: *value },
        TermF::Decay {
            start,
            end,
            end_date,
            lead_up,
            start_date,
        } => TermF::Decay {
            start: *start,
            end: *end,
            end_date: *end_date,
            lead_up: *lead_up,
            start_date: *start_date,
        },
        TermF::Curve { points } => TermF::Curve {
            points: points.clone(),
        },
        TermF::Conj { terms, p } => {
            let kids: Vec<Explanation> = terms.iter().map(|t| explained(t, now, env)).collect();
            notes.insert(
                "certifies".into(),
                Note::One(Scalar::Float(min_fulfillment(value, terms.len(), *p))),
            );
            let vs: Vec<f64> = kids.iter().map(|k| k.value).collect();
            notes.insert(
                "shares".into(),
                Note::Many(
                    member_shares(&vs, *p)
                        .into_iter()
                        .map(Scalar::Float)
                        .collect(),
                ),
            );
            TermF::Conj { terms: kids, p: *p }
        }
        TermF::Offset { delta, term } => TermF::Offset {
            delta: *delta,
            term: explained(term, now, env),
        },
        TermF::Importance { w, term } => TermF::Importance {
            w: *w,
            term: explained(term, now, env),
        },
        TermF::Gate { gate, body } => TermF::Gate {
            gate: explained(gate, now, env),
            body: explained(body, now, env),
        },
        TermF::OffsetBy { delta, term } => TermF::OffsetBy {
            delta: explained(delta, now, env),
            term: explained(term, now, env),
        },
        TermF::Shift { delta, term } => TermF::Shift {
            delta: *delta,
            term: explained(term, after(now, us_of(*delta)), env),
        },
        TermF::Within { window, p, term } => {
            let step = us_of(div_delta(*window, WITHIN_SAMPLES));
            let times: Vec<Instant> = (0..=WITHIN_SAMPLES).map(|i| after(now, step * i)).collect();
            let vs: Vec<f64> = times.iter().map(|t| fulfillment(term, *t, env)).collect();
            let shares = member_shares(&vs, *p);
            // `max(range(n), key=…)` keeps the FIRST maximal index.
            let mut peak = 0usize;
            for (i, s) in shares.iter().enumerate() {
                if *s > shares[peak] {
                    peak = i;
                }
            }
            notes.insert("peakAt".into(), iso_note(times[peak]));
            notes.insert("peakShare".into(), Note::One(Scalar::Float(shares[peak])));
            TermF::Within {
                window: *window,
                p: *p,
                term: explained(term, times[peak], env),
            }
        }
        TermF::After {
            event,
            anchor,
            term,
            pending,
            needs,
        } => {
            let (label, at) = match bound(env, event, now) {
                None => ("pending", now),
                // The slippage the parent applied.
                Some(Outcome::Completed(done)) => ("completed", after(now, -us_of(done - *anchor))),
                Some(Outcome::Cancelled(_)) => ("cancelled", now),
            };
            notes.insert("bound".into(), Note::One(Scalar::Text(label.to_string())));
            TermF::After {
                event: event.clone(),
                anchor: *anchor,
                term: explained(term, at, env),
                pending: explained(pending, now, env),
                needs: *needs,
            }
        }
        TermF::Piecewise { head, pieces } => {
            let (since, in_f) = in_force(head, pieces, now);
            notes.insert("pieces".into(), Note::One(Scalar::Int(pieces.len() as i64)));
            notes.insert(
                "since".into(),
                Note::One(Scalar::Text(since.map(iso).unwrap_or_default())),
            );
            TermF::Piecewise {
                head: explained(in_f, now, env),
                pieces: vec![],
            }
        }
    };
    Explanation {
        value,
        notes,
        node: Box::new(node),
    }
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

// --- Smart constructors (§1: records are dumb data; these are the only path) -

fn unit(name: &str, v: f64) -> Result<f64, FplError> {
    if !(0.0..=1.0).contains(&v) {
        return err(format!("{name} must be in [0, 1], got {v}"));
    }
    Ok(v)
}

pub fn mk_flat(value: f64) -> Result<Term, FplError> {
    Ok(Term::new(TermF::Flat {
        value: unit("Flat.value", value)?,
    }))
}

pub fn mk_decay(
    start: f64,
    end: f64,
    end_date: Instant,
    lead_up: Delta,
    start_date: Option<Instant>,
) -> Result<Term, FplError> {
    if lead_up <= Duration::zero() {
        return err("Decay.lead_up must be positive");
    }
    if start > 0.98 {
        // The pre-window value IS 0.98; a higher start would make urgency DROP
        // on window entry, breaking slippage-monotonicity.
        return err(format!(
            "Decay.start must be ≤ 0.98 (pre-window value), got {start}"
        ));
    }
    Ok(Term::new(TermF::Decay {
        start: unit("Decay.start", start)?,
        end: unit("Decay.end", end)?,
        end_date,
        lead_up,
        start_date,
    }))
}

pub fn mk_curve(points: Vec<CurvePoint>) -> Result<Term, FplError> {
    if points.is_empty() {
        return err("Curve needs at least one point");
    }
    for pt in &points {
        let name = if pt.label.is_empty() {
            iso(pt.at)
        } else {
            pt.label.clone()
        };
        unit(&format!("Curve point {name}"), pt.value)?;
    }
    for pair in points.windows(2) {
        if pair[1].at <= pair[0].at {
            return err("Curve points must be strictly increasing in time");
        }
    }
    Ok(Term::new(TermF::Curve { points }))
}

pub fn mk_conj(terms: Vec<Term>, p: f64) -> Result<Term, FplError> {
    // ≤ 0: FPL conjunctions only. ≥ −32: overflow guard under the 0.001 clamp.
    if !(-32.0..=0.0).contains(&p) {
        return err(format!("Conj.p must be in [-32, 0], got {p}"));
    }
    Ok(Term::new(TermF::Conj { terms, p }))
}

pub fn mk_offset(delta: f64, term: Term) -> Result<Term, FplError> {
    if !(-1.0..=1.0).contains(&delta) {
        return err(format!("Offset.delta must be in [-1, 1], got {delta}"));
    }
    Ok(Term::new(TermF::Offset { delta, term }))
}

pub fn mk_gate(gate: Term, body: Term) -> Result<Term, FplError> {
    Ok(Term::new(TermF::Gate { gate, body }))
}

pub fn mk_shift(delta: Delta, term: Term) -> Result<Term, FplError> {
    Ok(Term::new(TermF::Shift { delta, term }))
}

pub fn mk_within(window: Delta, p: f64, term: Term) -> Result<Term, FplError> {
    if window <= Duration::zero() {
        return err("Within.window must be positive");
    }
    if !(-32.0..=32.0).contains(&p) {
        return err(format!(
            "Within.p must be in [-32, 32] (overflow guard), got {p}"
        ));
    }
    Ok(Term::new(TermF::Within { window, p, term }))
}

pub fn mk_importance(w: f64, term: Term) -> Result<Term, FplError> {
    if w <= 0.0 {
        return err(format!("Importance.w must be positive, got {w}"));
    }
    Ok(Term::new(TermF::Importance { w, term }))
}

pub fn mk_offset_by(delta: Term, term: Term) -> Result<Term, FplError> {
    Ok(Term::new(TermF::OffsetBy { delta, term }))
}

pub fn mk_after(
    event: String,
    anchor: Instant,
    term: Term,
    pending: Term,
    needs: Option<Delta>,
) -> Result<Term, FplError> {
    if event.is_empty() {
        return err("After.event must name a completion event");
    }
    if let Some(n) = needs {
        if n <= Duration::zero() {
            return err("After.needs must be positive");
        }
    }
    Ok(Term::new(TermF::After {
        event,
        anchor,
        term,
        pending,
        needs,
    }))
}

/// A todo's fulfillment function given its own spec and its checklist: each
/// item is a leaf worth 0.5, the checklist is their conjunction, and the
/// parent's own value is an OFFSET on that aggregate. No items: the spec
/// itself. No spec: the aggregate alone. Neither: `None`.
pub fn checklist(own: Option<Term>, items: usize) -> Result<Option<Term>, FplError> {
    if items == 0 {
        return Ok(own);
    }
    let mut leaves = Vec::with_capacity(items);
    for _ in 0..items {
        leaves.push(mk_flat(0.5)?);
    }
    let aggregate = mk_conj(leaves, PRIORITY_POWER)?;
    Ok(Some(match own {
        None => aggregate,
        Some(o) => mk_offset_by(o, aggregate)?,
    }))
}

/// The NORMAL FORM of a schedule of functions, and the laws it satisfies:
/// unit (no pieces IS the head), join (a piece whose term is a Piecewise is
/// spliced in, so no Piecewise ever nests), two adjacent pieces with the same
/// term are one piece, and instants strictly increase or this refuses.
/// `fulfillment` gives the raw record and its normal form the same reading at
/// every instant.
pub fn mk_piecewise(head: Term, pieces: Vec<(Instant, Term)>) -> Result<Term, FplError> {
    for pair in pieces.windows(2) {
        if pair[0].0 >= pair[1].0 {
            return err(format!(
                "piecewise instants must strictly increase: {} then {}",
                iso(pair[0].0),
                iso(pair[1].0)
            ));
        }
    }
    Ok(piecewise(head, pieces))
}

/// `mk_piecewise` with the ordering precondition already established — the
/// internal path, so `normalize` needs no `Result` it could not produce.
fn piecewise(head: Term, pieces: Vec<(Instant, Term)>) -> Term {
    let first = pieces.first().map(|p| p.0);
    let (start, start_pieces) = opened(head);
    let mut schedule: Vec<(Instant, Term)> = start_pieces
        .into_iter()
        .filter(|(at, _)| first.is_none_or(|f| *at < f))
        .collect();
    for (i, piece) in pieces.iter().enumerate() {
        schedule.extend(spliced(piece, pieces.get(i + 1).map(|p| p.0)));
    }
    let mut out: Vec<(Instant, Term)> = vec![];
    let mut current = start.clone();
    for (at, term) in schedule {
        if term != current {
            current = term.clone();
            out.push((at, term));
        }
    }
    if out.is_empty() {
        start
    } else {
        Term::new(TermF::Piecewise {
            head: start,
            pieces: out,
        })
    }
}

/// A term as (the function before its first transition, its transitions),
/// normalised first so the split is one level deep and stays so.
fn opened(term: Term) -> (Term, Vec<(Instant, Term)>) {
    match term.out() {
        TermF::Piecewise { head, pieces } => {
            let normal = piecewise(head.clone(), pieces.clone());
            match normal.out() {
                TermF::Piecewise { head, pieces } => (head.clone(), pieces.clone()),
                _ => (normal, vec![]),
            }
        }
        _ => (term, vec![]),
    }
}

/// One outer piece as a schedule: at its instant, whichever inner function is
/// in force there; then the inner transitions after it and before the next.
fn spliced(piece: &(Instant, Term), until: Option<Instant>) -> Vec<(Instant, Term)> {
    let (inner, inner_pieces) = opened(piece.1.clone());
    let mut at_instant = inner;
    let mut later = vec![];
    for (at, term) in inner_pieces {
        if at <= piece.0 {
            at_instant = term;
        } else if until.is_none_or(|u| at < u) {
            later.push((at, term));
        }
    }
    let mut out = vec![(piece.0, at_instant)];
    out.extend(later);
    out
}

/// The Piecewise-outermost normal form: every operator that reads its subterms
/// POINTWISE (Conj, Offset, Gate, Importance, OffsetBy) is pushed under a
/// Piecewise over the merged partition of its parts' instants, and a Shift
/// translates the instants it crosses. Within averages over a window and After
/// binds history, so neither commutes with a partition; their subterms are
/// normalised and they stay where they are.
pub fn normalize(term: &Term) -> Term {
    match term.out() {
        TermF::Flat { .. } | TermF::Decay { .. } | TermF::Curve { .. } => term.clone(),
        TermF::Piecewise { head, pieces } => piecewise(
            normalize(head),
            pieces.iter().map(|(at, t)| (*at, normalize(t))).collect(),
        ),
        TermF::Conj { terms, p } => {
            let p = *p;
            pointwise(terms.iter().map(normalize).collect(), &|parts| {
                Term::new(TermF::Conj {
                    terms: parts.to_vec(),
                    p,
                })
            })
        }
        TermF::Offset { delta, term } => {
            let delta = *delta;
            pointwise(vec![normalize(term)], &|parts| {
                Term::new(TermF::Offset {
                    delta,
                    term: parts[0].clone(),
                })
            })
        }
        TermF::Gate { gate, body } => pointwise(vec![normalize(gate), normalize(body)], &|parts| {
            Term::new(TermF::Gate {
                gate: parts[0].clone(),
                body: parts[1].clone(),
            })
        }),
        TermF::OffsetBy { delta, term } => {
            pointwise(vec![normalize(delta), normalize(term)], &|parts| {
                Term::new(TermF::OffsetBy {
                    delta: parts[0].clone(),
                    term: parts[1].clone(),
                })
            })
        }
        TermF::Importance { w, term } => {
            let w = *w;
            pointwise(vec![normalize(term)], &|parts| {
                Term::new(TermF::Importance {
                    w,
                    term: parts[0].clone(),
                })
            })
        }
        TermF::Shift { delta, term } => {
            let by = *delta;
            let inner = normalize(term);
            match inner.out() {
                // ⟦Shift(δ, pw)⟧(now) = ⟦pw⟧(now + δ), so a piece from τ is in
                // force from τ − δ.
                TermF::Piecewise { head, pieces } => piecewise(
                    Term::new(TermF::Shift {
                        delta: by,
                        term: head.clone(),
                    }),
                    pieces
                        .iter()
                        .map(|(at, t)| {
                            (
                                after(*at, -us_of(by)),
                                Term::new(TermF::Shift {
                                    delta: by,
                                    term: t.clone(),
                                }),
                            )
                        })
                        .collect(),
                ),
                _ => Term::new(TermF::Shift {
                    delta: by,
                    term: inner,
                }),
            }
        }
        TermF::Within { window, p, term } => Term::new(TermF::Within {
            window: *window,
            p: *p,
            term: normalize(term),
        }),
        TermF::After {
            event,
            anchor,
            term,
            pending,
            needs,
        } => Term::new(TermF::After {
            event: event.clone(),
            anchor: *anchor,
            term: normalize(term),
            pending: normalize(pending),
            needs: *needs,
        }),
    }
}

/// An operator over already-normalised `parts`, lifted over their schedules:
/// the head is the operator over the heads, and at every instant any part
/// changes, a piece with the operator over what each part reads there. Parts
/// without a schedule are constant in the partition.
fn pointwise(parts: Vec<Term>, rebuild: &dyn Fn(&[Term]) -> Term) -> Term {
    if !parts
        .iter()
        .any(|p| matches!(p.out(), TermF::Piecewise { .. }))
    {
        return rebuild(&parts);
    }
    let mut instants: BTreeSet<Instant> = BTreeSet::new();
    for part in &parts {
        if let TermF::Piecewise { pieces, .. } = part.out() {
            instants.extend(pieces.iter().map(|(at, _)| *at));
        }
    }
    let at = |part: &Term, moment: Option<Instant>| -> Term {
        match part.out() {
            TermF::Piecewise { head, pieces } => match moment {
                None => head.clone(),
                Some(m) => in_force(head, pieces, m).1.clone(),
            },
            _ => part.clone(),
        }
    };
    let head = rebuild(&parts.iter().map(|p| at(p, None)).collect::<Vec<_>>());
    let pieces: Vec<(Instant, Term)> = instants
        .into_iter()
        .map(|m| {
            (
                m,
                rebuild(&parts.iter().map(|p| at(p, Some(m))).collect::<Vec<_>>()),
            )
        })
        .collect();
    piecewise(head, pieces)
}

// --- §7 JSON boundary (parse, don't validate — via the smart constructors) ---

/// Python's `datetime.isoformat()`: microseconds only when non-zero.
pub fn iso(t: Instant) -> String {
    if t.and_utc().timestamp_subsec_micros() == 0 {
        t.format("%Y-%m-%dT%H:%M:%S").to_string()
    } else {
        t.format("%Y-%m-%dT%H:%M:%S%.6f").to_string()
    }
}

/// `iso`'s inverse, for anything reading the JSON wire.
pub fn parse_iso(s: &str) -> Result<Instant, FplError> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map(|d| d.and_hms_opt(0, 0, 0).expect("midnight is a time"))
        })
        .map_err(|e| FplError(format!("not an ISO instant: {s:?} ({e})")))
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

// --- §2 the literal bridge, restricted to the term constructors -------------
//
// A small, correct printer and parser for the §2 grammar as the term
// constructors use it (floats, strings, `datetime`, `timedelta`, tuples,
// keyword calls). `literal.rs` owns the general form over every stored kind;
// these are written to the same rules so the two can be unified without a
// behaviour change.

/// Python `repr` for a float: the shortest string that round-trips, with `.0`
/// on integral values, and exponent form when the decimal point falls at or
/// before −4 or past 16.
pub fn print_float(v: f64) -> String {
    if v.is_nan() || v.is_infinite() {
        // §2: not storable. Naming it beats panicking inside a printer.
        return if v.is_nan() {
            "nan".into()
        } else if v > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        };
    }
    let sign = if v.is_sign_negative() { "-" } else { "" };
    let exp_form = format!("{:e}", v.abs());
    let (mantissa, exponent) = exp_form
        .split_once('e')
        .expect("LowerExp always emits an 'e'");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let decpt: i32 = exponent
        .parse::<i32>()
        .expect("LowerExp exponent is an integer")
        + 1;
    let n = digits.len() as i32;
    if decpt <= -4 || decpt > 16 {
        let e = decpt - 1;
        let (head, tail) = digits.split_at(1);
        let frac = if tail.is_empty() {
            String::new()
        } else {
            format!(".{tail}")
        };
        format!(
            "{sign}{head}{frac}e{}{:02}",
            if e < 0 { '-' } else { '+' },
            e.abs()
        )
    } else if decpt <= 0 {
        format!("{sign}0.{}{digits}", "0".repeat((-decpt) as usize))
    } else if decpt >= n {
        format!("{sign}{digits}{}.0", "0".repeat((decpt - n) as usize))
    } else {
        let (a, b) = digits.split_at(decpt as usize);
        format!("{sign}{a}.{b}")
    }
}

/// Python `repr` for a string: single quotes unless the text holds `'` and no
/// `"`; `\\`, the delimiter, `\n`, `\r`, `\t`, then `\xNN`/`\uNNNN`/`\UNNNNNNNN`
/// for the non-printable. Printable non-ASCII is written as itself.
pub fn print_str(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || (c as u32) == 0x7f => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c if (c as u32) < 0x80 => out.push(c),
            c if !printable_non_ascii(c) => {
                let n = c as u32;
                let _ = if n <= 0xff {
                    write!(out, "\\x{n:02x}")
                } else if n <= 0xffff {
                    write!(out, "\\u{n:04x}")
                } else {
                    write!(out, "\\U{n:08x}")
                };
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// A deliberately narrow stand-in for `str.isprintable` over non-ASCII: the C1
/// controls, the separators, the formatting characters and the surrogates
/// escape, everything else is written as itself. The full Unicode-category
/// answer belongs in `literal.rs`; a term's own string fields (an event name,
/// a curve label) do not reach past this.
fn printable_non_ascii(c: char) -> bool {
    !matches!(c as u32,
        0x80..=0x9f | 0xa0 | 0xad | 0x2000..=0x200f | 0x2028..=0x202f
        | 0x205f..=0x206f | 0x3000 | 0xd800..=0xdfff | 0xfeff | 0xfff9..=0xfffb)
}

/// `datetime(Y, M, D, h, m, s[, µs])` — the seventh argument only when non-zero.
pub fn print_instant(t: Instant) -> String {
    let us = t.and_utc().timestamp_subsec_micros();
    let head = format!(
        "datetime({}, {}, {}, {}, {}, {}",
        t.year(),
        t.month(),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    );
    if us == 0 {
        format!("{head})")
    } else {
        format!("{head}, {us})")
    }
}

/// `timedelta(days=…, seconds=…, microseconds=…)` — only the non-zero parts,
/// in that order, over CPython's normalisation (`0 <= seconds < 86400`).
pub fn print_delta(d: Delta) -> String {
    let us = us_of(d);
    let day = 86_400_000_000i64;
    let rest = us.rem_euclid(day);
    let parts: Vec<String> = [
        ("days", us.div_euclid(day)),
        ("seconds", rest / 1_000_000),
        ("microseconds", rest % 1_000_000),
    ]
    .into_iter()
    .filter(|(_, v)| *v != 0)
    .map(|(name, v)| format!("{name}={v}"))
    .collect();
    format!("timedelta({})", parts.join(", "))
}

fn print_tuple(items: &[String]) -> String {
    match items.len() {
        0 => "()".into(),
        1 => format!("({},)", items[0]),
        _ => format!("({})", items.join(", ")),
    }
}

/// The canonical §2 print of a term: EVERY field, in declared order, keyword
/// form. Equal terms print byte-identically, which is what lets the print be
/// the identity.
pub fn print_term(term: &Term) -> String {
    match term.out() {
        TermF::Flat { value } => format!("Flat(value={})", print_float(*value)),
        TermF::Decay {
            start,
            end,
            end_date,
            lead_up,
            start_date,
        } => format!(
            "Decay(start={}, end={}, end_date={}, lead_up={}, start_date={})",
            print_float(*start),
            print_float(*end),
            print_instant(*end_date),
            print_delta(*lead_up),
            start_date
                .map(print_instant)
                .unwrap_or_else(|| "None".into())
        ),
        TermF::Curve { points } => {
            let pts: Vec<String> = points
                .iter()
                .map(|pt| {
                    format!(
                        "CurvePoint(at={}, value={}, label={})",
                        print_instant(pt.at),
                        print_float(pt.value),
                        print_str(&pt.label)
                    )
                })
                .collect();
            format!("Curve(points={})", print_tuple(&pts))
        }
        TermF::Conj { terms, p } => {
            let ts: Vec<String> = terms.iter().map(print_term).collect();
            format!("Conj(terms={}, p={})", print_tuple(&ts), print_float(*p))
        }
        TermF::Offset { delta, term } => {
            format!(
                "Offset(delta={}, term={})",
                print_float(*delta),
                print_term(term)
            )
        }
        TermF::Gate { gate, body } => {
            format!("Gate(gate={}, body={})", print_term(gate), print_term(body))
        }
        TermF::OffsetBy { delta, term } => {
            format!(
                "OffsetBy(delta={}, term={})",
                print_term(delta),
                print_term(term)
            )
        }
        TermF::Shift { delta, term } => {
            format!(
                "Shift(delta={}, term={})",
                print_delta(*delta),
                print_term(term)
            )
        }
        TermF::Within { window, p, term } => format!(
            "Within(window={}, p={}, term={})",
            print_delta(*window),
            print_float(*p),
            print_term(term)
        ),
        TermF::Importance { w, term } => {
            format!(
                "Importance(w={}, term={})",
                print_float(*w),
                print_term(term)
            )
        }
        TermF::After {
            event,
            anchor,
            term,
            pending,
            needs,
        } => format!(
            "After(event={}, anchor={}, term={}, pending={}, needs={})",
            print_str(event),
            print_instant(*anchor),
            print_term(term),
            print_term(pending),
            needs.map(print_delta).unwrap_or_else(|| "None".into())
        ),
        TermF::Piecewise { head, pieces } => {
            let ps: Vec<String> = pieces
                .iter()
                .map(|(at, t)| format!("Piece(at={}, term={})", print_instant(*at), print_term(t)))
                .collect();
            format!(
                "Piecewise(head={}, pieces={})",
                print_term(head),
                print_tuple(&ps)
            )
        }
    }
}

/// One expression of the §2 grammar, before it means anything.
#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Tuple(Vec<Lit>),
    Call {
        name: String,
        pos: Vec<Lit>,
        kw: Vec<(String, Lit)>,
    },
}

struct Parser<'a> {
    src: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn space(&mut self) {
        while self.i < self.src.len() && self.src[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.space();
        self.src.get(self.i).copied()
    }
    fn eat(&mut self, c: u8) -> Result<(), FplError> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            err(format!("expected {:?} at byte {}", c as char, self.i))
        }
    }
    fn name(&mut self) -> String {
        self.space();
        let start = self.i;
        while self
            .src
            .get(self.i)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
        {
            self.i += 1;
        }
        String::from_utf8_lossy(&self.src[start..self.i]).into_owned()
    }
    fn string(&mut self) -> Result<Lit, FplError> {
        let quote = self.src[self.i];
        self.i += 1;
        let mut out = String::new();
        loop {
            let c = *self
                .src
                .get(self.i)
                .ok_or_else(|| FplError("unterminated string".into()))?;
            if c == quote {
                self.i += 1;
                return Ok(Lit::Str(out));
            }
            if c != b'\\' {
                // Step over the whole UTF-8 run, not the lead byte alone.
                let start = self.i;
                self.i = start + utf8_len(c);
                if self.i > self.src.len() {
                    return err("truncated UTF-8 in string");
                }
                out.push_str(&String::from_utf8_lossy(&self.src[start..self.i]));
                continue;
            }
            self.i += 1;
            let e = *self
                .src
                .get(self.i)
                .ok_or_else(|| FplError("unterminated escape".into()))?;
            self.i += 1;
            match e {
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'\\' | b'\'' | b'"' => out.push(e as char),
                b'x' | b'u' | b'U' => {
                    let width = match e {
                        b'x' => 2,
                        b'u' => 4,
                        _ => 8,
                    };
                    let end = self.i + width;
                    if end > self.src.len() {
                        return err("truncated escape");
                    }
                    let hex = std::str::from_utf8(&self.src[self.i..end])
                        .map_err(|_| FplError("bad escape".into()))?;
                    self.i = end;
                    let n =
                        u32::from_str_radix(hex, 16).map_err(|_| FplError("bad escape".into()))?;
                    out.push(char::from_u32(n).ok_or_else(|| FplError("bad codepoint".into()))?);
                }
                other => return err(format!("unknown escape \\{}", other as char)),
            }
        }
    }
    fn number(&mut self) -> Result<Lit, FplError> {
        let start = self.i;
        if self.src.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        let mut float = false;
        while let Some(c) = self.src.get(self.i) {
            match c {
                b'0'..=b'9' => self.i += 1,
                b'.' | b'e' | b'E' => {
                    float = true;
                    self.i += 1;
                }
                b'+' | b'-' if matches!(self.src.get(self.i - 1), Some(b'e' | b'E')) => self.i += 1,
                _ => break,
            }
        }
        let text = std::str::from_utf8(&self.src[start..self.i]).unwrap_or("");
        if float {
            text.parse::<f64>()
                .map(Lit::Float)
                .map_err(|e| FplError(format!("bad float {text:?}: {e}")))
        } else {
            text.parse::<i64>()
                .map(Lit::Int)
                .map_err(|e| FplError(format!("bad int {text:?}: {e}")))
        }
    }
    fn tuple(&mut self) -> Result<Lit, FplError> {
        self.i += 1;
        let mut items = vec![];
        loop {
            if self.peek() == Some(b')') {
                self.i += 1;
                return Ok(Lit::Tuple(items));
            }
            items.push(self.expr()?);
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b')') => {}
                _ => return err(format!("expected ',' or ')' at byte {}", self.i)),
            }
        }
    }
    fn call(&mut self, name: String) -> Result<Lit, FplError> {
        self.eat(b'(')?;
        let (mut pos, mut kw) = (vec![], vec![]);
        loop {
            if self.peek() == Some(b')') {
                self.i += 1;
                return Ok(Lit::Call { name, pos, kw });
            }
            let save = self.i;
            let key = if matches!(self.peek(), Some(c) if c.is_ascii_alphabetic() || c == b'_') {
                let k = self.name();
                if self.peek() == Some(b'=') {
                    self.i += 1;
                    Some(k)
                } else {
                    self.i = save;
                    None
                }
            } else {
                None
            };
            let value = self.expr()?;
            match key {
                Some(k) => kw.push((k, value)),
                None => pos.push(value),
            }
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b')') => {}
                _ => return err(format!("expected ',' or ')' at byte {}", self.i)),
            }
        }
    }
    fn expr(&mut self) -> Result<Lit, FplError> {
        match self.peek() {
            None => err("unexpected end of expression"),
            Some(b'(') => self.tuple(),
            Some(b'\'' | b'"') => self.string(),
            Some(c) if c.is_ascii_digit() || c == b'-' => self.number(),
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => {
                let name = self.name();
                match name.as_str() {
                    "None" => Ok(Lit::None),
                    "True" => Ok(Lit::Bool(true)),
                    "False" => Ok(Lit::Bool(false)),
                    _ => self.call(name),
                }
            }
            Some(c) => err(format!("unexpected {:?} at byte {}", c as char, self.i)),
        }
    }
}

fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

/// Parse one §2 expression. The grammar IS the whitelist — nothing else is
/// admitted, and a refusal is a value, never a crash.
pub fn parse_literal(text: &str) -> Result<Lit, FplError> {
    let mut p = Parser {
        src: text.as_bytes(),
        i: 0,
    };
    let lit = p.expr()?;
    p.space();
    if p.i != p.src.len() {
        return err(format!("trailing input at byte {}", p.i));
    }
    Ok(lit)
}

fn arg<'a>(pos: &'a [Lit], kw: &'a [(String, Lit)], index: usize, name: &str) -> Option<&'a Lit> {
    pos.get(index)
        .or_else(|| kw.iter().find(|(k, _)| k == name).map(|(_, v)| v))
}

fn need<'a>(
    call: &str,
    pos: &'a [Lit],
    kw: &'a [(String, Lit)],
    index: usize,
    name: &str,
) -> Result<&'a Lit, FplError> {
    arg(pos, kw, index, name).ok_or_else(|| FplError(format!("{call}(...) missing {name}")))
}

fn as_f64(l: &Lit) -> Result<f64, FplError> {
    match l {
        Lit::Float(f) => Ok(*f),
        Lit::Int(i) => Ok(*i as f64),
        other => err(format!("expected a number, got {other:?}")),
    }
}

fn as_i64(l: &Lit) -> Result<i64, FplError> {
    match l {
        Lit::Int(i) => Ok(*i),
        other => err(format!("expected an integer, got {other:?}")),
    }
}

fn as_str(l: &Lit) -> Result<String, FplError> {
    match l {
        Lit::Str(s) => Ok(s.clone()),
        other => err(format!("expected a string, got {other:?}")),
    }
}

fn as_instant(l: &Lit) -> Result<Instant, FplError> {
    match l {
        Lit::Call { name, pos, kw } if name == "datetime" => {
            let get = |i: usize, n: &str, default: i64| -> Result<i64, FplError> {
                match arg(pos, kw, i, n) {
                    Some(v) => as_i64(v),
                    None => Ok(default),
                }
            };
            let (y, mo, d) = (get(0, "year", 0)?, get(1, "month", 1)?, get(2, "day", 1)?);
            let (h, mi, s) = (
                get(3, "hour", 0)?,
                get(4, "minute", 0)?,
                get(5, "second", 0)?,
            );
            let us = get(6, "microsecond", 0)?;
            u32::try_from(mo)
                .ok()
                .zip(u32::try_from(d).ok())
                .and_then(|(mo, d)| NaiveDate::from_ymd_opt(y as i32, mo, d))
                .and_then(|date| date.and_hms_micro_opt(h as u32, mi as u32, s as u32, us as u32))
                .ok_or_else(|| {
                    FplError(format!("datetime out of range: {y}-{mo}-{d} {h}:{mi}:{s}"))
                })
        }
        other => err(format!("expected datetime(...), got {other:?}")),
    }
}

fn as_delta(l: &Lit) -> Result<Delta, FplError> {
    match l {
        Lit::Call { name, pos, kw } if name == "timedelta" => {
            let get = |i: usize, n: &str| -> Result<i64, FplError> {
                match arg(pos, kw, i, n) {
                    Some(v) => as_i64(v),
                    None => Ok(0),
                }
            };
            Ok(Duration::microseconds(
                get(0, "days")? * 86_400_000_000
                    + get(1, "seconds")? * 1_000_000
                    + get(2, "microseconds")?,
            ))
        }
        other => err(format!("expected timedelta(...), got {other:?}")),
    }
}

fn as_opt<T>(l: &Lit, f: impl Fn(&Lit) -> Result<T, FplError>) -> Result<Option<T>, FplError> {
    match l {
        Lit::None => Ok(None),
        other => f(other).map(Some),
    }
}

fn as_tuple(l: &Lit) -> Result<&[Lit], FplError> {
    match l {
        Lit::Tuple(items) => Ok(items),
        other => err(format!("expected a tuple, got {other:?}")),
    }
}

/// A §2 literal as a `Term`, dispatched into the smart constructors — whose
/// error IS the parse error (§2). The vocabulary is exactly the term
/// constructors plus `CurvePoint`, `Piece`, `datetime` and `timedelta`.
pub fn term_of_literal(l: &Lit) -> Result<Term, FplError> {
    let Lit::Call { name, pos, kw } = l else {
        return err(format!("expected a term constructor, got {l:?}"));
    };
    let a = |i: usize, n: &str| need(name, pos, kw, i, n);
    match name.as_str() {
        "Flat" => mk_flat(as_f64(a(0, "value")?)?),
        "Decay" => mk_decay(
            as_f64(a(0, "start")?)?,
            as_f64(a(1, "end")?)?,
            as_instant(a(2, "end_date")?)?,
            match arg(pos, kw, 3, "lead_up") {
                Some(v) => as_delta(v)?,
                None => Duration::weeks(1),
            },
            match arg(pos, kw, 4, "start_date") {
                Some(v) => as_opt(v, as_instant)?,
                None => None,
            },
        ),
        "Curve" => {
            let mut points = vec![];
            for pt in as_tuple(a(0, "points")?)? {
                let Lit::Call {
                    name: pn,
                    pos: pp,
                    kw: pk,
                } = pt
                else {
                    return err(format!("expected CurvePoint(...), got {pt:?}"));
                };
                if pn != "CurvePoint" {
                    return err(format!("unknown constructor {pn:?} in Curve.points"));
                }
                points.push(CurvePoint {
                    at: as_instant(need(pn, pp, pk, 0, "at")?)?,
                    value: as_f64(need(pn, pp, pk, 1, "value")?)?,
                    label: match arg(pp, pk, 2, "label") {
                        Some(v) => as_str(v)?,
                        None => String::new(),
                    },
                });
            }
            mk_curve(points)
        }
        "Conj" => {
            let mut terms = vec![];
            for t in as_tuple(a(0, "terms")?)? {
                terms.push(term_of_literal(t)?);
            }
            mk_conj(
                terms,
                match arg(pos, kw, 1, "p") {
                    Some(v) => as_f64(v)?,
                    None => PRIORITY_POWER,
                },
            )
        }
        "Offset" => mk_offset(as_f64(a(0, "delta")?)?, term_of_literal(a(1, "term")?)?),
        "Gate" => mk_gate(
            term_of_literal(a(0, "gate")?)?,
            term_of_literal(a(1, "body")?)?,
        ),
        "OffsetBy" => mk_offset_by(
            term_of_literal(a(0, "delta")?)?,
            term_of_literal(a(1, "term")?)?,
        ),
        "Shift" => mk_shift(as_delta(a(0, "delta")?)?, term_of_literal(a(1, "term")?)?),
        "Within" => mk_within(
            as_delta(a(0, "window")?)?,
            as_f64(a(1, "p")?)?,
            term_of_literal(a(2, "term")?)?,
        ),
        "Importance" => mk_importance(as_f64(a(0, "w")?)?, term_of_literal(a(1, "term")?)?),
        "After" => mk_after(
            as_str(a(0, "event")?)?,
            as_instant(a(1, "anchor")?)?,
            term_of_literal(a(2, "term")?)?,
            term_of_literal(a(3, "pending")?)?,
            match arg(pos, kw, 4, "needs") {
                Some(v) => as_opt(v, as_delta)?,
                None => None,
            },
        ),
        "Piecewise" => {
            let mut pieces = vec![];
            for p in as_tuple(a(1, "pieces")?)? {
                let Lit::Call {
                    name: pn,
                    pos: pp,
                    kw: pk,
                } = p
                else {
                    return err(format!("expected Piece(...), got {p:?}"));
                };
                if pn != "Piece" {
                    return err(format!("unknown constructor {pn:?} in Piecewise.pieces"));
                }
                pieces.push((
                    as_instant(need(pn, pp, pk, 0, "at")?)?,
                    term_of_literal(need(pn, pp, pk, 1, "term")?)?,
                ));
            }
            mk_piecewise(term_of_literal(a(0, "head")?)?, pieces)
        }
        other => err(format!("unknown constructor {other:?}")),
    }
}

/// `print_term`'s inverse: one canonical literal back into a `Term`.
pub fn parse_term(text: &str) -> Result<Term, FplError> {
    term_of_literal(&parse_literal(text)?)
}
