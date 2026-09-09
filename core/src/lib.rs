//! Prodrome — a temporal, content-addressed event database with
//! fulfillment-priority semantics.
//!
//! A store is a directory of plain files: one file per object, holding that
//! object's canonical print, named by the SHA-256 of those bytes. Objects name
//! their parents, so the history is a DAG; order is causal and the `at` a
//! writer stamped is data, read by the folds and never by the merge. A fold
//! turns the DAG into what is believed at an instant; a term of FPL turns a
//! todo into what it is worth as a function of time.
//!
//! The specification is `../SPEC.md` — it is the contract, and this crate is
//! the reference implementation of it. The vectors in `../conformance/` are
//! frozen evidence that an independent implementation once answered the same:
//! prints, hashes, linearisations, frontiers and verify findings exactly,
//! fulfillments to 1e-9. Types first (§1): every value is a closed algebraic
//! type, invariants live in smart constructors, and a record that exists is
//! valid.
//!
//! # Quick start
//!
//! ```
//! use std::collections::BTreeSet;
//!
//! use prodrome::event::{mk_created, mk_spec_revised, TodoId};
//! use prodrome::fold::{env_at, evaluation_env, flatten, Untrusted};
//! use prodrome::fpl::{delta_from_hours, fulfillment, instant_of, mk_decay};
//! use prodrome::literal::Datetime;
//! use prodrome::registers::nodes_of;
//! use prodrome::store::EventStore;
//! use prodrome::view::entries;
//!
//! # fn main() -> Result<(), prodrome::literal::ProdromeError> {
//! # let dir = std::env::temp_dir().join("prodrome-quick-start");
//! # let _ = std::fs::remove_dir_all(&dir);
//! // A store is a directory. The second argument is the deployment's trust
//! // policy (§5): the actors whose lifecycle events are claims, not bindings.
//! let store = EventStore::new(&dir, BTreeSet::new());
//!
//! // An event carries the instant its writer stamped on it. The store has no
//! // clock: `at` is data, and nothing here reads the machine's.
//! let at = Datetime::new(2026, 9, 8, 9, 0, 0, 0)?;
//! store.append(mk_created("todo-1", at, "bassel", "publish the crate", "")?, None)?;
//!
//! // A spec is an FPL term — what this todo is worth as a function of time.
//! let deadline = instant_of(Datetime::new(2026, 9, 15, 17, 0, 0, 0)?);
//! let spec = mk_decay(0.55, 0.05, deadline, delta_from_hours(72.0), None)?;
//! store.append(mk_spec_revised("todo-1", at, "bassel", spec, "")?, None)?;
//!
//! // Read the DAG back: every object rehashed on the way in, in causal order.
//! let objects = store.read_dag_named()?;
//! assert_eq!(objects.len(), 2);
//! assert!(store.verify().is_empty(), "no finding against this store");
//!
//! // Fold at an instant, and price what the fold believes.
//! let now = Datetime::new(2026, 9, 14, 9, 0, 0, 0)?;
//! let untrusted = Untrusted::none();
//! let events = store.events()?;
//! let env = env_at(&events, now, &untrusted);
//! let functions = flatten(&events, now, &untrusted)?;
//! let todo = TodoId::new("todo-1")?;
//! let value = fulfillment(&functions[&todo], instant_of(now), &evaluation_env(&env));
//! assert!((0.0..=1.0).contains(&value));
//!
//! // Or the whole composition at once: one row per todo the chain mentions,
//! // each with its outcome, its function, its price and its conflicts (§6.7).
//! let rows = entries(&nodes_of(&objects), now, &untrusted)?;
//! assert_eq!(rows.len(), 1);
//! assert_eq!(rows[0].state(), "open");
//! assert_eq!(rows[0].value(), Some(value));
//! # let _ = std::fs::remove_dir_all(&dir);
//! # Ok(())
//! # }
//! ```
//!
//! Layout, one module per layer of the spec:
//! - `literal`  — §2: the grammar, its printer (Python `repr` rules) and parser
//! - `payload`  — §4: the record kind's fields, as a type parameter
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
pub mod payload;
pub mod registers;
pub mod store;
pub mod view;
