# Dioxus v2 implementation traceability — 2026-08-18

| Requirement | Implementation | Automated evidence |
| --- | --- | --- |
| Typed canonical routes and 21 workspaces | `apps/console/src/routes.rs`; `WorkspaceV2` | route round-trip, malformed/legacy, UUID, and channel tests |
| Route-local reactivity | Dioxus `Store`, `Signal`, `Memo`, server futures | route module ownership and session-cache tests |
| Session cache | `SessionCacheV2` | 64-entry, 32 MiB, freshness, tags, and scope-change tests |
| Web Server Functions | `apps/console/src/delivery.rs` | typed endpoints, trusted-context construction, no-store/public-cache code tests; mutation/auth adapters remain fail-closed blockers |
| No arbitrary browser gRPC proxy | Console server route set | runtime smoke expects the former proxy path to return 404 |
| gRPC catalog v2 | `crates/proto`, `crates/integration-contracts` | exact descriptor/name/count and `201..265` tests |
| Release WASM splitting | `web-split`, router route modules, `xtask/src/wasm.rs` | generated graph validation, missing/stale chunk tests, Brotli budgets |
| Native-only Blitz/WGPU | `apps/console/src/app.rs`, `platform.rs`, `native_delivery.rs` | renderer/capability/kill-switch tests, TLS-only direct Tonic composition, and dependency-policy scan |
| AccessKit and IME patch | `third_party/native-renderer` | semantic-role tests plus native compile and renderer regression surface |
| 10,000-row virtualization | `apps/console/src/native_spike.rs` | bounded visible-window test; manual keyboard/VoiceOver evidence required |
| Mobile same renderer path | Cargo `native` feature and typed unavailable adapters | iOS and Android cross-target compile passed through the same Native/Blitz graph; this is not runtime certification |
| Atomic rollback | v4 cutover manifest and ADR-016 | manifest exact generation/range and no-merge tests |

## Interface mapping

- Web UI → `ConsoleGateway` → typed Dioxus Server Functions → trusted session
  authority. Bootstrap/query/public preview are active; command,
  authentication, and public-form adapters currently return typed
  `Unavailable`/`Unauthenticated` and block release.
- macOS/iOS/Android UI → `ConsoleGateway` → direct TLS Tonic channel. The
  operation-specific generated `erpx.<context>.v2` client dispatch remains
  unimplemented and returns typed `Unavailable`, so compile success is not a
  runtime certification.
- Command receipts carry aggregate identity/version and invalidation tags;
  only matching cached loaders are invalidated.

The old Console composition and transport sources are not members of the
active module graph. They remain only as retained pre-cutover acceptance input
until the corresponding historical G11/G14 source-bound tests are archived;
they cannot be reached by the v2 router or server.
