//! §7 — FPL: the term language, one evaluator.
//!
//! See ../../SPEC.md. Implemented against ../../conformance/fpl.py, and
//! ported from the reference `fpl.py` — the arithmetic ORDER is part
//! of the port, because the vectors are the reference's floats.
//!
//! Types first (§1, §8). `TermF<A>` is ONE LAYER of the term functor with its
//! child positions as `A`; `Term` is its fixed point and `Explanation` is the
//! same shape carrying an annotation — the Cofree the spec calls "decoration,
//! not evaluation". Invariants live in the `mk_*` smart constructors; a `Term`
//! that exists came through one of them (or through `parse_term`, which calls
//! them).
//!
//! NO JSON HERE. A term has ONE serialization and it is §2's literal print;
//! the JSON shape a browser reads is `prodrome-wasm`'s `json` module, built on
//! the types below, because JSON is JavaScript's literal grammar and this crate
//! has its own.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use thiserror::Error;

use crate::literal::{self, print_literal, Call, Finite, ProdromeError, Table};

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

    /// The kind TAG — the name `prodrome-wasm`'s JSON shape puts on this
    /// layer, and the one word of that shape the core still owns, because an
    /// explanation's notes are keyed beside it.
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

/// The term's own fields, as notes: every scalar of this layer, keyed the way
/// the wire keys it, and none of its subterms.
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

/// The term's own shape, with every node carrying its own fulfillment.
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
    // Stated as what `w` MUST be rather than what it must not: `w <= 0.0` is
    // false for a NaN, which would have admitted the one unprintable float a
    // term can hold (§2: `inf`/`nan` are not storable).
    if !(w.is_finite() && w > 0.0) {
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

// --- ISO instants: the one instant SPELLING every boundary shares ------------

/// Python's `datetime.isoformat()`: microseconds only when non-zero.
pub fn iso(t: Instant) -> String {
    if t.and_utc().timestamp_subsec_micros() == 0 {
        t.format("%Y-%m-%dT%H:%M:%S").to_string()
    } else {
        t.format("%Y-%m-%dT%H:%M:%S%.6f").to_string()
    }
}

/// `iso`'s inverse, for anything reading an instant off a wire.
pub fn parse_iso(s: &str) -> Result<Instant, FplError> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map(|d| d.and_hms_opt(0, 0, 0).expect("midnight is a time"))
        })
        .map_err(|e| FplError(format!("not an ISO instant: {s:?} ({e})")))
}

// --- §2 the literal bridge: `literal` is the grammar ------------------------
//
// ONE printer and ONE parser for the whole store. `literal::Value` is the
// grammar's expression, `literal::print_literal` its canonical print and
// `literal::parse_literal` its strict parser; everything below is the
// translation between a `Term` and one of those expressions, plus the
// vocabulary §7 contributes to a loader — `TERM_SIGNATURES`, which `event`
// chains onto §4's so a spec can nest anywhere inside a stored object.
//
// The two layers keep DIFFERENT time types, deliberately. `literal::Datetime`
// and `literal::Timedelta` are the GRAMMAR's records: CPython's field bounds
// and its normalisation, and no arithmetic at all. Evaluation needs arithmetic
// on instants and spans — `now + δ`, `done − anchor`, a window cut into 64 —
// and takes `chrono`'s. Unifying them would mean either a date library inside
// `literal` or an evaluator built on a record with no `+`, so the layers meet
// HERE, in four total conversions, and the grammar stays ignorant of time as
// anything but a shape.

/// §7's constructors, name and declared field order — the vocabulary a literal
/// loader consults for a term. §2's `datetime`/`timedelta` are the grammar's
/// own and need no entry.
pub const TERM_SIGNATURES: &[(&str, &[&str])] = &[
    ("Flat", &["value"]),
    (
        "Decay",
        &["start", "end", "end_date", "lead_up", "start_date"],
    ),
    ("Curve", &["points"]),
    ("CurvePoint", &["at", "value", "label"]),
    ("Conj", &["terms", "p"]),
    ("Offset", &["delta", "term"]),
    ("Gate", &["gate", "body"]),
    ("Shift", &["delta", "term"]),
    ("Within", &["window", "p", "term"]),
    ("Importance", &["w", "term"]),
    ("After", &["event", "anchor", "term", "pending", "needs"]),
    ("Piecewise", &["head", "pieces"]),
    ("Piece", &["at", "term"]),
    ("OffsetBy", &["delta", "term"]),
];

/// The vocabulary of a BARE term — what [`parse_term`] reads against. A stored
/// object is read against `event::EventVocabulary<P>`, which is this, §4's own
/// kinds, and the host payload's.
pub const TERM_VOCABULARY: Table = Table(TERM_SIGNATURES);

/// A term's refusal, as the store's refusal. The two layers keep their own
/// error types because their vocabularies of failure differ — a store reports
/// a missing parent, a term an out-of-range exponent — and this is the one
/// direction the boundary needs.
impl From<FplError> for ProdromeError {
    fn from(error: FplError) -> ProdromeError {
        ProdromeError::Invalid(error.0)
    }
}

/// And back: a grammar refusal reaching a caller who asked for a term.
impl From<ProdromeError> for FplError {
    fn from(error: ProdromeError) -> FplError {
        FplError(error.to_string())
    }
}

fn f(name: &str, value: literal::Value) -> (String, literal::Value) {
    (name.to_owned(), value)
}

/// Every float inside a `Term` is finite: `unit`, `mk_conj`'s range,
/// `mk_offset`'s and `mk_importance`'s all refuse a NaN or an infinity, so the
/// printer is total and the `Finite` smart constructor cannot fire here.
fn float_value(value: f64) -> literal::Value {
    literal::Value::Float(
        Finite::new(value).expect("a Term's floats are finite: every mk_* refuses the rest"),
    )
}

/// The grammar's `datetime` as an evaluable instant — THE place §2's record
/// becomes §7's arithmetic, and what `fold` uses to read an event's `at`.
/// Total: `literal::Datetime`'s constructor already refused everything a
/// `NaiveDateTime` cannot hold, so the `expect` is a proof and not a hope.
pub fn instant_of(at: literal::Datetime) -> Instant {
    NaiveDate::from_ymd_opt(at.year(), at.month(), at.day())
        .and_then(|day| {
            day.and_hms_micro_opt(at.hour(), at.minute(), at.second(), at.microsecond())
        })
        .expect("literal::Datetime::new admits only representable instants")
}

/// And back, for an instant the evaluator computed. `literal::Datetime` is the
/// narrower type — years 1..=9999, as CPython — so this is the fallible
/// direction, and the caller that cannot fail is the one printing a `Term`
/// whose instants all came through [`instant_of`].
pub fn datetime_of(t: Instant) -> Result<literal::Datetime, ProdromeError> {
    literal::Datetime::new(
        t.year(),
        t.month(),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
        t.and_utc().timestamp_subsec_micros(),
    )
}

fn instant_value(t: Instant) -> literal::Value {
    literal::Value::Datetime(
        datetime_of(t)
            .expect("a Term's instants came through the grammar, whose years are 1..=9999"),
    )
}

fn instant_field(value: &literal::Value, context: &str) -> Result<Instant, FplError> {
    match value {
        literal::Value::Datetime(at) => Ok(instant_of(*at)),
        other => err(format!("{context} must be a datetime(...), got {other:?}")),
    }
}

/// A `chrono` span as the grammar's `timedelta(...)`. An i64 of microseconds is
/// at most ~107 000 days, well inside CPython's 999 999 999-day range.
fn delta_value(d: Delta) -> literal::Value {
    literal::Value::Timedelta(
        literal::Timedelta::from_micros(i128::from(us_of(d)))
            .expect("a Delta of i64 microseconds is inside timedelta's range"),
    )
}

fn delta_field(value: &literal::Value, context: &str) -> Result<Delta, FplError> {
    match value {
        literal::Value::Timedelta(d) => i64::try_from(d.total_micros())
            .map(Duration::microseconds)
            .map_err(|_| FplError(format!("{context} is too large to evaluate"))),
        other => err(format!("{context} must be a timedelta(...), got {other:?}")),
    }
}

fn lit_field<'a>(call: &'a Call, name: &str) -> Result<&'a literal::Value, FplError> {
    call.field(name)
        .ok_or_else(|| FplError(format!("{}(...) is missing {name}", call.name)))
}

fn lit_number(call: &Call, name: &str) -> Result<f64, FplError> {
    match lit_field(call, name)? {
        literal::Value::Float(value) => Ok(value.get()),
        literal::Value::Int(int) => int
            .as_i64()
            .map(|value| value as f64)
            .ok_or_else(|| FplError(format!("{}.{name} is out of range", call.name))),
        other => err(format!(
            "{}.{name} must be a number, got {other:?}",
            call.name
        )),
    }
}

fn lit_text(call: &Call, name: &str) -> Result<String, FplError> {
    match call.field(name) {
        // A trailing label or note omitted from a hand-written literal reads as
        // the `''` the canonical print puts back, exactly as in §4.
        None => Ok(String::new()),
        Some(literal::Value::Str(text)) => Ok(text.clone()),
        Some(other) => err(format!(
            "{}.{name} must be a string, got {other:?}",
            call.name
        )),
    }
}

fn lit_tuple<'a>(call: &'a Call, name: &str) -> Result<&'a [literal::Value], FplError> {
    lit_field(call, name)?
        .as_tuple()
        .ok_or_else(|| FplError(format!("{}.{name} must be a tuple", call.name)))
}

/// One element of a tuple field, as the constructor it must be.
fn lit_element<'a>(value: &'a literal::Value, name: &str) -> Result<&'a Call, FplError> {
    match value.as_call() {
        Some(call) if call.name == name => Ok(call),
        other => err(format!("expected a {name}(...), got {other:?}")),
    }
}

impl Term {
    /// The term as ONE expression of §2's grammar: every field, in declared
    /// order, keyword form. `print_literal` of this is the canonical print, and
    /// the print is the identity because [`Term::from_value`] reads it back.
    pub fn to_value(&self) -> literal::Value {
        let call = literal::Value::call;
        match self.out() {
            TermF::Flat { value } => call("Flat", vec![f("value", float_value(*value))]),
            TermF::Decay {
                start,
                end,
                end_date,
                lead_up,
                start_date,
            } => call(
                "Decay",
                vec![
                    f("start", float_value(*start)),
                    f("end", float_value(*end)),
                    f("end_date", instant_value(*end_date)),
                    f("lead_up", delta_value(*lead_up)),
                    f(
                        "start_date",
                        start_date.map_or(literal::Value::None, instant_value),
                    ),
                ],
            ),
            TermF::Curve { points } => call(
                "Curve",
                vec![f(
                    "points",
                    literal::Value::Tuple(
                        points
                            .iter()
                            .map(|point| {
                                call(
                                    "CurvePoint",
                                    vec![
                                        f("at", instant_value(point.at)),
                                        f("value", float_value(point.value)),
                                        f("label", literal::Value::str(point.label.clone())),
                                    ],
                                )
                            })
                            .collect(),
                    ),
                )],
            ),
            TermF::Conj { terms, p } => call(
                "Conj",
                vec![
                    f(
                        "terms",
                        literal::Value::Tuple(terms.iter().map(Term::to_value).collect()),
                    ),
                    f("p", float_value(*p)),
                ],
            ),
            TermF::Offset { delta, term } => call(
                "Offset",
                vec![f("delta", float_value(*delta)), f("term", term.to_value())],
            ),
            TermF::Gate { gate, body } => call(
                "Gate",
                vec![f("gate", gate.to_value()), f("body", body.to_value())],
            ),
            TermF::Shift { delta, term } => call(
                "Shift",
                vec![f("delta", delta_value(*delta)), f("term", term.to_value())],
            ),
            TermF::Within { window, p, term } => call(
                "Within",
                vec![
                    f("window", delta_value(*window)),
                    f("p", float_value(*p)),
                    f("term", term.to_value()),
                ],
            ),
            TermF::Importance { w, term } => call(
                "Importance",
                vec![f("w", float_value(*w)), f("term", term.to_value())],
            ),
            TermF::After {
                event,
                anchor,
                term,
                pending,
                needs,
            } => call(
                "After",
                vec![
                    f("event", literal::Value::str(event.clone())),
                    f("anchor", instant_value(*anchor)),
                    f("term", term.to_value()),
                    f("pending", pending.to_value()),
                    f("needs", needs.map_or(literal::Value::None, delta_value)),
                ],
            ),
            TermF::Piecewise { head, pieces } => call(
                "Piecewise",
                vec![
                    f("head", head.to_value()),
                    f(
                        "pieces",
                        literal::Value::Tuple(
                            pieces
                                .iter()
                                .map(|(at, term)| {
                                    call(
                                        "Piece",
                                        vec![
                                            f("at", instant_value(*at)),
                                            f("term", term.to_value()),
                                        ],
                                    )
                                })
                                .collect(),
                        ),
                    ),
                ],
            ),
            TermF::OffsetBy { delta, term } => call(
                "OffsetBy",
                vec![f("delta", delta.to_value()), f("term", term.to_value())],
            ),
        }
    }

    /// One expression of §2's grammar as a `Term`, dispatched into the smart
    /// constructors — whose error IS the parse error (§2). The grammar has
    /// already checked the NAMES against [`TERM_SIGNATURES`] and put the fields
    /// in declared order; what is left is the meaning, which is this layer's.
    pub fn from_value(value: &literal::Value) -> Result<Term, FplError> {
        let call = value
            .as_call()
            .ok_or_else(|| FplError(format!("expected a term constructor, got {value:?}")))?;
        let term =
            |name: &str| -> Result<Term, FplError> { Term::from_value(lit_field(call, name)?) };
        match call.name.as_str() {
            "Flat" => mk_flat(lit_number(call, "value")?),
            "Decay" => mk_decay(
                lit_number(call, "start")?,
                lit_number(call, "end")?,
                instant_field(lit_field(call, "end_date")?, "Decay.end_date")?,
                match call.field("lead_up") {
                    Some(value) => delta_field(value, "Decay.lead_up")?,
                    None => Duration::weeks(1),
                },
                match call.field("start_date") {
                    None | Some(literal::Value::None) => None,
                    Some(value) => Some(instant_field(value, "Decay.start_date")?),
                },
            ),
            "Curve" => {
                let mut points = Vec::new();
                for item in lit_tuple(call, "points")? {
                    let point = lit_element(item, "CurvePoint")?;
                    points.push(CurvePoint {
                        at: instant_field(lit_field(point, "at")?, "CurvePoint.at")?,
                        value: lit_number(point, "value")?,
                        label: lit_text(point, "label")?,
                    });
                }
                mk_curve(points)
            }
            "Conj" => {
                let mut terms = Vec::new();
                for item in lit_tuple(call, "terms")? {
                    terms.push(Term::from_value(item)?);
                }
                mk_conj(
                    terms,
                    match call.field("p") {
                        Some(_) => lit_number(call, "p")?,
                        None => PRIORITY_POWER,
                    },
                )
            }
            "Offset" => mk_offset(lit_number(call, "delta")?, term("term")?),
            "Gate" => mk_gate(term("gate")?, term("body")?),
            "OffsetBy" => mk_offset_by(term("delta")?, term("term")?),
            "Shift" => mk_shift(
                delta_field(lit_field(call, "delta")?, "Shift.delta")?,
                term("term")?,
            ),
            "Within" => mk_within(
                delta_field(lit_field(call, "window")?, "Within.window")?,
                lit_number(call, "p")?,
                term("term")?,
            ),
            "Importance" => mk_importance(lit_number(call, "w")?, term("term")?),
            "After" => mk_after(
                lit_text(call, "event")?,
                instant_field(lit_field(call, "anchor")?, "After.anchor")?,
                term("term")?,
                term("pending")?,
                match call.field("needs") {
                    None | Some(literal::Value::None) => None,
                    Some(value) => Some(delta_field(value, "After.needs")?),
                },
            ),
            "Piecewise" => {
                let mut pieces = Vec::new();
                for item in lit_tuple(call, "pieces")? {
                    let piece = lit_element(item, "Piece")?;
                    pieces.push((
                        instant_field(lit_field(piece, "at")?, "Piece.at")?,
                        Term::from_value(lit_field(piece, "term")?)?,
                    ));
                }
                mk_piecewise(term("head")?, pieces)
            }
            other => err(format!("{other:?} is not one of SPEC §7's terms")),
        }
    }
}

/// The canonical §2 print of a term.
pub fn print_term(term: &Term) -> String {
    print_literal(&term.to_value())
}

/// `print_term`'s inverse: one canonical literal back into a `Term`, through
/// the closed vocabulary of §7 and every smart constructor.
pub fn parse_term(text: &str) -> Result<Term, FplError> {
    Term::from_value(&literal::parse_literal(text, &TERM_VOCABULARY)?)
}
