# G3-006-CUSTOMER360-PAGES evidence

Task: route-aware Dioxus Customer 360 console surfaces.

## Delivered behavior

- Added typed, in-memory console navigation backed by the existing `erpx-ui::PlatformRoute` registry.
- The console visibly resolves the account list plus all required customer views: overview, organization, contacts, commercial, entitlements, onboarding, health, support, access-audit, and notes.
- Customer row selection moves to the typed customer overview route. Customer-scoped tabs are disabled until a validated customer identifier is available.
- Overview/customer-scoped data uses the existing audited platform `ListCustomers` and `GetCustomer` gRPC methods. The request carries the existing query context, actor/session metadata, and a non-empty read purpose.
- Routes whose backend projections are not yet exposed render an explicit read-only `Empty` placeholder identifying the canonical path and the missing projection. No business values are fabricated.
- The shell continues to gate content through `RouteStateBoundary`; authenticated, read-only, support, expired, revoked, denied, offline, stale, and conflict states remain owned by the shared session boundary.

## Route evidence

The console unit test `every_customer_360_path_resolves_to_a_visible_context` round-trips these paths through the typed registry:

```text
/platform/customers
/platform/customers/customer-001
/platform/customers/customer-001/organization
/platform/customers/customer-001/contacts
/platform/customers/customer-001/commercial
/platform/customers/customer-001/entitlements
/platform/customers/customer-001/onboarding
/platform/customers/customer-001/health
/platform/customers/customer-001/support
/platform/customers/customer-001/access-audit
/platform/customers/customer-001/notes
```

## Credential and authority boundary

- No credential, tenant database reference, storage secret, or fabricated role is rendered by the new surfaces.
- The console only transports the memory-held session access token through the existing authenticated gRPC seam. It does not persist tokens or decide authorization.
- Global-read auditing and authorization remain server responsibilities. The UI displays the audit disclosure but cannot grant access.
- Customer identifiers are validated by `PlatformRoute` and are treated as opaque route data; unsafe path segments are rejected.

## Exact verification

Passed:

```text
rustfmt --edition 2024 --check erpx/apps/console/src/main.rs
cargo test -p erpx-console --no-default-features --features web
cargo test -p erpx-console --no-default-features --features desktop
cargo check -p erpx-console --no-default-features --features web
cargo check -p erpx-console --no-default-features --features desktop
cargo clippy -p erpx-console --no-default-features --features web --all-targets -- -D warnings
cargo clippy -p erpx-console --no-default-features --features desktop --all-targets -- -D warnings
cargo test -p erpx-ui
cargo clippy -p erpx-ui --all-targets -- -D warnings
```

Results: 3 console tests passed on web and desktop; 15 shared UI tests plus doc-tests passed; web/desktop checks and both console/shared-UI clippy runs passed.

The requested workspace-wide formatter command was also run:

```text
cargo fmt --manifest-path erpx/Cargo.toml --all --check
```

It passed on the final worktree. The packet file also passes scoped rustfmt.

## Residual backend/client gaps

- The current protobuf/server contract exposes only Customer list/get summaries. Organization, contacts, commercial, entitlements, onboarding, health, support, access-audit, and notes data endpoints are not yet available; their UI surfaces intentionally remain placeholders.
- The console still has no shared Dioxus Router/browser deep-link bootstrap. The typed route state and all in-console tabs resolve correctly; wiring platform URL history/deep links requires a later routing packet and must not be implemented as client authorization.
- Android/iOS target builds and authenticated login/refresh transport integration remain CI/backend integration work from G1-005; this packet does not change those boundaries.

## Files changed

- `erpx/apps/console/src/main.rs`
- `erpx/apps/console/assets/main.css`
- `erpx/audits/G3-006-CUSTOMER360-PAGES/evidence/README.md`

## Rollback

Revert the two console source/asset changes and remove this evidence directory. No server, protobuf, database, Folio, shared-contract, or secret files were changed by this packet.
