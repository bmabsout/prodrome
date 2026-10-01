//! The core tests' REVIEW schema (`core/tests/schemas/review.rs`), a schema
//! with no valuation that is not the todo's, instantiated through
//! [`crate::schema!`] exactly as a host instantiates its own, and read over a
//! small store: so the generic path runs in CI, and nothing todo-shaped in it
//! goes unnoticed.

use prodrome::change::mk_change;
use prodrome::event::{canonical_envelope, seal_hash, Envelope, Hash};
use prodrome::genesis::mk_genesis;
use serde_json::{json, Value};

use crate::exports::Json;

// The schema's whole surface is the core tests'; this reads part of it.
#[allow(dead_code)]
#[path = "../../core/tests/schemas/review.rs"]
mod schema;

use schema::{day, moved, opened, Phase, Review};

/// A review names its entity `pr`.
impl Json for Review {
    const KEY: &'static str = "pr";
}

crate::schema!(Reviews = Review);

/// One object as the exports are sent it.
fn sent(object: &Envelope<Review>) -> Value {
    json!({ "hash": seal_hash(object).as_str(), "text": canonical_envelope(object) })
}

/// A genesis, then on `pr-1` an opening, a move to review, and two moves
/// that each supersede it and not each other, a merge and a close: the one
/// real conflict of the review's order. On `pr-2`, an opening alone.
fn store() -> (Vec<Envelope<Review>>, Vec<Hash>) {
    let genesis = Envelope::Genesis(mk_genesis("reviews", &"0".repeat(32)).expect("a genesis"));
    let root = seal_hash(&genesis);
    let change = |deps: &[&Hash], event| {
        let deps = deps.iter().map(|dep| (*dep).clone()).collect();
        Envelope::Change(mk_change(root.clone(), deps, event).expect("a change"))
    };
    let open = change(&[], opened("pr-1", day(1), "ana", "a change"));
    let review = change(&[], moved("pr-1", day(2), "ana", Phase::Review));
    let under = seal_hash(&review);
    let merged = change(&[&under], moved("pr-1", day(3), "ana", Phase::Merged));
    let closed = change(&[&under], moved("pr-1", day(3), "bo", Phase::Closed));
    let other = change(&[], opened("pr-2", day(4), "bo", "another"));
    let objects = vec![genesis, open, review, merged, closed, other];
    let names = objects.iter().map(seal_hash).collect();
    (objects, names)
}

fn replica() -> (Reviews, Vec<Hash>) {
    let (objects, names) = store();
    let sent = Value::Array(objects.iter().map(sent).collect()).to_string();
    let replica = Reviews::new(&sent).unwrap_or_else(|_| panic!("the objects are read"));
    (replica, names)
}

fn answer(text: Result<String, wasm_bindgen::JsError>) -> Value {
    serde_json::from_str(&text.unwrap_or_else(|_| panic!("the export refused"))).expect("JSON")
}

/// `verify` at the review schema: a healthy store, every row named by its
/// review under `pr`, kinded by its constructor.
#[test]
fn a_review_store_verifies() {
    let (replica, names) = replica();
    let verified = answer(replica.verify());
    assert_eq!(verified["ok"], json!(true), "{verified}");
    assert_eq!(verified["genesis"], json!([names[0].as_str()]));
    let rows = verified["objects"].as_array().expect("rows");
    assert_eq!(rows.len(), 6);
    let merged = rows
        .iter()
        .find(|row| row["hash"] == names[3].as_str())
        .expect("the merge's row");
    assert_eq!(merged["pr"], "pr-1");
    assert_eq!(merged["kind"], "Moved");
    assert!(merged.get("todo").is_none(), "{merged}");
}

/// A refusal is a value the export throws, never a panic.
#[test]
fn a_set_that_is_not_objects_is_refused() {
    assert!(crate::exports::Replica::<Review>::of("{}").is_err());
    let todo = r#"[{"hash": "00", "text": "Created()"}]"#;
    let replica = crate::exports::Replica::<Review>::of(todo).expect("the shape is right");
    assert!(
        replica.read().is_err(),
        "a name that is no name reads nothing"
    );
}
