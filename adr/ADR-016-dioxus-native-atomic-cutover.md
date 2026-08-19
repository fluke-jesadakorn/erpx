# ADR-016: Dioxus Native/Blitz atomic cutover

**Status:** Accepted  
**Date:** 2026-08-18

ERPX has one typed Dioxus route tree and one delivery-contract crate. Web uses
SSR, hydration, and typed Server Functions under `/api/server-fn/*`. Native
uses the vendored Blitz/WGPU closure and a direct Tonic channel. WebView is
measurement evidence only and is never a runtime fallback.

The protobuf tree is unversioned. Project-owned package names, routes, Rust
types, and artifacts may not carry generation branding. Provider-owned API
versions and immutable provider revision fields remain allowed at their
boundaries.

Platform administration exposes Holding 360 at `/platform/holdings` and UUID
holding detail routes. Business Customer 360 remains inside an authenticated
holding workspace.

Development stays monolithic for hot reload. Release uses Dioxus Router WASM
splitting and the generated-asset verifier rejects stale, missing, or
unreferenced chunks. Native certification remains a separate manual gate.

Authority is constructed only by trusted session/native adapters. Client
route parameters never supply actor, storage route, or holding authority.

Evidence: `apps/console/src/routes.rs`, `crates/proto/proto/erpx`,
`xtask/src/wasm.rs`, and `audits/DIOXUS-CUTOVER-2026-08-18.md`.
