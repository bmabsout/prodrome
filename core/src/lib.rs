//! Prodrome — the temporal-logic database under Suzatary, as one crate.
//!
//! The specification is `../SPEC.md`; the conformance vectors are
//! `../conformance/*.json`, generated from the Python reference. This crate
//! must reproduce them: prints, hashes, linearisations, frontiers and verify
//! findings exactly, fulfillments to 1e-9. Types first (§1): every value is a
//! closed algebraic type, invariants live in smart constructors, and a record
//! that exists is valid.
//!
//! Layout, one module per layer of the spec:
//! - `literal`  — §2: the grammar, its printer (Python `repr` rules) and parser
//! - `event`    — §4: the event kinds, envelopes, hashing (§3)
//! - `store`    — §3: objects on disk, heads, linearisation, verify, adopt/merge
//! - `fpl`      — §7: `TermF`, `Term`, evaluation, normal form, explain (Cofree)
//! - `fold`     — §6.1–6.5: causal folds, flatten, history
//! - `registers`— §6.6: frontiers over the DAG, the fold as a monoid action
//! - `breaks`   — §7 breakpoints and series knots
//! - `view`     — §6.7: the entry, the composition of the folds above

pub mod breaks;
pub mod event;
pub mod fold;
pub mod fpl;
pub mod literal;
pub mod registers;
pub mod store;
pub mod view;
