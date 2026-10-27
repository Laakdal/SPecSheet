# Third-party notices

The repository contains no third-party source files or assets. This page credits the work SPecSheet builds on.

## Compiled into the app

- **libdeflate** by Eric Biggers (MIT), in a GDeflate variant vendored by the [`gdeflate-sys`](https://crates.io/crates/gdeflate-sys) crate (MIT or Apache-2.0). It decompresses GDeflate-packed textures in `specsheet-archive`.
- **Rust crates.** About 380 crates, including [Bevy](https://bevyengine.org) and [egui](https://github.com/emilk/egui), each under its own license, almost all MIT or Apache-2.0. `Cargo.lock` lists them. A release that ships binaries must include their license texts.

## Ported or adapted

### CityHash64 v1.0.3

`crates/specsheet-archive/src/city.rs` ports Google's CityHash64, revision 1.0.3, which the game uses to key archive entries. A hash must match bit for bit, so the port follows the original algorithm and its constants.

> Copyright (c) 2011 Google, Inc.

> Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

> The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

### GisDeflate

`crates/specsheet-archive/src/gdeflate.rs` follows the GDeflate tile-stream header layout of sk-zk's GisDeflate. The tiles themselves are decoded by libdeflate.

> Copyright (c) 2024 skzk

> Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

> The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

## Used as references

These were consulted to cross-check format facts that our own tools had observed. The evidence in `research/evidence/` comes from our own tools.

- **TruckLib** (`TruckLib.HashFs`, `TruckLib.Models`, `TruckLib.Sii`) and **Extractor** by sk-zk, under **GPL-2.0**: HashFS, model and SII layouts. SPecSheet is MIT, so its code must not copy or translate code from these projects. Only facts such as field offsets and magic numbers may be taken from them. The model and texture code was rewritten clean-room from our own evidence (D13), and the archive reader follows.
- **GisDeflate** by sk-zk (MIT): the GDeflate tile-stream header.

## Euro Truck Simulator 2

Euro Truck Simulator 2 is a trademark of SCS Software. Its game files belong to SCS Software and are not part of this repository; see [README](README.md#license).
