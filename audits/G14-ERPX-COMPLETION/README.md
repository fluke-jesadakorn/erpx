# G14 ERPX directory hardening and reconciliation verifier

Status: the G14 verifier is implemented, but this lane does not claim the
directory gate is green. The real identity-contract acceptance run currently
exposes existing directory-RBAC compilation errors, and all external vendor
certification remains queued.

This lane owns only:

- `apps/migrator/tests/g14_directory_acceptance.rs`
- this README

It does not modify manifests, migrations, protobufs, shared libraries, server
or worker registration, authorization, console code, `MASTER_PLAN.md`, or
`EXECUTION_LEDGER.md`.

## Executable checks

The verifier has three tests:

1. `g14_authoritative_directory_contracts_and_seams_are_present` checks the
   existing public identity source and its integration seams for the required
   bounds and hardening markers. It checks implementation surfaces, not a
   schema-only fixture: SQLx stores, transactional outbox writes, native feed
   adapters, the worker fail-closed boundary, immutable identity links, role
   hash chaining, revocation SLA rows, rollback, and reconciliation contracts.
2. `g14_runs_the_identity_crate_public_directory_acceptance_tests` invokes the
   actual `erpx-identity` `directory` test suite with `--offline`. It does not
   provide an in-memory directory client, fabricate vendor responses, or treat
   a mock as certification evidence.
3. `g14_external_vendor_certification_is_queued_without_environment` reports
   a non-secret queued blocker when the three endpoint references are absent:
   `ERPX_G14_LDAP_ENDPOINT`, `ERPX_G14_AD_DS_ENDPOINT`, and
   `ERPX_G14_UCS_ENDPOINT`. Even when references exist, this verifier does not
   claim vendor acceptance without the certified external suite.

Run the lane with:

```text
cargo test -p erpx-migrator --test g14_directory_acceptance --offline
```

## Requirement coverage

| Requirement | Executable/evidence coverage |
| --- | --- |
| Full import up to 1,000,000 users | `MAX_DIRECTORY_IMPORT_USERS`, `MAX_FULL_IMPORT_USERS`, `DirectoryImportPlan`, `ImportConfig`, and the identity bounded-import tests. |
| Generic LDAP, AD DS, UCS | Public provider/feed enums, `FeedKind::supports`, and the closed vendor matrix. |
| Native change feeds | RFC 4533, AD DirSync, and UCS Provisioning Service feed types, cursors, and fail-closed unavailable adapter. |
| TLS-only communication | Endpoint scheme validation, certificate-validation evidence, protocol floor, trust-anchor rotation, and bounded connect/StartTLS/bind policy. |
| Five-minute revocation | `DIRECTORY_REVOCATION_SLA`, `REVOCATION_SLA_MINUTES`, stale authorization checks, revocation deadlines, and persisted SLA rows. |
| Immutable identity linking | Connector-scoped immutable links, stable fingerprints, conflict rejection, and no mutable username/DN rebind path. |
| Directory-managed role revocation | Directory role facts carry `previous_hash`/`fact_hash`; revocation is represented as an append-only fact. |
| Redaction, MFA, break-glass | Zeroized/redacted credentials, high-risk step-up types, explicit local-only break-glass path, and fail-closed error variants. |
| Rollback | `FullImportRunner` invokes `rollback_import` on feed, page/checkpoint, and store failure; SQLx rollback is a store contract. |
| Reconciliation | `ReconciliationReport` and `reconcile_import` are required by `DirectorySyncStore`; SQLx import, projection, outbox, checkpoint, and SLA seams are audited by the verifier. |

## Findings from the current worktree

### Local contract test: blocked by existing directory-RBAC compilation errors

The verifier successfully builds and starts, but the nested public-contract
run fails while compiling `erpx-identity`:

```text
error[E0631]: type mismatch in function arguments
    directory/rbac.rs:810:25
error[E0369]: binary operation `==` cannot be applied to type
    Result<(), DirectoryRbacError>
error[E0618]: expected function, found DirectoryManagedRoleMapping
```

The current shared source now exposes the declared
`DirectorySyncStore::reconcile_import` implementation, but the identity crate
cannot compile its public directory test surface until the RBAC errors are
repaired: `max_by_key` receives the wrong reference shape, the error enum is
compared without the required equality implementation, and a test-local
`mapping` binding shadows its helper function. This is a real G14 acceptance
blocker, not a test harness failure. The verifier intentionally does not repair
it because the shared identity library is outside this lane’s ownership.

The source audit also confirms that the existing sync implementation already
contains the relevant rollback, transactional staging/finalization, durable
outbox idempotency, immutable-link conflict, and five-minute revocation paths.
Those are implementation evidence only until the identity crate compiles and
the SQLx database/rebuild checks run successfully.

### External certification: queued

No LDAP, on-premises Microsoft AD DS, or UCS endpoint reference is configured
in the current environment. Queue these acceptance runs without activating a
customer connector:

- generic LDAP: LDAPS/StartTLS certificate and hostname validation, paged
  one-million-user import, RFC 4533 cookie continuity, disable/delete, and
  reconnect/revocation behavior;
- Microsoft AD DS: TLS bind, objectGUID linking, DirSync cookie/USN behavior,
  ACL denial, rename, disable/delete, and cookie invalidation;
- UCS: TLS/HTTPS service authentication, immutable UUID linking, Provisioning
  Service token/sequence restart, disable/delete, and outage recovery.

Until those runs are signed, the connector must remain non-activatable. The
worker’s current no-transport path is fail-closed and refuses to claim import
work; this is not vendor acceptance evidence.

## Reconciliation and rollback evidence boundary

The verifier requires the authoritative code to retain:

- checkpoint-after-commit import progression and bounded pages;
- transactional canonical rows, revocation rows, rebuildable projection, and
  `erpx_control.outbox_events` publication with idempotency protection;
- immutable-link conflict rejection rather than silent rebinding;
- append-only role facts and five-minute revocation deadlines;
- a reconciliation report that blocks on count or digest mismatch; and
- forward-only rollback that preserves immutable identity, role-revocation,
  and outbox history.

The verifier itself performs no database writes, customer activation, command
ownership change, Folio write retirement, or production cutover. Once the
shared `reconcile_import` implementation is complete, the coordinator should
run the ignored PostgreSQL tests, projection rebuild/reconciliation checks,
and the external vendor acceptance suites before treating G14 as complete.

## Verification record

- `cargo fmt --all --check`: passed for the current ERPX tree after formatting
  the new verifier.
- `cargo test -p erpx-migrator --test g14_directory_acceptance --offline`:
  verifier starts; external-certification queue passes; identity-contract
  subprocess is blocked by the directory-RBAC compile errors above.
- No external LDAP/AD DS/UCS certification was claimed.
- No mocks, schema-only completion evidence, customer activation, or Folio
  write retirement was performed.

Rollback for this lane is limited to removing the new verifier and audit
README. It has no database or runtime side effect and does not alter ERPX or
Folio ownership.

## Coordinator revalidation — current tree — 2026-07-27

The historical RBAC compile-error finding above is superseded by current
coordinator evidence: the full identity crate, workspace Clippy, workspace
tests, directory SQLx acceptance, G14 directory acceptance, and Generic
LDAP/AD/UCS live-harness tests now pass locally. The focused G14 directory
acceptance is 8/8; the external-readiness target correctly leaves its one
credentialless certification preflight ignored/queued.

Remaining G14 blockers are external and authoritative: credentialed Generic
LDAP/RFC 4533, on-premises AD DS/DirSync, and UCS Provisioning Service runs;
measured one-million-user and five-minute revocation evidence; physical device
execution; approved protobuf baseline comparison; and live rollback or
cutover evidence. No customer connector activation, Folio write retirement,
command-owner switch, or production cutover occurred.

## Coordinator continuation — local G14 SQLx and RBAC evidence — 2026-07-28

Fresh disposable PostgreSQL runs now provide real local evidence for the
identity-owned directory paths. `g14_local_directory_sqlx_acceptance` passed
with the Generic LDAP full-import/resume and immutable-link path, AD DS
DirSync disable/revocation-SLA path, UCS Provisioning Service delete path,
Generic LDAP RFC 4533 cursor replay and role grant/revoke, projection, and
transactional outbox assertions. The separate
`role_mapping_save_rejects_scope_escape_without_side_effects` acceptance also
passed, proving a holding connector cannot persist a platform-scoped mapping
and that the rejected write leaves no mapping or outbox side effect. The
directory-RBAC authority target independently passed its persisted
connector-scope rejection and valid contained mapping/outbox assertions.
All disposable databases were removed after each run.

This is authoritative local SQLx evidence, not external vendor certification.
Credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, and UCS
Provisioning Service acceptance; measured one-million-user import and
five-minute revocation; physical mobile execution; approved descriptor
comparison; and live Folio migration/rollback/cutover evidence remain open.
No connector activation, command-owner switch, Folio write retirement, or
production cutover occurred.

The acceptance fixture was then extended and rerun with a persisted platform
connector and a platform-owned holding role mapping. The platform connector
round-tripped with platform scope, and the contained holding mapping committed
with its durable mapping row. This closes the local platform-plus-holding
scope matrix only; it does not certify a live multi-holding directory.

The deterministic directory volume suite also passed 5/5, including the
one-million-user bounded import/checkpoint restart replay, native LDAP/AD/UCS
cursor continuity, exact five-minute revocation deadlines, append-only role
fact projection, and local-only break-glass/high-risk MFA boundaries. This
remains deterministic contract evidence rather than a live vendor-scale or
physical-device certification.

## Coordinator current-tree correction — 2026-07-31

Current evidence updates the disposition without weakening the certification
boundary. A real local Generic LDAP/RFC 4533 run passed over trusted LDAPS,
covering full import, durable native-cookie continuity, provider-side disable,
incremental change observation, immutable linking, durable projection/outbox
evidence, and the five-minute revocation-SLA record. Real PostgreSQL 18/TLS and
HTTPS MinIO backup/PITR acceptance also passed with rollback and cleanup.

UCS UDM authentication is real and TLS-bound, but the official Provisioning
Service app/backend remains blocked by its valid-license requirement; native
subscription/message/feed evidence is not claimed. The local Samba
AD-compatible probe is not Microsoft AD DS evidence. Microsoft AD DS/DirSync,
licensed UCS Provisioning Service, approved descriptor provenance, physical
mobile execution, external scale/revocation measurements, complete G13
cutover/retirement, and production approval remain open. No activation,
ownership switch, Folio-write retirement, or production cutover occurred.
