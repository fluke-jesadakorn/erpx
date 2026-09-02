# Dioxus cutover baseline — 2026-08-18

This retained record separates observed build facts from release
certification. ERPX uses the pinned Rust toolchain and Dioxus CLI. The active
project-owned protobuf packages and service paths are unversioned.

The release build emitted a split Web graph within the recorded base,
workspace-entry, and cold-route Brotli budgets. Development intentionally
remains monolithic for hot reload. Raw generated manifests stay under
`.runtime/` and are not committed.

Native/Blitz remains experimental. macOS performance and accessibility need
the locked hardware evidence; iOS and Android compilation does not claim
runtime certification. Use `cargo xtask verify-platforms` only for the manual
platform gate.
