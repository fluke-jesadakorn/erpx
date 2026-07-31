# G6-PRV-010 — Safe Platform Operations tenant detail adapter

## Delivered behavior

Added `SqlxTenantStoreRegistry::get_detail`, which validates an opaque tenant
identifier before querying the authoritative control-plane registry and returns
only safe tenant/holding references, lifecycle state, registry version, and
boolean registration-presence flags. Database names/keys, storage namespaces,
credential references, and backup references are never represented by the
detail DTO or selected into it. The existing tenant registry remains the
control-plane source; no tenant database or migration changed.

## Files changed

- `crates/infrastructure/src/repositories.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/infrastructure/tests/tenant_registry_detail.rs`
- `erpx/audits/G6-PRV-010/README.md`

## Verification

```text
cargo fmt --all --check
cargo clippy -p erpx-infrastructure --all-targets -- -D warnings
cargo test -p erpx-infrastructure --test tenant_registry_detail
```

The focused tests cover safe debug output and invalid opaque IDs failing before
database access. Platform Operations GetTenant transport, readiness/detail
composition, provisioning actions, backups, integrations, and full UI journeys
remain open.

## Rollback

Remove only the additive detail DTO/export, method, focused test, and audit.
There is no database or migration rollback.
