# G6-G12-UI-010 — Authenticated Platform Operations holding list and detail

## Delivered

- The Dioxus console calls the authenticated Tonic
  `PlatformOperationsService::ListHoldings` endpoint for `/platform/holdings`.
- The list follows the server's bounded opaque cursor until completion and
  renders only safe registry/readiness metadata.
- Each validated opaque holding route calls `GetHolding` and renders the same
  safe metadata for `/platform/holdings/:id`.
- WebAssembly and native transport adapters attach the access token, actor,
  session, platform query context, and customer-visible audit purpose.
- Loading, denied, error, empty, and read-only states remain explicit; no
  credentials, database keys, storage namespaces, backup references, or
  mutation controls enter the client model.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Console route/model tests — PASS in `cargo test --workspace`.
- Folio lint/typecheck — PASS in the integration wave.

## Open boundary

Projection, backup, integration, audit, settings actions, native secure
storage, Android/iOS builds, and the full Platform Operations journey remain
open. The holding detail adapter is read-only and does not imply provisioning
or lifecycle command authorization.

## Rollback

Revert only the console holding adapter/model/surface changes and this audit
record. Preserve shared route contracts and unrelated Folio/ERPX changes.
