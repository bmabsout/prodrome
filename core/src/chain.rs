//! §7.1 — the chain compiler: `After` erased against a snapshot.
//!
//! See ../../SPEC.md §7.1. `After` is the ONE term that reads history (§7):
//! every other constructor is a function of `now` and its subterms, so a term
//! with no `After` anywhere in it can be evaluated against no environment at
//! all. This module is that erasure, and its whole justification is §9.13 —
//! the compiled term, read against the EMPTY environment, answers what the
//! interpreted one answered against the real one, at every instant.
//!
//! An OPTIMISATION, not a semantics change, and the difference is the point.
//! Nothing here is a second evaluator (§10): [`Compiled::fulfillment`] and
//! [`Compiled::explain`] are `fpl`'s own, called on a term this module
//! rewrote. The interpreted path stays exactly as it was, a host that never
//! compiles reads exactly what it read before, and no stored byte and no
//! conformance vector moves.
//!
//! WHAT IT BUYS. The interpreter consults the environment once per `After`
//! node per evaluation, and `Within` is 65 evaluations of its subterm — so one
//! link under a window costs 65 lookups and a chain of `d` links under one
//! costs 65·d, at EVERY query. The compiler pays one walk of the term once and
//! every evaluation afterwards pays none.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::fpl::{
    self, iso, piecewise, total_seconds, Delta, Env, Explanation, Instant, Note, Outcome, Scalar,
    Term, TermF,
};

/// A refusal from [`compile_chain`] or [`chain_order`]. One inhabitant, and it
/// is a VALUE: a chain that depends on itself is a modelling error the host
/// must be told about, never a partial answer and never a crash.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ChainError {
    /// The dependency graph the links draw over the todos closes on itself.
    /// The path names the loop, first node repeated at the end.
    #[error("the chain depends on itself: {}", .0.join(" → "))]
    Cycle(Vec<String>),
}

/// One `After` node, as the compiler resolved it against the snapshot.
///
/// The three constructors are the three readings of §7's `After` and they mean
/// OPPOSITE things downstream — a moot link is a 1.0 nobody earned — so they
/// are an ADT and never a bound-or-not boolean with fields hanging off it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// The snapshot says nothing about the upstream: the link is its pending
    /// branch at every instant, and it will re-compile differently later.
    Pending { event: String },
    /// The upstream completed at `at`, `slip` behind the `anchor` the link was
    /// authored against, so the body slides by that much from `at` on.
    /// `ready` is `at + needs` where the link declared a lead time (§7.1:
    /// `needs` is the COMPILER's field — the evaluator has never read it).
    Completed {
        event: String,
        at: Instant,
        slip: Delta,
        needs: Option<Delta>,
        ready: Option<Instant>,
    },
    /// The upstream was CANCELLED at `at`: from there the link prices as moot.
    /// Surfacing this is a requirement and not a courtesy (§7) — a 1.0 from a
    /// cancellation is indistinguishable from a 1.0 from a demand met, and
    /// this note is the only thing that tells them apart.
    Moot { event: String, at: Instant },
}

impl Link {
    /// The upstream this link names.
    pub fn event(&self) -> &str {
        match self {
            Link::Pending { event } | Link::Completed { event, .. } | Link::Moot { event, .. } => {
                event
            }
        }
    }

    /// The link's GRADED OFFSET δ ∈ [0, 1], applied with §7's corrected form:
    /// 1 where the upstream was cancelled — `x·(1 − |1|) + max(0, 1) = 1` for
    /// every `x`, which is the moot constant written as an offset — and 0
    /// everywhere else, where `offset(x, 0) = x` and the compiler elides it.
    pub fn grade(&self) -> f64 {
        match self {
            Link::Moot { .. } => 1.0,
            Link::Pending { .. } | Link::Completed { .. } => 0.0,
        }
    }

    /// The instant the link binds, where the snapshot binds it at all.
    pub fn at(&self) -> Option<Instant> {
        match self {
            Link::Pending { .. } => None,
            Link::Completed { at, .. } | Link::Moot { at, .. } => Some(*at),
        }
    }

    /// The word `explain` writes for this reading, the same three the
    /// interpreted `After`'s `bound` note uses.
    fn bound(&self) -> &'static str {
        match self {
            Link::Pending { .. } => "pending",
            Link::Completed { .. } => "completed",
            Link::Moot { .. } => "cancelled",
        }
    }

    /// This link as one map of scalars — the shape [`Compiled::notes`] carries
    /// it in, keyed the way §7's own notes are keyed.
    fn scalars(&self) -> BTreeMap<String, Scalar> {
        let mut m = BTreeMap::new();
        m.insert("event".to_owned(), Scalar::Text(self.event().to_owned()));
        m.insert("bound".to_owned(), Scalar::Text(self.bound().to_owned()));
        m.insert("grade".to_owned(), Scalar::Float(self.grade()));
        if let Some(at) = self.at() {
            m.insert("at".to_owned(), Scalar::Text(iso(at)));
        }
        if let Link::Completed {
            slip, needs, ready, ..
        } = self
        {
            m.insert(
                "slipHours".to_owned(),
                Scalar::Float(total_seconds(*slip) / 3600.0),
            );
            if let Some(needs) = needs {
                m.insert(
                    "needsHours".to_owned(),
                    Scalar::Float(total_seconds(*needs) / 3600.0),
                );
            }
            if let Some(ready) = ready {
                m.insert("readyAt".to_owned(), Scalar::Text(iso(*ready)));
            }
        }
        m
    }
}

/// A term with every `After` resolved, beside the [`Link`]s it was resolved
/// from.
///
/// A NEWTYPE and not a bare `Term`, for one reason that is worth the type: its
/// readers take NO ENVIRONMENT. That is the claim being made — a compiled term
/// cannot consult one, because the only constructor that would is gone — and a
/// bare `Term` would let a caller pass an environment that silently did
/// nothing, which is exactly the confusion this is meant to end. The links
/// ride along because a cancellation compiles to a value no reader could
/// otherwise recognise (§7).
#[derive(Debug, Clone, PartialEq)]
pub struct Compiled {
    term: Term,
    links: Vec<Link>,
}

impl Compiled {
    /// The compiled term. Storable and printable like any other (§7): it is
    /// one expression of §2's grammar, and it holds no `After`.
    pub fn term(&self) -> &Term {
        &self.term
    }

    /// The compiled term, by value.
    pub fn into_term(self) -> Term {
        self.term
    }

    /// Every link the compiler resolved, in the order it met them (pre-order
    /// over the term).
    pub fn links(&self) -> &[Link] {
        &self.links
    }

    /// The links that priced as MOOT — the cancellations §7 says must be
    /// surfaced by reporting and never silently.
    pub fn moot(&self) -> impl Iterator<Item = &Link> {
        self.links
            .iter()
            .filter(|link| matches!(link, Link::Moot { .. }))
    }

    /// What the compiled term is worth at an instant. §7's ONE evaluator,
    /// against the empty environment — the argument is absent because there is
    /// nothing left in the term that could read it.
    pub fn fulfillment(&self, now: Instant) -> f64 {
        fpl::fulfillment(&self.term, now, &Env::new())
    }

    /// §7's explanation of the compiled term: the same decoration, over the
    /// shape the compiler produced, so the reader still sees a tree. The chain
    /// notes are merged into the ROOT — a `Piecewise` whose piece reads 1.0
    /// because an upstream was cancelled looks exactly like one that reads 1.0
    /// because its demand was met, and `moot` is what distinguishes them.
    pub fn explain(&self, now: Instant) -> Explanation {
        let mut explanation = fpl::explained(&self.term, now, &Env::new());
        explanation.notes.extend(self.notes());
        explanation
    }

    /// The chain as notes, in §7's own `Note` vocabulary: `links`, one map per
    /// link, and `moot`, the cancelled upstreams by name, present only when
    /// there are some. Empty for a term that held no `After` at all.
    pub fn notes(&self) -> BTreeMap<String, Note> {
        let mut notes = BTreeMap::new();
        if self.links.is_empty() {
            return notes;
        }
        notes.insert(
            "links".to_owned(),
            Note::Maps(self.links.iter().map(Link::scalars).collect()),
        );
        let moot: Vec<Scalar> = self
            .moot()
            .map(|link| Scalar::Text(link.event().to_owned()))
            .collect();
        if !moot.is_empty() {
            notes.insert("moot".to_owned(), Note::Many(moot));
        }
        notes
    }
}

/// §7.1 — compile a term against the environment as of one instant.
///
/// Total: a term is a finite tree, so there is nothing here to refuse. The
/// refusal lives on [`compile_chain`], which is handed a graph.
///
/// The result is [`fpl::normalize`]d, so a chain of links is ONE schedule at
/// the root over the merged partition of their instants rather than three
/// freeze quantifiers the evaluator re-enters per sample.
pub fn compile(term: &Term, env: &Env) -> Compiled {
    let mut links = Vec::new();
    let resolved = resolve(term, env, &mut links);
    Compiled {
        term: fpl::normalize(&resolved),
        links,
    }
}

/// One term with its `After`s replaced by what the snapshot says, before the
/// normal form flattens the result.
///
/// The three cases are §7's `After` semantics read as a SCHEDULE instead of as
/// a lookup, which is sound because `bound` gates a binding on `o.at() <= now`
/// and a `Piecewise` piece at τ is in force for exactly `now >= τ`:
///
/// - absent: the pending branch at every instant;
/// - `Completed(τ)`: pending before τ, and from τ the body slid by the
///   slippage — and sliding `now` by a constant IS a `Shift`;
/// - `Cancelled(τ)`: pending before τ, and from τ a graded offset at δ = 1
///   over the body, which is the moot constant with the mooted demand still
///   visible under it.
fn resolve(term: &Term, env: &Env, links: &mut Vec<Link>) -> Term {
    let TermF::After {
        event,
        anchor,
        term: body,
        pending,
        needs,
    } = term.out()
    else {
        // Every other constructor reads `now` and its subterms and nothing
        // else, so compiling it is compiling its children.
        return Term::new(term.out().clone().map(|child| resolve(&child, env, links)));
    };
    let link = match env.get(event) {
        None => Link::Pending {
            event: event.clone(),
        },
        Some(Outcome::Completed(at)) => Link::Completed {
            event: event.clone(),
            at: *at,
            slip: *at - *anchor,
            needs: *needs,
            // §7.1: `needs` is CONSUMED HERE and nowhere else. The compiler is
            // the first reader holding the declared lead time beside the
            // upstream's actual instant; the evaluator still never reads it.
            ready: needs.map(|needs| *at + needs),
        },
        Some(Outcome::Cancelled(at)) => Link::Moot {
            event: event.clone(),
            at: *at,
        },
    };
    links.push(link.clone());
    let pending = resolve(pending, env, links);
    match link {
        Link::Pending { .. } => pending,
        Link::Completed { at, slip, .. } => {
            let body = resolve(body, env, links);
            let from = if slip.is_zero() {
                // `Shift(0, x)` is `x`: the identity, so it is not written.
                body
            } else {
                mk_shift(-slip, body)
            };
            piecewise(pending, vec![(at, from)])
        }
        Link::Moot { at, .. } => {
            let body = resolve(body, env, links);
            piecewise(pending, vec![(at, mk_moot(body))])
        }
    }
}

/// `Shift`, where the argument is one the compiler computed. `mk_shift` refuses
/// nothing, so the `expect` is a proof and not a hope.
fn mk_shift(delta: Delta, term: Term) -> Term {
    fpl::mk_shift(delta, term).expect("Shift admits every span")
}

/// The MOOT CONSTANT as the graded offset it is: `offset(x, 1) = x·0 + 1 = 1`
/// for every `x` in [0, 1], so the value is exactly 1.0 and the demand that
/// was dropped is still in the tree for a reader to see.
fn mk_moot(term: Term) -> Term {
    fpl::mk_offset(1.0, term).expect("1.0 is in [-1, 1]")
}

/// §7.1 — compile a whole chain: one [`Compiled`] per todo, or the cycle.
///
/// The map is todo to function — [`crate::fold::flatten`]'s answer, keyed by
/// the names the links use — and the links draw a graph over its keys. That
/// graph is checked FIRST, whole, because the compiler is the first reader
/// that holds it: a link is local, a loop is not.
pub fn compile_chain(
    functions: &BTreeMap<String, Term>,
    env: &Env,
) -> Result<BTreeMap<String, Compiled>, ChainError> {
    let order = chain_order(functions)?;
    Ok(order
        .into_iter()
        .map(|todo| {
            let compiled = compile(&functions[&todo], env);
            (todo, compiled)
        })
        .collect())
}

/// The chain in DEPENDENCY ORDER: every todo after the upstreams its links
/// name, ties broken by name so the answer is deterministic. A cycle is
/// refused with the path that closes it.
///
/// The order a host reports in, and the order a host that caches compiled
/// terms fills its cache in.
pub fn chain_order(functions: &BTreeMap<String, Term>) -> Result<Vec<String>, ChainError> {
    let edges = upstreams(functions);
    let mut order = Vec::with_capacity(functions.len());
    let mut settled: BTreeSet<String> = BTreeSet::new();
    let mut path: Vec<String> = Vec::new();
    for todo in functions.keys() {
        visit(todo, &edges, &mut settled, &mut path, &mut order)?;
    }
    Ok(order)
}

/// Per todo, the upstreams its links name that are TODOS OF THIS CHAIN. A link
/// onto something the map does not hold is not an edge: the snapshot answers
/// it, and it constrains no order.
fn upstreams(functions: &BTreeMap<String, Term>) -> BTreeMap<String, BTreeSet<String>> {
    functions
        .iter()
        .map(|(todo, term)| {
            let mut events = BTreeSet::new();
            events_of(term, &mut events);
            events.retain(|event| functions.contains_key(event));
            (todo.clone(), events)
        })
        .collect()
}

/// Every event name an `After` in this term binds.
fn events_of(term: &Term, out: &mut BTreeSet<String>) {
    if let TermF::After { event, .. } = term.out() {
        out.insert(event.clone());
    }
    for child in term.out().children() {
        events_of(child, out);
    }
}

/// Depth-first post-order, with the ACTIVE PATH as the cycle witness: meeting a
/// todo that is still on the path is the loop, and the path from it is the
/// answer the host is told.
fn visit(
    todo: &str,
    edges: &BTreeMap<String, BTreeSet<String>>,
    settled: &mut BTreeSet<String>,
    path: &mut Vec<String>,
    order: &mut Vec<String>,
) -> Result<(), ChainError> {
    if settled.contains(todo) {
        return Ok(());
    }
    if let Some(from) = path.iter().position(|node| node == todo) {
        let mut loop_ = path[from..].to_vec();
        loop_.push(todo.to_owned());
        return Err(ChainError::Cycle(loop_));
    }
    path.push(todo.to_owned());
    for upstream in edges.get(todo).into_iter().flatten() {
        visit(upstream, edges, settled, path, order)?;
    }
    path.pop();
    settled.insert(todo.to_owned());
    order.push(todo.to_owned());
    Ok(())
}
