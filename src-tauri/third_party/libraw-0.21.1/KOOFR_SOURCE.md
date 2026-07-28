# Vendored LibRaw source

Koofr-GUI builds this fixed LibRaw 0.21.1 source snapshot into the Windows
binary as a static library. It is derived from the source bundled by the
`libraw_rs_vendor` 1.0.0 crate (crate repository revision
`320bafdb5045a2980e15d4936eec80c99df339b3`) and is compiled by
`src-tauri/build.rs`.

Koofr-GUI selects LibRaw's CDDL-1.0 distribution option. The authoritative
license text is `LICENSE.CDDL`; upstream copyright information is in
`COPYRIGHT`. No optional external codecs, OpenMP, or CPU-specific flags are
enabled by the application build.
