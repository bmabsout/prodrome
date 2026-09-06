//! §2: "Anything else is a refusal (`ValueError`-shaped), never a crash:
//! parsing is fuzzed." The reference's own guard is `tests/test_fuzz.py`; this
//! is the same property in Rust, where "never a crash" also means never a
//! stack overflow — a deep nest is a refusal, not an abort (`MAX_DEPTH`).

use prodrome::literal::{parse_literal, print_literal, Open, Table};
use proptest::prelude::*;

/// A vocabulary shaped like a real one, so the fuzzer reaches the call arms.
static VOCABULARY: Table = Table(&[
    ("Sealed", &["prev", "event"]),
    ("Woven", &["parents", "event"]),
    ("Note", &["on", "lines"]),
]);

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    #[test]
    fn arbitrary_text_never_panics(text in ".{0,200}") {
        let _ = parse_literal(&text, &VOCABULARY);
        let _ = parse_literal(&text, &Open);
    }

    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..200)) {
        if let Ok(text) = std::str::from_utf8(&bytes) {
            let _ = parse_literal(text, &VOCABULARY);
        }
    }

    /// Whatever survives the parse must print back to something the parser
    /// accepts, and the second print must equal the first — the stability that
    /// lets `seal_hash` hash a print (`literals.py`'s module docstring).
    #[test]
    fn a_parse_that_succeeds_prints_stably(text in ".{0,200}") {
        if let Ok(value) = parse_literal(&text, &VOCABULARY) {
            let once = print_literal(&value);
            let again = parse_literal(&once, &VOCABULARY).expect("a canonical print reparses");
            prop_assert_eq!(once, print_literal(&again));
        }
    }
}

#[test]
fn a_deep_nest_is_a_refusal_and_not_a_stack_overflow() {
    let deep = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
    assert!(parse_literal(&deep, &Open).is_err());
    let deep = format!("Note(on={}1{})", "(".repeat(10_000), ")".repeat(10_000));
    assert!(parse_literal(&deep, &VOCABULARY).is_err());
}
