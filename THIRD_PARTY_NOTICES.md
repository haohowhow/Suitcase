# Third-party notices

Koofr-GUI is distributed with the following preview components. Versions are
locked by `package-lock.json`, `Cargo.lock`, or the vendored source directory.
The full license files shipped by each dependency remain authoritative.

| Component | Fixed version | Distribution | License used |
| --- | --- | --- | --- |
| PDF.js / `pdfjs-dist` | 6.1.200 | Bundled JavaScript, worker, CMaps, standard fonts, and image decoders | Apache-2.0 |
| DOMPurify | 3.4.12 | Bundled JavaScript | Apache-2.0 (dependency also offers MPL-2.0) |
| `heic-to` | 1.5.2 | Bundled JavaScript / WASM worker | LGPL-3.0 |
| LibRaw | 0.21.1 | Vendored source, statically built | CDDL-1.0 |
| `chardetng` | 0.1.17 | Statically linked Rust crate | Apache-2.0 OR MIT |
| `encoding_rs` | 0.8.35 | Statically linked Rust crate | Apache-2.0 OR MIT |
| `image` | 0.25.10 | Statically linked Rust crate | MIT OR Apache-2.0 |

LibRaw source, `COPYRIGHT`, `LICENSE.CDDL`, and provenance are stored in
`src-tauri/third_party/libraw-0.21.1/`. Koofr-GUI selects LibRaw's CDDL-1.0
option. Source code for npm and Cargo dependencies is available from their
respective package registries using the exact committed lock files.
