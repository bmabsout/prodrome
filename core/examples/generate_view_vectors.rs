//! Regenerates `conformance/view/*.json` — SPEC §6.7's `view::entries`,
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
//! TWO TRUST POLICIES, not one, because §6.7's whole point is the asymmetry
//! between them: `common::ACTORS` is two `"bassel"` writes for every
//! `"triage"` one, so untrusting `"triage"` (the crate's usual policy,
//! `tests/fold_laws.rs`'s `untrusted()`) and untrusting `"bassel"` instead
//! exercise the SAME logs from opposite sides of that 2:1 split — the first
//! is the common case (a minority write is provisional), the second is the
//! inverted one (most of a log's writes are). `Untrusted::none()` would have
//! been a third file of nothing but `Standing::Confirmed`.

#[path = "../tests/common/mod.rs"]
mod common;

use std::fs;
use std::path::PathBuf;

use prodrome::event::{canonical, Actor};
use prodrome::fold::Untrusted;
use prodrome::view;
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
use serde_json::{json, Value};

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

fn conformance_dir() -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "..", "conformance", "view"]
        .iter()
        .collect()
}

/// One file: every case, under one trust policy.
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
        let events: Value = Value::Array(
            log.iter()
                .map(|event| Value::String(canonical(event)))
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
                json!({
                    "at": prodrome::fpl::iso(prodrome::fpl::instant_of(at)).replace('T', " "),
                    "entries": entries.iter().map(common::json_entry).collect::<Vec<_>>(),
                })
            })
            .collect();
        cases.push(json!({
            "seed": seed,
            "events": events,
            "instants": instants,
        }));
    }
    json!({
        "policy": name,
        "untrusted": untrusted.actors().iter().map(|actor| actor.as_str().to_owned()).collect::<Vec<_>>(),
        "cases": cases,
    })
}

fn main() {
    let dir = conformance_dir();
    fs::create_dir_all(&dir).expect("conformance/view/ is creatable");

    let triage = Untrusted::of([Actor::new("triage").expect("valid")]);
    let bassel = Untrusted::of([Actor::new("bassel").expect("valid")]);

    for (file, policy, untrusted) in [
        ("triage-untrusted.json", "triage-untrusted", &triage),
        ("bassel-untrusted.json", "bassel-untrusted", &bassel),
    ] {
        let doc = generate(policy, untrusted);
        let text = serde_json::to_string_pretty(&doc).expect("the doc serialises");
        fs::write(dir.join(file), text + "\n").expect("conformance/view/*.json is writable");
        println!("wrote conformance/view/{file}");
    }
}
