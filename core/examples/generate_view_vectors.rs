//! Regenerates `conformance/view/*.py` — SPEC §6.7's `view::entries`,
//! frozen. Run BY HAND (`cargo run --example generate_view_vectors -p
//! prodrome-core`); nothing under `cargo test` and nothing CI runs calls
//! this, which is what makes the two files FROZEN evidence rather than a
//! cache `conformance_view.rs` could quietly refill. See
//! `core/tests/conformance_view.rs`'s module doc for the law this pins and
//! `README.md`/`SPEC.md` §6.7 for the same rule stated for a reader who never
//! opens this file.
//!
//! THE GENERATOR IS NOT SECOND. `common::a_log` is `tests/fold_laws.rs`'s own
//! random-log strategy (`tests/common/mod.rs`, moved there so this example
//! and that property test draw from the one place); this file's only new
//! code is running it under a FIXED seed instead of an ambient one, sealing
//! each draw into a chain (`common::chain_of`), and asking `view::entries`
//! about it — the same call `conformance_view.rs` replays.
//!
//! TWO POLICIES, not one, because §6.7's whole point is the asymmetry between
//! the two readings: `common::ACTORS` is two `"bassel"` writes for every
//! `"triage"` one, so a reference roster of `"triage"` (the crate's usual one,
//! `tests/fold_laws.rs`'s `roster()`) and one of `"bassel"` instead exercise
//! the SAME logs from opposite sides of that 2:1 split — the first is the
//! common case (a minority write is provisional), the second is the inverted
//! one (most of a log's writes are). `Untrusted::none()` would have been a
//! third file of nothing but `Confidence::Confirmed`.

#[path = "../tests/common/mod.rs"]
mod common;

use std::fs;
use std::path::PathBuf;

use prodrome::event::{canonical, Actor};
use prodrome::literal::{print_literal, Value};
use prodrome::policy::Untrusted;
use prodrome::view;
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

/// However many times this runs, the same 32 bytes: a fixed seed is the
/// whole reason to call this "frozen" rather than "cached".
const SEED: [u8; 32] = *b"the-prodrome-view-vectors-seed!!";

const LOGS: usize = 30;
/// Three drawn instants per log plus `common::far()` — the same shape
/// `tests/fold_laws.rs`'s laws ask for (`asked in
/// prop::collection::vec(0i64..WINDOW, 1..6)`) with one end pinned so every
/// log is also queried once with its whole history in view.
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

fn conformance_dir() -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "..", "conformance", "view"]
        .iter()
        .collect()
}

/// One file: every case, under one policy.
fn generate(name: &str, untrusted: &Untrusted) -> Value {
    let mut run = runner();
    let instants_strategy = proptest::collection::vec(0i64..common::WINDOW, RANDOM_INSTANTS);
    let mut cases = Vec::with_capacity(LOGS);
    for seed in 0..LOGS {
        let log = draw(&mut run, &common::a_log());
        let seconds: Vec<i64> = draw(&mut run, &instants_strategy);
        let nodes = common::chain_of(&log);
        // The log's own prints, in APPEND order — never a map keyed by name:
        // a JSON object has no order of its own (`serde_json`'s default `Map`
        // is a `BTreeMap`, sorted by key), and this chain's names are its
        // HASHES, which sort by nothing a reader would call "order". The
        // replay side reseals this exact array with the exact same
        // `common::chain_of`, so the names it gets — the ones `stream`,
        // `content` and `conflicts` carry — agree without either side
        // freezing them.
        let events = Value::Tuple(
            log.iter()
                .map(|event| Value::Str(canonical(event)))
                .collect(),
        );
        // The three drawn instants, plus `far()` so every log is also asked
        // about with its whole history in view.
        let mut instants: Vec<prodrome::literal::Datetime> =
            seconds.into_iter().map(common::moment).collect();
        instants.push(common::far());
        let instants: Vec<Value> = instants
            .into_iter()
            .map(|at| {
                let entries = view::entries(&nodes, at, untrusted).expect("a chain always folds");
                call(
                    "Asked",
                    vec![
                        ("at", Value::Datetime(at)),
                        (
                            "entries",
                            Value::Tuple(entries.iter().map(common::entry_value).collect()),
                        ),
                    ],
                )
            })
            .collect();
        cases.push(call(
            "ViewCase",
            vec![
                ("seed", Value::int(seed as i64)),
                ("events", events),
                ("instants", Value::Tuple(instants)),
            ],
        ));
    }
    call(
        "View",
        vec![
            ("policy", Value::Str(name.to_owned())),
            (
                "untrusted",
                Value::Tuple(
                    untrusted
                        .actors()
                        .iter()
                        .map(|actor| Value::Str(actor.as_str().to_owned()))
                        .collect(),
                ),
            ),
            ("cases", Value::Tuple(cases)),
        ],
    )
}

/// The file: a header comment, then the root with ONE CASE PER LINE, each line
/// the canonical print of that case's own value — the layout
/// `conformance/*.py` was migrated into, so a diff is readable and the printer
/// is still the only thing that writes a value.
fn lay_out(name: &str, root: &Value) -> String {
    let call = root.as_call().expect("the root is a call");
    let mut out = format!(
        "# conformance/view/{name} — SPEC §6.7 vectors, SEEDED (not taken from the
# reference) and frozen: `examples/generate_view_vectors.rs` is the only thing
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
            Value::Tuple(cases) if key == "cases" => {
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
    let dir = conformance_dir();
    fs::create_dir_all(&dir).expect("conformance/view/ is creatable");

    let triage = Untrusted::of([Actor::new("triage").expect("valid")]);
    let bassel = Untrusted::of([Actor::new("bassel").expect("valid")]);

    for (file, policy, untrusted) in [
        ("triage-untrusted.py", "triage-untrusted", &triage),
        ("bassel-untrusted.py", "bassel-untrusted", &bassel),
    ] {
        let text = lay_out(file, &generate(policy, untrusted));
        fs::write(dir.join(file), &text).expect("conformance/view/*.py is writable");
        println!("wrote conformance/view/{file} ({} bytes)", text.len());
    }
}
