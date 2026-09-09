//! Write `conformance/*.py` from `conformance/*.json`, once, by hand.
//!
//! ```console
//! $ cargo run --example migrate_vectors -p prodrome-core
//! ```
//!
//! The translation and both of its checks are `tests/common/migration.rs`'s;
//! this is the only thing that WRITES, and nothing under `cargo test` or CI
//! runs it — the same rule `generate_view_vectors` follows, and for the same
//! reason: a vector file a test can rewrite is a cache, not evidence.
//!
//! `tests/migration.rs` runs the same procedure and compares with what is on
//! disk, so the translation stays checked for as long as both formats are in
//! the tree. Both files go with the JSON.

#[path = "../tests/common/migration.rs"]
mod migration;

use std::fs;

fn main() {
    for (name, forward) in migration::FILES {
        let (text, fixture) = migration::rendered(name, *forward);
        let out = migration::conformance(&format!("{name}.py"));
        fs::write(&out, &text).unwrap_or_else(|e| panic!("cannot write {}: {e}", out.display()));
        println!("wrote {} ({} bytes)", out.display(), text.len());
        if let Some(fixture) = fixture {
            let path = migration::fixture_path();
            fs::create_dir_all(path.parent().expect("a parent"))
                .expect("wasm/conformance/ is creatable");
            let text = serde_json::to_string_pretty(&fixture).expect("it serialises") + "\n";
            fs::write(&path, &text)
                .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
            println!("wrote {} ({} bytes)", path.display(), text.len());
        }
    }
}
