//! Research tools, one per file under `examples/`: surveys, dumps and probes
//! over the user's ETS2 install (`docs/research-lab.md`). Each tool's first
//! lines say what it measures and how to run it, e.g.
//! `ETS2_DIR=<game dir> cargo run --release -p specsheet-research --example scs -- ls /def`.
//! Their output, when cited, is copied to `research/evidence/`.
//!
//! The library holds only what the tools share: the evidence header and CSV
//! output ([`evidence`]). The crate exists so the tools live in one place
//! above every library crate (D17).

pub mod evidence;
