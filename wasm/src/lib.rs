//! Prodrome in the browser — the same core, on the reader's machine.
//!
//! The web app's claim has always been that its numbers are the server's,
//! computed once by one evaluator (SPEC §1). That left the browser doing two
//! semantic things of its own: rehashing the chain objects, and drawing
//! straight lines between knots. This crate takes the FIRST of those out of
//! TypeScript and makes the second checkable, by compiling `prodrome-core`
//! itself to WebAssembly. The browser now runs the core; it still never has a
//! second evaluator.
//!
//! THIN ON PURPOSE. Every function here is: parse the arguments, call one
//! thing in `prodrome`, print the answer. No arithmetic, no policy, and no
//! shape that `view.py` does not already put on the wire. Anything
//! that needs a decision belongs in the core, where the conformance vectors
//! can see it.
//!
//! Strings and JSON at the boundary (see `wire.rs` for why), so the whole
//! surface is:
//!
//! | function         | asks                                                    |
//! | ---------------- | ------------------------------------------------------- |
//! | `verify_objects` | §3: do these bytes hash to these names, form one DAG, and end at which tips |
//! | `lifecycle`      | §4: spell one lifecycle event, through its own `mk_*`    |
//! | `seal`           | §3: a legacy store's write onto these heads — the name and the bytes |
//! | `merge_object`   | §3: a legacy store's join of these heads — no event, no actor |
//! | `fold`           | §6.1–6.5: what does the chain believe at an instant      |
//! | `registers`      | §6.6: which registers have more than one live write      |
//! | `entries`        | §6.7: every todo as the folds see it, composed ONCE      |
//! | `fulfillment`    | §7: what is this term worth now                          |
//! | `explain`        | §7: what is that number made of                          |
//! | `series_knots`   | §7 knots: what is that term's curve over a window        |
//! | `link`           | §7.2: every ref in a term bound to the todo it names     |
//! | `term_json`      | §2 → §7: a stored term's print, as the JSON shape above  |
//!
//! `store` is file-backed and a browser has no `events/` directory, so the
//! objects arrive over `/api/chain` instead and are read as a
//! [`prodrome::dag::Dag`]: [`verify_objects`] is its findings, asked of a set
//! in memory.
//!
//! NO CLOCK, anywhere below. §1 forbids one in the core, and a browser's clock
//! is the least trustworthy in the system; every moment is an argument — the
//! `at` a replica seals included, which is why [`lifecycle`] takes one rather
//! than reading one.
//!
//! ⚠️ SINCE STAGE 3 THIS CRATE ALSO WRITES, and it is worth saying why that is
//! not a widening. `seal` and `merge_object` build an ENVELOPE and hand back
//! its name and its bytes; they touch no store, because a browser has none.
//! Every rule about what an object may be is still the core's `mk_*`
//! constructors, and the box rehashes and re-verifies everything it is given
//! anyway on the way in. What the browser gains is the ability to
//! name a value the same way the box would — which is the one thing a replica
//! cannot do without §2's printer, and the one thing a second printer in
//! TypeScript would have got subtly wrong.

pub mod exports;
// Some of the boundary's shapes are the reference module's alone.
#[cfg_attr(not(feature = "reference"), allow(dead_code))]
mod json;
#[cfg(feature = "reference")]
mod reference;
#[cfg(test)]
mod review;
#[cfg(all(test, feature = "reference"))]
mod snapshot;
#[cfg_attr(not(feature = "reference"), allow(dead_code))]
mod wire;

#[cfg(feature = "reference")]
pub use reference::*;

/// A schema's exports, in ONE module beside every other schema's: a JS class
/// named `$name` for the schema `$schema`, which must implement
/// [`exports::Json`], and where `priced` is said, also
/// [`prodrome::schema::Valuation`] and [`exports::PricedJson`]. The invoking
/// crate depends on `wasm-bindgen` at this
/// crate's pinned version, which is the version of the `wasm-bindgen` CLI it
/// builds its module with anyway.
///
/// ```text
/// prodrome_wasm::schema!(Reviews = my_host::Review);
/// prodrome_wasm::schema!(Todos = prodrome::event::TodoEvent<my_host::Record>, priced);
/// ```
///
/// A macro because `#[wasm_bindgen]` exports no generic item: each method is
/// one line, a call of the generic function in [`exports`] of the same name,
/// so a schema costs its module the monomorphised functions and nothing else.
///
/// `new Reviews(objects)` reads the objects once (`[{hash, text}]`, see
/// [`exports::Replica::of`]); every method asks its question of what was read.
/// A refusal is the JS exception, never a panic. Call `free()` when done, or
/// let the finaliser.
#[macro_export]
macro_rules! schema {
    ($name:ident = $schema:ty) => {
        #[doc = concat!("The exports of the schema `", stringify!($schema), "`.")]
        #[wasm_bindgen::prelude::wasm_bindgen]
        pub struct $name($crate::exports::Replica<$schema>);

        #[wasm_bindgen::prelude::wasm_bindgen]
        impl $name {
            /// The objects, `[{hash, text}]`, read once at this schema.
            #[wasm_bindgen(constructor)]
            pub fn new(objects: &str) -> Result<$name, wasm_bindgen::JsError> {
                $crate::exports::Replica::of(objects)
                    .map($name)
                    .map_err(|refusal| wasm_bindgen::JsError::new(&refusal))
            }

            /// §3: do these bytes hash to these names, form one DAG, and end
            /// at which tips. Every row names its entity under the schema's
            /// key.
            pub fn verify(&self) -> Result<String, wasm_bindgen::JsError> {
                $crate::exports::thrown($crate::exports::verify(&self.0))
            }

            /// §3: the objects' tips, and each prodrome's heads by its
            /// genesis.
            pub fn tips(&self) -> Result<String, wasm_bindgen::JsError> {
                $crate::exports::thrown($crate::exports::tips(&self.0))
            }

            /// §3: what a replica holding `tips` (a JSON array of names)
            /// lacks, in causal order: the names to send it.
            pub fn since(&self, tips: &str) -> Result<String, wasm_bindgen::JsError> {
                $crate::exports::thrown($crate::exports::since(&self.0, tips))
            }

            /// §6: every entity's registers, each its maximal writes, at
            /// `at` (ISO, or `null` for everything) under `untrusted`.
            pub fn readings(
                &self,
                at: Option<String>,
                untrusted: &str,
            ) -> Result<String, wasm_bindgen::JsError> {
                $crate::exports::thrown($crate::exports::readings(&self.0, at, untrusted))
            }
        }
    };
    ($name:ident = $schema:ty, priced) => {
        $crate::schema!($name = $schema);

        #[wasm_bindgen::prelude::wasm_bindgen]
        impl $name {
            /// §6.7: every entity, as the folds see it at `at` (ISO, or
            /// `null` for the latest instant the objects stamp) under
            /// `untrusted`, in list order.
            pub fn entries(
                &self,
                at: Option<String>,
                untrusted: &str,
            ) -> Result<String, wasm_bindgen::JsError> {
                $crate::exports::thrown($crate::exports::entries(&self.0, at, untrusted))
            }

            /// §6.1 and §6.4, per prodrome: the environment, and every
            /// entity's fulfillment function.
            pub fn prices(
                &self,
                at: Option<String>,
                untrusted: &str,
            ) -> Result<String, wasm_bindgen::JsError> {
                $crate::exports::thrown($crate::exports::prices(&self.0, at, untrusted))
            }
        }
    };
}
