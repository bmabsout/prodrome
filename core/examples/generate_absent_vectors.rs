//! Regenerates `conformance/absent/*.py` — SPEC §9.18's vectors for `Absent`,
//! frozen. Run BY HAND (`cargo run --example generate_absent_vectors -p
//! prodrome-core`); nothing under `cargo test` and nothing CI runs calls this,
//! which is what makes the three files FROZEN evidence rather than a cache the
//! suites could quietly refill. Seeded the way `generate_view_vectors.rs`
//! seeds `conformance/view/*.py`: the crate's own generators under a fixed
//! seed, not the reference's, because the reference never had an `Absent`.
//!
//! THE GENERATORS ARE NOT SECOND. The terms and environments come from
//! `tests/common/terms.rs` — `a_term`, `an_exact_term`, `an_env`, the ones
//! `tests/fpl_laws.rs`'s properties draw from — and the logs from
//! `tests/common/mod.rs`'s `a_log_with_absence`, the one `tests/fold_laws.rs`'s
//! §9.18 properties draw from. This file's only new code is running them
//! under a fixed seed, keeping the draws that hold an `Absent`, and asking the
//! core about them.
//!
//! THREE FILES, in the shapes the suites already replay:
//!
//! - `fpl.py`: `Fpl`, like `conformance/fpl.py` — each term's print, its
//!   normal form, six readings (`None` for `∅`) and its explanation at one
//!   instant, against an environment of outcomes;
//! - `series.py`: `Series`, like `conformance/series.py` — exact terms over a
//!   window, and their knots, `∅` where the term has no value;
//! - `view.py`: `View`, like `conformance/view/*.py` — logs whose specs are
//!   `Absent`, references to todos with no function, to todos never seen and
//!   to each other, folded at several instants.

#[path = "../tests/common/mod.rs"]
mod common;

use std::fs;
use std::path::PathBuf;

use common::terms::{a_term, an_env, an_exact_term, hours, moment};
use prodrome::breaks::series_knots;
use prodrome::event::{canonical, Actor};
use prodrome::fpl::{
    datetime_of, explained, fulfillment, holds_absent, normalize, print_term, Closed, Env, Outcome,
    Term,
};
use prodrome::literal::{print_literal, Value};
use prodrome::policy::Untrusted;
use prodrome::view;
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

/// However many times this runs, the same 32 bytes.
const SEED: [u8; 32] = *b"the-prodrome-absent-vectors-seed";

const TERMS: usize = 40;
const SERIES: usize = 30;
const LOGS: usize = 30;
/// Six readings per term, as `conformance/fpl.py` has.
const SAMPLES: usize = 6;
/// Three drawn instants per log plus `common::far()`, as the view vectors ask.
const RANDOM_INSTANTS: usize = 3;

fn runner() -> TestRunner {
    let rng = TestRng::from_seed(RngAlgorithm::ChaCha, &SEED);
    TestRunner::new_with_rng(Config::default(), rng)
}

fn draw<S: Strategy>(run: &mut TestRunner, strategy: &S) -> S::Value {
    strategy
        .new_tree(run)
        .expect("a strategy with no filters never fails to produce a tree")
        .current()
}

/// The next draw that holds an `Absent`: a vector for `∅` that never met one
/// would be evidence of nothing.
fn draw_absent<S: Strategy<Value = Term>>(run: &mut TestRunner, strategy: &S) -> Term {
    loop {
        let term = draw(run, strategy);
        if holds_absent(&term) {
            return term;
        }
    }
}

/// A vector value: one call, its fields in declared order.
fn call(name: &str, fields: Vec<(&str, Value)>) -> Value {
    Value::call(
        name,
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn instant(at: prodrome::fpl::Instant) -> Value {
    Value::Datetime(datetime_of(at).expect("inside the grammar's years"))
}

fn reading(value: Option<f64>) -> Value {
    value.map_or(Value::None, |v| {
        Value::float(v).expect("a fulfillment is finite")
    })
}

/// The environment's outcomes, which are all an `FplCase` carries: its
/// tendings are dropped before anything is read, so the case is read against
/// exactly the environment it records.
fn outcomes_only(env: Env) -> Env {
    Env {
        outcomes: env.outcomes,
        ..Env::new()
    }
}

fn bounds(env: &Env) -> Value {
    Value::Tuple(
        env.outcomes
            .iter()
            .map(|(todo, outcome)| {
                let kind = match outcome {
                    Outcome::Completed(_) => "Completed",
                    Outcome::Cancelled(_) => "Cancelled",
                };
                call(
                    "Bound",
                    vec![
                        ("todo", Value::Str(todo.clone())),
                        ("kind", Value::Str(kind.to_owned())),
                        ("at", instant(outcome.at())),
                    ],
                )
            })
            .collect(),
    )
}

fn fpl_cases(run: &mut TestRunner) -> Value {
    let (terms, envs) = (a_term(), an_env());
    let instants = proptest::collection::vec(hours(), SAMPLES + 1);
    let cases = (0..TERMS)
        .map(|seed| {
            let term = draw_absent(run, &terms);
            let env = outcomes_only(draw(run, &envs));
            let mut at: Vec<i64> = draw(run, &instants);
            let explain_at = moment(at.pop().expect("one more than the samples"));
            let closed = Closed::of(term.clone()).expect("a_term() builds no Ref");
            call(
                "FplCase",
                vec![
                    ("seed", Value::int(seed as i64)),
                    ("term", Value::Str(print_term(&term))),
                    ("normalized", Value::Str(print_term(&normalize(&term)))),
                    ("env", bounds(&env)),
                    (
                        "samples",
                        Value::Tuple(
                            at.into_iter()
                                .map(|h| {
                                    let now = moment(h);
                                    call(
                                        "Sample",
                                        vec![
                                            ("now", instant(now)),
                                            ("value", reading(fulfillment(&closed, now, &env))),
                                        ],
                                    )
                                })
                                .collect(),
                        ),
                    ),
                    ("explain_at", instant(explain_at)),
                    (
                        "explain",
                        common::explanation_value(&explained(&closed, explain_at, &env)),
                    ),
                ],
            )
        })
        .collect();
    call("Fpl", vec![("terms", Value::Tuple(cases))])
}

fn series_cases(run: &mut TestRunner) -> Value {
    let (terms, from, span) = (an_exact_term(), -500i64..0, 1i64..500);
    let cases = (0..SERIES)
        .map(|seed| {
            let term = draw_absent(run, &terms);
            let from = draw(run, &from);
            let (from, to) = (moment(from), moment(from + draw(run, &span)));
            let closed = Closed::of(term.clone()).expect("an_exact_term() builds no Ref");
            let series = series_knots(&closed, from, to, |_| Env::new());
            call(
                "SeriesCase",
                vec![
                    ("seed", Value::int(seed as i64)),
                    ("term", Value::Str(print_term(&term))),
                    ("from", instant(from)),
                    ("to", instant(to)),
                    ("exact", Value::Bool(series.exact)),
                    (
                        "knots",
                        Value::Tuple(
                            series
                                .knots
                                .iter()
                                .map(|knot| {
                                    call(
                                        "Knot",
                                        vec![
                                            ("at", instant(knot.at)),
                                            ("value", reading(knot.value)),
                                        ],
                                    )
                                })
                                .collect(),
                        ),
                    ),
                ],
            )
        })
        .collect();
    call("Series", vec![("series", Value::Tuple(cases))])
}

/// The view vectors' shape, under the reference roster of `"triage"`.
fn view_cases(run: &mut TestRunner) -> Value {
    let untrusted = Untrusted::of([Actor::new("triage").expect("valid")]);
    let logs = common::a_log_with_absence();
    let instants = proptest::collection::vec(0i64..common::WINDOW, RANDOM_INSTANTS);
    let cases = (0..LOGS)
        .map(|seed| {
            let log = draw(run, &logs);
            let seconds: Vec<i64> = draw(run, &instants);
            let nodes = common::chain_of(&log);
            let mut at: Vec<prodrome::literal::Datetime> =
                seconds.into_iter().map(common::moment).collect();
            at.push(common::far());
            call(
                "ViewCase",
                vec![
                    ("seed", Value::int(seed as i64)),
                    (
                        "events",
                        Value::Tuple(log.iter().map(|e| Value::Str(canonical(e))).collect()),
                    ),
                    (
                        "instants",
                        Value::Tuple(
                            at.into_iter()
                                .map(|at| {
                                    let rows = view::entries(&nodes, at, &untrusted)
                                        .expect("a chain always folds");
                                    call(
                                        "Asked",
                                        vec![
                                            ("at", Value::Datetime(at)),
                                            (
                                                "entries",
                                                Value::Tuple(
                                                    rows.iter().map(common::entry_value).collect(),
                                                ),
                                            ),
                                        ],
                                    )
                                })
                                .collect(),
                        ),
                    ),
                ],
            )
        })
        .collect();
    call(
        "View",
        vec![
            ("policy", Value::Str("triage-untrusted".to_owned())),
            (
                "untrusted",
                Value::Tuple(vec![Value::Str("triage".to_owned())]),
            ),
            ("cases", Value::Tuple(cases)),
        ],
    )
}

/// The file: a header comment, then the root with ONE CASE PER LINE — the
/// layout of every `conformance/*.py`.
fn lay_out(name: &str, root: &Value) -> String {
    let call = root.as_call().expect("the root is a call");
    let mut out = format!(
        "# conformance/absent/{name} — SPEC §9.18 vectors, SEEDED (not taken from the
# reference) and frozen: `examples/generate_absent_vectors.rs` is the only thing
# that writes this file, by hand. One case per line.\n"
    );
    out.push_str(&call.name);
    out.push('(');
    for (index, (key, value)) in call.fields.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(key);
        out.push('=');
        match value {
            Value::Tuple(cases) if index + 1 == call.fields.len() => {
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

fn main() {
    let dir: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "conformance", "absent"]
        .iter()
        .collect();
    fs::create_dir_all(&dir).expect("conformance/absent/ is creatable");
    // One runner, in this order, so each file's draws are fixed by the seed.
    let mut run = runner();
    let files = [
        ("fpl.py", fpl_cases(&mut run)),
        ("series.py", series_cases(&mut run)),
        ("view.py", view_cases(&mut run)),
    ];
    for (file, root) in files {
        let text = lay_out(file, &root);
        fs::write(dir.join(file), &text).expect("conformance/absent/*.py is writable");
        println!("wrote conformance/absent/{file} ({} bytes)", text.len());
    }
}
