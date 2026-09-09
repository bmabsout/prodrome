//! THE MIGRATION, CHECKED: `conformance/*.py` says exactly what
//! `conformance/*.json` said.
//!
//! The vectors are frozen evidence with no generator left to re-run, so moving
//! them into this crate's own grammar (SPEC §2) had to be a translation that is
//! PROVED, not a refresh. `tests/common/migration.rs` is the translation; this
//! runs it and compares with what is on disk, so the claim is checked by
//! `cargo test` and not only by the commit message.
//!
//! It goes when the JSON goes, in the commit that switches the suites over to
//! the literals — its whole subject is that both formats are present and agree.

mod common;

#[path = "common/migration.rs"]
mod migration;

use std::fs;

#[test]
fn every_vector_file_says_in_literals_what_it_said_in_json() {
    let mut files = 0;
    for (name, forward) in migration::FILES {
        // `rendered` asserts the two halves of the translation itself: the
        // print round-trips through the parser, and the literal (plus what
        // left for the JSON boundary) is the JSON, exactly.
        let (text, fixture) = migration::rendered(name, *forward);

        // And what is on disk is what it produced — byte for byte, so the
        // committed file is the printer's own output and not a hand edit.
        let on_disk = migration::conformance(&format!("{name}.py"));
        let held = fs::read_to_string(&on_disk)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", on_disk.display()));
        assert_eq!(held, text, "{name}.py is not what the translation writes");

        if let Some(fixture) = fixture {
            let path = migration::fixture_path();
            let held = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            let held: serde_json::Value = serde_json::from_str(&held).expect("the fixture is JSON");
            assert_eq!(
                held,
                fixture,
                "{} is not what fpl.json held",
                path.display()
            );
        }
        files += 1;
    }
    assert_eq!(files, 6, "every vector file is translated");
}
