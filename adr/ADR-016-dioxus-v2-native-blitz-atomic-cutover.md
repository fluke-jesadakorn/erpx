# ADR-016: Dioxus v2 delivery and Native/Blitz atomic cutover

**Status:** Accepted

**Supersedes:** The WebView-capable target-adapter interpretation in ADR-010
**Date:** 2026-08-18

## Decision

ERPX Console has one typed Dioxus route tree and one delivery-contract crate.
Web renders with Dioxus SSR/hydration and typed Server Functions. Native
renders with Dioxus Native over the vendored Blitz/WGPU closure and owns a
direct Tonic channel. WebView is benchmark evidence only and is not a runtime
fallback.

The canonical tree contains the public, authentication, Console, and platform
layouts; four shared Console routes; 21 workspace roots; typed resource and
UUID record routes; the item-reference route; and the two public marketing
routes. Commerce and Talent are Web-only. Invalid, non-canonical, and legacy
paths fail closed without aliases.

The active protobuf surface is exactly 65 services in
`erpx.<context>.v2`, catalogued as `IFC-SVC-201..265`. Generated v1 modules,
the browser arbitrary-gRPC proxy, and the legacy session-upgrade endpoint are
not registered in the runtime.

Development remains a monolithic Dioxus hot-reload build. Release enables
Dioxus Router WASM splitting and must emit independently addressable entries
for all 21 workspaces. The generated-asset verifier rejects stale, missing, or
unreferenced chunks and enforces the base, workspace, and cold-route Brotli
budgets.

Native/Blitz remains experimental. ERPX vendors the pinned source closure,
records its licenses/revisions/checksums, and carries bounded patches for
AccessKit actions, focus, semantic state, dialogs, and UTF-8 IME composition.
Unsupported platform integrations return an explicit `Supported`, `Degraded`,
or `Unavailable` capability. The native kill switch can stop distribution but
cannot select another renderer.

## Consequences

- Web, Server Functions, gRPC v2, and native artifacts are one process set and
  are promoted or rolled back together.
- Rollback does not reset PostgreSQL, MinIO, or retained objects.
- Authority is constructed only by trusted session/native adapters; client
  route parameters never supply actor or tenant authority.
- Protected responses are `private, no-store`; public preview responses use a
  validator and explicit cache policy.
- macOS release still requires measured accessibility and performance evidence
  on the locked certification hardware. iOS and Android require cross-target
  compilation but are not runtime-certified by that compile result.

## Evidence

- `crates/delivery-contract/src/lib.rs`
- `apps/console/src/routes.rs`
- `apps/console/src/delivery.rs`
- `apps/console/src/native_delivery.rs`
- `third_party/native-renderer/README.md`
- `xtask/src/wasm.rs`
- `audits/DIOXUS-V2-CUTOVER-2026-08-18.md`
- `audits/DIOXUS-V2-TRACEABILITY-2026-08-18.md`
