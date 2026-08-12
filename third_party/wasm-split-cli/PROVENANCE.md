# Provenance

- Crate: `wasm-split-cli`
- Upstream version: `0.8.0-alpha.1`
- crates.io source checksum: recorded in the workspace `Cargo.lock`
- Upstream repository: <https://github.com/DioxusLabs/dioxus>
- Upstream revision: `5fb7e6e1ca748a0901bd536388c7eb567e560b74`
- Upstream path: `packages/wasm-split/wasm-split-cli`
- License: MIT OR Apache-2.0; verbatim license texts are retained beside this file.

ERPX carries one compatibility patch: normalize `reloc.CODE` offsets by the
encoded function-count prefix reported by `wasmparser::Payload::CodeSectionStart`,
accounting for whether the linked Walrus release retains that prefix in its
original function ranges. Checked arithmetic and range validation fail closed.
No `reloc.DATA` behavior is changed.
