# Third-party notices

This document lists the key third-party components distributed with **Suitcase** (Unofficial Windows Client for Koofr), providing version, distribution, and licensing details for compliance. Versions are locked by `package-lock.json`, `Cargo.lock`, or the vendored source directory. The full license texts provided by each dependency remain the authoritative legal terms.

Full copies of key upstream license texts are bundled in the `licenses/` directory of this repository and the Windows installer:

- `licenses/heic-to-LGPL-3.0.txt` — GNU Lesser General Public License v3.0 (for `heic-to`)
- `licenses/GPL-3.0.txt` — GNU General Public License v3.0 (incorporated by reference into LGPL-3.0)
- `licenses/Apache-2.0.txt` — Apache License 2.0 (for `pdfjs-dist`, `dompurify`, and Apache-2.0 Rust/npm dependencies)
- `licenses/vault-crypto-MIT.txt` — MIT License (for `vault-crypto` from `koofr/vault`)

## Bundled & Notable Components

| Component | Fixed version / ref | Distribution | License used |
| --- | --- | --- | --- |
| PDF.js / `pdfjs-dist` | 6.1.200 | Bundled JavaScript, worker, CMaps, standard fonts, and image decoders | Apache-2.0 |
| DOMPurify | 3.4.12 | Bundled JavaScript | Apache-2.0 (dependency also offers MPL-2.0) |
| `heic-to` | 1.5.2 | Bundled JavaScript / WASM worker | LGPL-3.0 |
| LibRaw | 0.21.1 | Vendored source, statically built | CDDL-1.0 |
| `vault-crypto` (`koofr/vault`) | `17c95914` | Statically linked Rust crate | MIT |
| `chardetng` | 0.1.17 | Statically linked Rust crate | Apache-2.0 OR MIT |
| `encoding_rs` | 0.8.35 | Statically linked Rust crate | (Apache-2.0 OR MIT) AND BSD-3-Clause |
| `image` | 0.25.9 | Statically linked Rust crate | MIT OR Apache-2.0 |
| `cssparser`, `selectors`, `option-ext`, `dtoa-short` | Locked in `Cargo.lock` | Unmodified statically linked Rust crates | MPL-2.0 |

## License-Specific Compliance Notes

- **LibRaw (CDDL-1.0)**: Source, `COPYRIGHT`, `LICENSE.CDDL`, and provenance are stored in `src-tauri/third_party/libraw-0.21.1/` and distributed alongside the installer. Suitcase selects LibRaw's CDDL-1.0 option.
- **`heic-to` (LGPL-3.0)**: Distributed under LGPL-3.0 (incorporating GPL-3.0; both texts are included in `licenses/`). Suitcase itself is open-source under the MIT License; users may modify or replace `heic-to` and rebuild the application from the complete source tree using the instructions in `docs/BUILDING.md`.
- **MPL-2.0 Rust crates**: Used unmodified as transitive Cargo dependencies. Source code for all npm and Cargo dependencies is available from their respective package registries using the committed `package-lock.json` and `src-tauri/Cargo.lock`.
