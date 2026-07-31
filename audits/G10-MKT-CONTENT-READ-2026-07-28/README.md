# G10 Marketing content replacement-read list — 2026-07-28

## Scope

This packet adds an authenticated, holding-routed list read over the existing
`erpx_tenant.marketing_content_read_rows` replacement projection for assets,
templates, forms, and domains. It is metadata-only: aggregate state, object
bytes, bearer tokens, provider references, actors, and credential material do
not cross the read contract.

## Authoritative behavior

- SQLx uses the tenant read pool and binds the holding RLS setting.
- Rows are accepted only when the aggregate type, UUIDs, safe label/status,
  positive source version, source event, checkpoint, and projection receipt
  are continuous.
- The list is bounded to 100 rows and keyset-paginates by aggregate UUID.
  An unfiltered page fails closed if UUID ordering is ambiguous across types.
- The RPC requires authenticated `marketing.read`, explicit
  holding/company/branch scope, customer-scope containment, tenant read
  routing, bounded cursors, and append-only customer-visible audit.
- The server rechecks each projected row scope before returning it. The
  Dioxus console rejects malformed IDs, unsupported types, unsafe text,
  source version gaps, and non-read-only rows before rendering.

## Evidence

- `cargo check -p erpx-proto --offline` — PASS.
- `cargo check -p erpx-server --offline` — PASS.
- `cargo check -p erpx-console --offline --features web` — PASS.
- `cargo test -p erpx-console --offline --features web --no-fail-fast --quiet` —
  PASS, 113 tests.
- Strict server/console Clippy with warnings denied — PASS.
- Marketing content infrastructure unit/migration contract tests and the
  dedicated SQLx acceptance are wired; live execution remains queued without
  `ERPX_TEST_TENANT_WRITE_DATABASE_URL`.

## Rollback

The transport and console list are additive. Rollback removes the RPC and UI
adapter plus the infrastructure list method; existing write snapshots,
outbox events, and rebuildable read rows remain unchanged. No activation,
provider change, Folio ownership change, or production cutover occurred.
