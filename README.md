# prodrome

The core, in Rust: one evaluator for every host — the box (through a Python
binding), the browser and the phone (through WebAssembly). `SPEC.md` is the
contract; `conformance/` are the vectors the Python reference produced
(`scripts/conformance.py` in the parent repository regenerates them); the
laws in SPEC §9 are the tests.

    nix develop .#rust -c cargo test --manifest-path prodrome/Cargo.toml
