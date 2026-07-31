# G10-MKT-054 Marketing signed content-object access

This packet adds the authorization-binding seam for Marketing asset versions.
It is intentionally not a complete object transport or MinIO production gate.

## Integrated behavior

- Asset versions are validated before any signed URL is generated.
- The caller must provide a non-platform, tenant-containing access context with
  actor, purpose, correlation, and exact upload/download permission.
- The tenant registry's opaque storage namespace is required; caller URLs and
  credentials are not accepted.
- Upload and download URLs are bounded to one hour and use the asset's stored
  opaque object key, not caller-supplied key material.
- Cross-holding access, permission confusion, invalid namespaces, and excessive
  lifetimes fail closed.
- MinIO credentials remain in the existing redacted configuration boundary.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- Focused Marketing content object-access tests — PASS; signed URL binding,
  scope, permission, and lifetime cases.

## Explicitly open

This packet does not claim live MinIO connectivity, object metadata/checksum
commit, server RPC/HTTP authorization integration, audit-row persistence,
preview/publication workers, domain binding/verification, form submission
commands, Dioxus/SSR, or the G10 gate. Tenant-write form and custom-domain
aggregate snapshots are now covered by the adjacent G10-MKT-053 amendment;
binding and submission behavior remain open.

No Folio behavior or Folio write ownership changed.
