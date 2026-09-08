//! SPEC §9 law 1: `print ∘ parse` is the identity on every stored object, and
//! `sha256(print)` is its name.
//!
//! The vectors are AUTHORED HERE from SPEC §2 — one per production of the
//! grammar, plus the cases that make a printer wrong: a float that needs
//! exponent form, a string carrying both quote characters, an unprintable code
//! point beside a printable non-ASCII one, a one-element tuple, a `datetime`
//! whose seventh argument is present and one whose is not. Beside them the
//! synthetic corpus of `common::corpus()`, read here through `literal::Open` —
//! the vocabulary that admits any constructor name in keyword form — because
//! this file is testing §2 and nothing else. The same objects go through the
//! CLOSED vocabulary of §4, as typed events, in `tests/events.rs`.
//!
//! The refusals are the other half of the law: §2 says parsing admits exactly
//! this grammar, so what a stranger needs to see is the shape of what it turns
//! down. They are refused BY CONSTRUCTION — this is a closed-vocabulary
//! grammar and not a walker over somebody else's AST, so there is no
//! `__import__` to reach and no attribute to walk — and pinned anyway, because
//! a reader deciding whether the parser is a security boundary should be able
//! to read the attacks it was written against.

mod common;

use prodrome::event::{canonical_envelope, seal_hash};
use prodrome::literal::{parse_literal, print_literal, Open};
use sha2::{Digest, Sha256};

/// Canonical prints: text that parses and prints back byte for byte.
const CANONICAL: &[&str] = &[
    // §2's atoms.
    "None",
    "True",
    "False",
    // Integers: decimal, optional leading `-`, arbitrary width.
    "0",
    "7",
    "-7",
    "12345678901234567890",
    "-12345678901234567890",
    // Floats: CPython `repr` — shortest round-tripping, `.0` on integral
    // values, exponent form below 1e-4 and at or above 1e16.
    "1.0",
    "-1.0",
    "-0.0",
    "0.1",
    "0.5",
    "0.001",
    "0.0001",
    "1e-05",
    "2.5e-07",
    "1e+16",
    "1e+22",
    "1000000000000000.0",
    "123456789012345.6",
    "0.30000000000000004",
    "-1.5",
    // Strings: single quotes unless the text holds `'` and no `"`.
    "'plain'",
    "''",
    "\"it's\"",
    "'say \"hi\"'",
    "'both \\' and \"'",
    "'tab\\there'",
    "'new\\nline'",
    "'carriage\\rreturn'",
    "'back\\\\slash'",
    "'\\x00\\x07\\x1f'",
    // Printable non-ASCII is written as itself; a Cf code point is escaped,
    // because CPython's `str.isprintable` says so.
    "'é ü ✓ 日本'",
    "'emoji 🙂'",
    "'\\u200b zero width'",
    // A printable astral character is itself; an unprintable one is `\U`.
    "'𝄞 clef'",
    "'\\U0001d173 beam'",
    // Datetimes: the seventh argument only when microseconds are non-zero.
    "datetime(2026, 1, 1, 0, 0, 0)",
    "datetime(2026, 9, 6, 7, 3, 0, 250)",
    "datetime(1, 1, 1, 0, 0, 0)",
    "datetime(9999, 12, 31, 23, 59, 59, 999999)",
    // Timedeltas: the non-zero parts only, in declared order.
    "timedelta()",
    "timedelta(days=1)",
    "timedelta(seconds=10800)",
    "timedelta(microseconds=1)",
    "timedelta(days=-1, seconds=86399)",
    "timedelta(days=2, seconds=30, microseconds=5)",
    // Tuples: the one-element comma, the empty display, nesting.
    "()",
    "(1,)",
    "(1, 'a')",
    "((1, 2), (3,))",
    "(None, True, False)",
    // Constructor calls: every field, in declared order, keyword form.
    "Note(on='block', lines=('a line',))",
    "SubTodo(body='draft it', done=True)",
    "Flat(value=0.5)",
    "Conj(terms=(Flat(value=0.5), Flat(value=0.25)), p=-4.0)",
    "Sealed(prev='', event=None)",
];

/// Text the grammar accepts and prints DIFFERENTLY: the same value has one
/// print, so a non-canonical spelling normalises to the canonical one.
///
/// Field ORDER is not here: `Open` does not know a declared order, so a call
/// keeps the keywords it was handed. Normalising those is a property of a real
/// vocabulary, and `literal`'s own unit tests state it over one.
const NORMALISED: &[(&str, &str)] = &[
    ("(1)", "1"),
    ("[1, 2]", "(1, 2)"),
    ("[]", "()"),
    ("-0", "0"),
    ("timedelta(hours=3)", "timedelta(seconds=10800)"),
    ("timedelta(seconds=-1)", "timedelta(days=-1, seconds=86399)"),
    ("timedelta(weeks=1)", "timedelta(days=7)"),
    (
        "datetime(2026, 1, 1, 0, 0, 0, 0)",
        "datetime(2026, 1, 1, 0, 0, 0)",
    ),
    ("# a note\n\n  None  \n", "None"),
];

/// What §2 does NOT admit. Every one of these is a refusal — never a crash,
/// which `tests/fuzz.rs` states as a property over arbitrary text.
const REFUSED: &[&str] = &[
    // Not expressions of the grammar at all.
    "",
    "x",
    "None None",
    "1 + 1",
    "-'a'",
    "{1: 2}",
    "{1, 2}",
    "[x for x in y]",
    "lambda: 1",
    "b'bytes'",
    "1j",
    "0x10",
    "1_000",
    "'unterminated",
    "(1, 2",
    "None if True else None",
    "(x := 1)",
    "f'{1}'",
    // §2: `inf`/`nan` are not storable, and there is no name to write them.
    "inf",
    "nan",
    "float('nan')",
    "1e400",
    // Calls: `Open` admits any NAME, and nothing else about a call. A
    // positional argument has no field order to bind to, `*`/`**` are not the
    // grammar, and an attribute is not a value.
    "Pair(1, 2)",
    "Pair(*x)",
    "Pair(**{'left': 1})",
    "Pair(left=1, left=2)",
    "Pair(left=1).left",
    "Pair(1, 2).__class__",
    "Pair(1, 2).__class__.__bases__",
    "__import__('os').system('id')",
    "open('/etc/passwd')",
    "eval('1')",
    "Pair(left=1); Pair(left=2)",
    // Out-of-range records: the smart constructor's error IS the parse error.
    "datetime(2026, 2, 30, 0, 0, 0)",
    "datetime(2026, 13, 1, 0, 0, 0)",
    "datetime(2026, 1, 1, 24, 0, 0)",
    "timedelta(fortnights=1)",
];

#[test]
fn every_canonical_vector_round_trips() {
    for text in CANONICAL {
        let value = parse_literal(text, &Open).unwrap_or_else(|e| panic!("parse {text:?}: {e}"));
        assert_eq!(&print_literal(&value), text, "print(parse({text:?}))");
    }
}

#[test]
fn a_value_has_exactly_one_print() {
    for (text, canonical) in NORMALISED {
        let value = parse_literal(text, &Open).unwrap_or_else(|e| panic!("parse {text:?}: {e}"));
        assert_eq!(&print_literal(&value), canonical, "print(parse({text:?}))");
        // And the canonical spelling is a fixed point of the same route.
        let again = parse_literal(canonical, &Open).expect("the canonical print parses");
        assert_eq!(&print_literal(&again), canonical);
    }
}

#[test]
fn the_grammar_refuses_everything_else() {
    for text in REFUSED {
        assert!(
            parse_literal(text, &Open).is_err(),
            "§2 must refuse {text:?}"
        );
    }
}

#[test]
fn every_stored_object_round_trips_and_hashes_to_its_name() {
    let corpus = common::corpus();
    assert!(corpus.len() > 5, "a corpus, not a single object");
    for (name, envelope) in &corpus {
        let text = canonical_envelope(envelope);
        let value =
            parse_literal(&text, &Open).unwrap_or_else(|e| panic!("object {}: {e}", name.as_str()));
        assert_eq!(print_literal(&value), text, "object {}", name.as_str());
        let digest = hex(Sha256::digest(text.as_bytes()).as_slice());
        assert_eq!(
            digest,
            name.as_str(),
            "sha256 of the print names the object"
        );
        assert_eq!(seal_hash(envelope).as_str(), name.as_str());
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
