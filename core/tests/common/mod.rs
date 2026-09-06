//! Shared plumbing for the conformance suites (SPEC §9): loading a vector
//! file, and comparing two JSON trees the way the spec compares them — shape
//! and strings exactly, floats to 1e-9.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

/// The largest float deviation any comparison has seen, so a suite can report
/// its margin rather than only its verdict.
pub static WORST: AtomicU64 = AtomicU64::new(0);

pub fn tolerance() -> f64 {
    1e-9
}

pub fn worst_seen() -> f64 {
    f64::from_bits(WORST.load(Ordering::Relaxed))
}

fn record(delta: f64) {
    let mut current = WORST.load(Ordering::Relaxed);
    while delta > f64::from_bits(current) {
        match WORST.compare_exchange_weak(
            current,
            delta.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

pub fn vectors(name: &str) -> Value {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "conformance", name]
        .iter()
        .collect();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()))
}

/// `mine` against `theirs`, in the spec's terms. Returns the first
/// disagreement as a path plus a message.
pub fn agrees(path: &str, mine: &Value, theirs: &Value) -> Result<(), String> {
    match (mine, theirs) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (
                a.as_f64().unwrap_or(f64::NAN),
                b.as_f64().unwrap_or(f64::NAN),
            );
            let delta = (a - b).abs();
            record(delta);
            if delta <= tolerance() * a.abs().max(b.abs()).max(1.0) {
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
            let mut mine_keys: Vec<&String> = a.keys().collect();
            let mut theirs_keys: Vec<&String> = b.keys().collect();
            mine_keys.sort();
            theirs_keys.sort();
            if mine_keys != theirs_keys {
                return Err(format!("{path}: keys {mine_keys:?} != {theirs_keys:?}"));
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
