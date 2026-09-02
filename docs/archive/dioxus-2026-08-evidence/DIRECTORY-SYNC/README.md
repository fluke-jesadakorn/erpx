# ERPX G2/G14 — Directory synchronization

Status: implementation slice complete; integration and external certification remain queued with the coordinator. This packet owns only `identity/src/directory/sync.rs`, `identity/src/directory/feeds.rs`, and this evidence file. It does not register the module, add migrations, modify protobufs, or change server/worker/shared authorization ownership.

## Authoritative behavior

`DirectoryConnector` is the durable connector contract. It requires a TLS-only endpoint (`ldaps://` for LDAP/AD, `ldap://` only when the selected transport is explicit StartTLS, and `https://` for UCS Provisioning Service), a bounded page size of 1–5,000, and a bounded in-flight setting of 1–64. Connector scope is either `Platform` or one exact `Holding(Uuid)`; scope containment is checked by `DirectoryAuthorizationContext` before a command is accepted.

`ImportPlan`, `ImportCheckpoint`, `DirectoryPage`, and `FullImportRunner` provide the full-import contract:

- The hard maximum is 1,000,000 users per import. No caller can raise it through the public constructor.
- The source page, staging rows, idempotency marker, and next cursor are committed in one SQL transaction. The worker never advances a cursor before that transaction commits.
- Each page is bounded, sorted by immutable external ID, and has an exact continuation cursor. A replayed page is a no-op through `(import_id, idempotency_key)`; a changed replay is a conflict.
- Records are staged under an import snapshot. Canonical directory identities and the rebuildable read model are changed only by terminal finalization, so a failed import can be rolled back without exposing partial source state.
- A missing user in a successful full snapshot is transitioned to `deleted`; an observed disabled or deleted record is never converted to active. Native disable/delete changes produce a `RevocationSlaSignal` with a five-minute deadline.
- `ImmutableIdentityLink` is the only link key. It is an authority plus immutable value (`entryUUID`, AD `objectGUID`/immutable ID, or UCS immutable UUID), fingerprinted with SHA-256. External IDs, DNs, UPNs, login names, email, and display names are mutable attributes and cannot replace the link.
- Terminal finalization accepts the current connector-scoped role mappings. It validates scope containment inside the same transaction, upserts mapping revisions, appends hash-chained grant/revoke facts for every staged user, and appends compensating revokes for users absent from the snapshot. The role-fact sequence advances past the existing per-identity tail, so a full import cannot rewrite or truncate role history.
- The same finalization transaction hydrates `directory_identity_links` and the disposable `directory_identity_projection` from the canonical snapshot. Active, disabled, deleted, and missing users therefore reach the revocation read boundary atomically with identity outbox events; a projection rebuild can use the committed source events without trusting worker memory.

## Feed contracts

`feeds.rs` has explicit protocol seams. Production clients must implement the corresponding trait and fail closed on TLS, certificate, bind, cookie, or protocol errors:

| Connector | Feed | Required native state |
| --- | --- | --- |
| Generic LDAP | `GenericLdapPollingFeed` | RFC 2696-style paged cookie, stable immutable-ID ordering, polling/change watermark supplied by the client |
| Microsoft Active Directory Domain Services | `ActiveDirectoryDirSyncFeed` | AD DirSync cookie, object security, incremental values, link values, naming context, and highest committed USN |
| Generic LDAP sync server | `Rfc4533Feed` | RFC 4533 cookie, `RefreshOnly` for a full refresh, `RefreshAndPersist` for native changes, reload/refresh-required handling |
| Univention Corporate Server | `UcsProvisioningServiceFeed` | HTTPS Provisioning Service token and sequence, position, full-snapshot marker, and change continuation |

`FailClosedProtocolAdapter` is an explicit unavailable seam. It returns an error and no records; it is not a test data source or completion evidence.

## Required SQLx migration contract

The migration owner must add these write-side tables under `erpx_identity` without changing the existing identity migration’s write/read boundary. The exact column names consumed by the SQLx adapter are:

- `directory_connectors`: `id`, `vendor`, `feed_kind`, `scope_kind`, `scope_id`, `endpoint`, `security`, `page_size`, `max_in_flight_pages`, `enabled`, timestamps, and encrypted credential reference (never a credential value in an event or read model).
- `directory_imports`: `id`, `connector_id`, `scope_kind`, `scope_id`, `state`, `page_size`, `max_users`, JSONB `cursor`, `users_seen`, `pages_applied`, `last_external_id`, 32-byte `snapshot_digest`, `started_at`, `updated_at`, `finalized_at`, and `failure_reason`.
- `directory_import_pages`: `(import_id, idempotency_key)` primary key, `page_digest`, JSONB `cursor`, JSONB `next_checkpoint`, `records_count`, and `applied_at`.
- `directory_import_members`: `(import_id, external_id)` primary key, `identity_id`, `connector_id`, immutable authority/value, `state`, 32-byte `record_digest`, JSONB `record`, deterministic `event_id`, unique `idempotency_key`, and `observed_at`.
- `directory_identities`: `id`, `connector_id`, scope, `external_id`, immutable authority/value, `state`, JSONB `record`, `aggregate_version`, `last_import_id`, `created_at`, and `updated_at`; enforce uniqueness on `(connector_id, external_id)` and an immutable-link trigger/constraint for the authority/value pair.
- `directory_identity_links`: immutable authority/value primary key, `connector_id`, `external_id`, `identity_id`, optional linked `principal_id`, and append-only link timestamp. A conflict for the same immutable key must fail closed rather than rebind.
- `directory_revocation_sla`: `event_id` primary key, connector/external/link identity, observed time, deadline, reason, and status (`pending`, `acknowledged`, `late`).
- `directory_reconciliation_runs`: import ID, expected/staged/canonical counts, expected/observed 32-byte digests, status (`reconciled` or `blocked`), and checked time.

The adapter uses the existing `erpx_control.outbox_events` schema for durable source events. Its event writes are in the same transaction as the canonical state change. `holding_scope` contains `{kind, holding_id, connector_id}`; `aggregate_type` is `directory_identity` or `directory_import`; `aggregate_version` is strictly positive; and `idempotency_key` is unique. No outbox payload contains bind credentials or secrets.

## Exact integration hooks

The coordinator should register `pub mod directory { pub mod feeds; pub mod sync; }` and re-export the required DTOs from the identity crate only after the shared API ownership is ready.

1. **Worker.** Register a per-connector full-import job that constructs `FullImportRunner<F, SqlxDirectorySyncStore>`, calls `run`, persists the returned checkpoint receipt, and schedules the five-minute SLA scanner. Register a native change job that calls `ingest_changes` with the persisted protocol cursor. Use one runner page at a time and cap concurrent runners by `ImportConfig::max_in_flight_pages`.
2. **Projector/read model.** Consume `directory.identity_upserted.v1`, `directory.identity_disabled.v1`, `directory.identity_deleted.v1`, and `directory.import_completed.v1` from `erpx_control.outbox_events`. Use `(projection_name, event_id)` receipts and a per-identity aggregate-version checkpoint. The read model must be disposable and rebuildable from the source outbox; never write source identity state from the read database.
3. **Role mapping.** The worker loads the connector's directory-managed mappings before a full import and calls `FullImportRunner::run_with_role_mappings`; the SQLx finalizer validates the persisted connector scope, resolves group observations from staged record JSON, and appends only connector-scoped role facts. A delete/disable or missing snapshot member is a revocation command, not a role deletion: role assignment history stays append-only and effective access is cut off at the source event/deadline.
4. **Authorization/API.** The server command boundary calls `DirectoryAuthorizationContext::authorize` with the authoritative actor, exact platform/holding scope, current MFA timestamp, action, and request time. Effective role authorization consults the identity write-side directory projection before local role assignments; stale, revoked, disabled, or unmapped directory-managed identities fail closed, while only principals without a directory projection use the local role path. High-risk import, reconcile, rollback, and mapped access actions require recent MFA. Break-glass is accepted only for a localhost request and must be a separately audited local operator path; it is not a remote-directory fallback.
5. **Console/UI.** The UI should call typed connector/import/reconciliation status APIs only. Render scope, vendor/feed kind, cursor watermark, users/pages, last successful checkpoint, SLA pending/late counts, and rollback state. The UI must not accept or display credentials, raw cookies, immutable-link values, or arbitrary endpoint URLs.
6. **Reconciliation.** A scheduled worker calls `DirectorySyncStore::reconcile_import`, which compares completed snapshot counts and immutable/record content between staged and canonical rows and persists a `ReconciliationReport`. The coordinator then adds source-watermark, outbox, and read-projection checkpoint evidence, scans pending SLA rows, and marks a run reconciled only when all authorities agree. Any unexplained count or link mismatch blocks activation and queues a new bounded import; it never repairs by mutating a link.
7. **Rollback.** Before terminal finalize, `rollback_import` deletes only the staged import rows and marks the import `rolled_back`. After terminal finalize, rollback is a compensating source command/rebuild request: do not delete historical identities, links, role revocations, or outbox events. Rebuild the read model from the authoritative source outbox and retain the import audit trail.

## G14 test and evidence hooks

The colocated unit/property-oriented tests cover TLS and bounds, connector/vendor closure, sorted page/cursor progression, terminal state, replay and immutable-link conflicts, five-minute revocation deadlines, scope containment and MFA, deterministic event/page idempotency, LDAP case-insensitive attribute mapping, and fail-closed unavailable protocol seams. The ignored SQLx acceptance fixture also runs a resumable full import with a real role mapping and asserts grant/revoke facts plus three projected identities; it is intentionally database-backed rather than schema-only. The SQLx methods stage pages, lock/finalize imports, persist immutable links, write source outbox events, apply native changes transactionally, and record SLA signals.

The coordinator must add the integration evidence after registration: SQLx tests against PostgreSQL for rollback/idempotent retry and concurrent claim behavior; projection rebuild tests; API authorization tests; 1M-record bounded-memory soak; and vendor packet captures/acceptance tests. No mock client can certify a vendor protocol.

## External certification blockers queued

No external LDAP, Microsoft AD DS, or UCS environment/credential was available in this worktree. The following blockers are therefore explicitly queued, while the local domain and persistence work remains usable:

- **CERT-DIR-LDAP-001:** run against a real TLS LDAP server with certificate validation, paged results, stable immutable ordering, polling watermark, disable, delete, timeout, and reconnect cases.
- **CERT-DIR-AD-001:** run against AD DS with DirSync rights and a real cookie across restart, USN advancement, disable, delete, rename, ACL denial, and cookie invalidation cases.
- **CERT-DIR-RFC4533-001:** run RefreshOnly/RefreshAndPersist against an RFC 4533 server, including refresh-required and delete states.
- **CERT-DIR-UCS-001:** run against UCS Provisioning Service over HTTPS with a service credential, token/sequence restart, disable/delete, and service outage cases.

Until those packets are signed, the connector remains non-activatable and the fail-closed adapter must be used. This is a certification blocker, not evidence of successful vendor acceptance.
