# Directory-managed RBAC

## Scope

This packet defines the isolated directory-managed role mapping boundary for
generic LDAP, on-premises Microsoft Active Directory Domain Services, and
Univention Corporate Server connectors. The module is registered only as the
`directory::rbac` child; shared manifests, migrations, protobuf, server and
worker registration, console code, and coordinator ledgers are outside this
packet.

## Authoritative behavior

- `DirectoryGroupRef` and `DirectoryRoleRef` are bounded typed references;
  mutable usernames, DNs, and display names cannot become identity keys.
- `DirectoryRoleIdentity` accepts only the immutable authority/value link from
  synchronization and derives a stable hash key. Connector ID remains part of
  every mapping, fact chain, read-model key, and outbox aggregate identity.
- `DirectoryManagedRoleMapping` is revisioned and connector-scoped.
  `DirectoryRoleMappingSet` rejects connector mismatches, duplicate mapping
  keys, and mappings outside the connector scope. Platform connectors may
  contain holding mappings; holding connectors cannot widen to platform scope.
- `DirectoryRoleEvaluation` fails closed for inactive identities, revoked
  observations, future-dated observations, and observations older than the
  five-minute revocation SLA. Group membership grants only an active mapped
  role, and mapping-level MFA requirements are explicit.
- `DirectoryRoleFactLedger` and `SqlxDirectoryRbacStore` model grant/revoke
  changes as append-only, revisioned facts with idempotency keys, hash-chain
  verification, and transactional redacted outbox events. Reconciliation and
  rollback preserve immutable identity links and role history; the read model
  is rebuildable.
- High-risk mapping/fact operations require fresh MFA. Local break-glass is
  accepted only when explicitly marked and localhost-only.

## Coordinator review — 2026-07-28

The synchronization audit found and corrected a real persistence-boundary
defect: full-import projection and role-fact keys had been derived from the
mutable external ID even though staged records carried an immutable
authority/value pair. The canonical key now derives only from that immutable
pair, native-change identity links are populated with the same key, and an
existing immutable pair cannot be rebound to another canonical identity.
Grant facts persisted through `SqlxDirectoryRbacStore` also require an active
connector-contained managed mapping; revocation facts remain append-only when
the mapping is deactivated.

Dedicated regression coverage is in
[`crates/identity/tests/directory_rbac_immutable_linking.rs`](../../crates/identity/tests/directory_rbac_immutable_linking.rs):
external-ID/login renames retain the same role key, while a changed immutable
directory object key produces a different identity. The SQLx scope-authority
test and `directory_sqlx_acceptance` remain the live disposable-database
evidence for forged connector-scope rejection and canonical immutable-link
persistence; no vendor directory certification is implied. Identity-only
Clippy with warnings denied and the focused directory test suites pass.

## Focused evidence

The unit suite in
[`erpx/crates/identity/src/directory/rbac.rs`](../../crates/identity/src/directory/rbac.rs)
covers connector and holding scope containment, stale-feed revocation,
high-risk MFA, local-only break-glass, append-only idempotent facts, hash-chain
verification, idempotent projection, and history-preserving rollback.

Focused commands:

```text
rustfmt --edition 2021 crates/identity/src/directory/rbac.rs
cargo test -p erpx-identity --lib directory::rbac
cargo clippy -p erpx-identity --all-targets -- -D warnings
```

The SQLx adapter is implemented but no live database certification is claimed
by this packet because the required routed PostgreSQL database pair and
credentials were unavailable during this slice. Live schema/write/read-model,
outbox, rollback, and reconciliation runs remain queued.

Coordinator continuation on 2026-07-28 exercised the current SQLx directory
authorization path against a fresh disposable PostgreSQL database. An active
mapped identity was allowed, a disabled identity was denied, a stale feed was
denied at the five-minute boundary, and a high-risk mapping required a recent
write-side TOTP verification before access was allowed. The database was
dropped after the run. This is local disposable evidence only; credentialed
vendor, scale, physical MFA, and production acceptance remain queued.

## Vendor certification blockers

No credentialed external environments were available for acceptance testing.
Generic LDAP/RFC 4533, on-premises AD DS/DirSync, and UCS Provisioning Service
must each be certified against a live TLS-only directory, immutable-link
rename/disable/delete cases, role-mapping updates, native feed replay, and
five-minute revocation behavior before this packet can claim vendor acceptance.

## Rollback

Use `DirectoryRbacRollbackPlan::staged_import` for an unfinalized import or
`DirectoryRbacRollbackPlan::finalized` for a forward-only connector disable.
Both preserve immutable links and append-only role facts; finalized rollback
marks the read model for rebuild. This packet does not activate a customer
directory or transfer production command ownership.
