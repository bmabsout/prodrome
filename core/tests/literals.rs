//! SPEC §9 law 1, on `conformance/literals.json`: `print ∘ parse` is the
//! identity on every stored object, and `sha256(print)` is its name.
//!
//! The objects are read through `literal::Open` — the vocabulary that admits
//! any constructor name in keyword form — because this file is testing §2 and
//! nothing else. The same 564 objects go through the CLOSED vocabulary of §4,
//! as typed events, in `tests/events.rs`.

use std::path::PathBuf;

use prodrome::literal::{parse_literal, print_literal, Open};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Vectors {
    objects: Vec<Object>,
    values: Vec<String>,
}

#[derive(Deserialize)]
struct Object {
    name: String,
    text: String,
}

fn vectors() -> Vectors {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "conformance",
        "literals.json",
    ]
    .iter()
    .collect();
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&raw).expect("literals.json is the generator's shape")
}

#[test]
fn every_value_vector_round_trips() {
    let vectors = vectors();
    assert!(!vectors.values.is_empty());
    for text in &vectors.values {
        let value = parse_literal(text, &Open).unwrap_or_else(|e| panic!("parse {text:?}: {e}"));
        assert_eq!(&print_literal(&value), text, "print(parse({text:?}))");
    }
}

#[test]
fn every_stored_object_round_trips_and_hashes_to_its_name() {
    let vectors = vectors();
    assert!(
        vectors.objects.len() > 500,
        "the live chain, not a fragment"
    );
    for object in &vectors.objects {
        let value = parse_literal(&object.text, &Open)
            .unwrap_or_else(|e| panic!("object {}: {e}", object.name));
        assert_eq!(print_literal(&value), object.text, "object {}", object.name);
        let digest = hex(Sha256::digest(object.text.as_bytes()).as_slice());
        assert_eq!(digest, object.name, "sha256 of the print names the object");
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
