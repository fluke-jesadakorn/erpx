# G3-006-CUSTOMER-AUDIT-UI-007

## Delivered

- The existing `/platform/customers/:id/access-audit` route now calls the typed `GetCustomerAccessAudit` gRPC method on both the web and native/desktop transport branches.
- Requests use the existing `x-erpx-access-token`, `x-erpx-actor`, and `x-erpx-session` metadata pattern, the existing `platform_query_context`, purpose `platform_customer_access_audit`, a page size of 25, and empty first-page cursors.
- The UI view model retains only audit ID, actor, module, scope, purpose, occurred-at time, and correlation ID. Wire `session_ref`, wire `customer_id`, and any raw projection payload are not copied into the display model.
- The route exposes loading, safe denied/error, empty, stale, read-only, first-page, and projection-freshness states. Other Customer 360 projection routes remain explicit read-only placeholders.
- Route tests retain deep-link credential redaction coverage; console tests cover platform scope, bounded request construction, display redaction, and safe error categories.

## Verification

| Command | Result |
| --- | --- |
| `rustfmt --edition 2024 --check erpx/apps/console/src/main.rs` | pass |
| `rustfmt --edition 2024 --check erpx/crates/ui/src/routes.rs` | pass |
| `cargo test --manifest-path erpx/Cargo.toml -p erpx-ui` | 19 passed |
| `cargo test --manifest-path erpx/Cargo.toml -p erpx-console --no-default-features --features web` | 8 passed |
| `cargo test --manifest-path erpx/Cargo.toml -p erpx-console --no-default-features --features desktop` | 8 passed |
| `cargo check --manifest-path erpx/Cargo.toml -p erpx-console --no-default-features --features web` | pass |
| `cargo check --manifest-path erpx/Cargo.toml -p erpx-console --no-default-features --features desktop` | pass |
| `cargo clippy --manifest-path erpx/Cargo.toml -p erpx-console --all-targets --no-default-features --features web -- -D warnings` | pass |
| `cargo clippy --manifest-path erpx/Cargo.toml -p erpx-console --all-targets --no-default-features --features desktop -- -D warnings` | pass |

The workspace-wide `cargo fmt --all -- --check` remains blocked by pre-existing formatting differences in `erpx/apps/worker/src/lib.rs` and `erpx/crates/identity/src/webauthn.rs`; those forbidden paths were not changed by this packet.

## Integration caveats

- Platform console requests intentionally send `TenantScope::Platform`, as required by this packet. The server currently authorizes that branch through the platform customer-read permission. Customer-security administrators using a holding/company/branch tenant scope remain a separate backend authorization path and must be exercised in an authenticated tenant-scoped integration test before this UI is treated as a tenant self-service surface.
- The implementation consumes only the first page. A continuation cursor is detected and disclosed, but pagination is intentionally not exposed in this bounded packet.
- Android/iOS target builds, native secure storage, deep-link event plumbing, and full Customer 360 backend projection coverage remain outside this packet.

## Rollback

Revert only `erpx/apps/console/src/main.rs`, `erpx/crates/ui/src/routes.rs`, and this evidence directory. No proto, server, domain, persistence, migration, or Folio state was changed.
