# Third-party notices

The repository contains no third-party source files or assets. This page credits the work SPecSheet builds on.

## Compiled into the app

- **libdeflate** by Eric Biggers (MIT), in a GDeflate variant vendored by the [`gdeflate-sys`](https://crates.io/crates/gdeflate-sys) crate (MIT or Apache-2.0). It decompresses GDeflate-packed textures in `specsheet-archive`.
- **Rust crates.** About 380 crates, including [Bevy](https://bevyengine.org) and [egui](https://github.com/emilk/egui), each under its own license, almost all MIT or Apache-2.0. `Cargo.lock` lists them. A release that ships binaries must include their license texts.

## Used as references

These were consulted to cross-check format facts that our own tools had observed. The evidence in `research/evidence/` comes from our own tools.

- **TruckLib** (`TruckLib.HashFs`, `TruckLib.Models`, `TruckLib.Sii`) and **Extractor** by sk-zk, under **GPL-2.0**: HashFS, model and SII layouts. SPecSheet is MIT, so its code must not copy or translate code from these projects. Only facts such as field offsets and magic numbers may be taken from them.
- **GisDeflate** by sk-zk (MIT): the GDeflate tile-stream header.

## Euro Truck Simulator 2

Euro Truck Simulator 2 is a trademark of SCS Software. Its game files belong to SCS Software and are not part of this repository; see [README](README.md#license).
