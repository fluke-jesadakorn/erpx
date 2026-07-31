# G14 external directory certification readiness

**Status:** `BLOCKED — certification environment not supplied`  
**Date:** 2026-07-27  
**Scope:** Generic LDAP/RFC 4533, on-premises Microsoft Active Directory
Domain Services (AD DS/DirSync), and Univention Corporate Server (UCS)
Provisioning Service.

This package is a readiness gate, not vendor certification. It adds no
directory adapter, does not activate a connector, does not contact an
external endpoint, and does not use a fake server. Local identity SQLx,
outbox, worker, projector, authorization, immutable-link, role-fact, and
five-minute revocation evidence remains separate from this external gate.

## Exact required references

The names below are the current contract used by
`apps/worker/tests/generic_ldap_live.rs` and
`apps/worker/tests/directory_vendor_live.rs`. Values are intentionally not
recorded in this audit.

| Lane | Required environment variables |
| --- | --- |
| Generic LDAP | `ERPX_G14_GENERIC_LDAP_TEST_DATABASE_URL`, `ERPX_G14_GENERIC_LDAP_ENDPOINT`, `ERPX_G14_GENERIC_LDAP_BASE_DN`, `ERPX_G14_GENERIC_LDAP_BIND_DN`, `ERPX_G14_GENERIC_LDAP_PASSWORD`, `ERPX_G14_GENERIC_LDAP_CA_SHA256`, `ERPX_G14_GENERIC_LDAP_SERVER_NAME`, `ERPX_G14_GENERIC_LDAP_TLS_MODE` |
| AD DS/DirSync | `ERPX_G14_AD_TEST_DATABASE_URL`, `ERPX_G14_AD_ENDPOINT`, `ERPX_G14_AD_BASE_DN`, `ERPX_G14_AD_BIND_DN`, `ERPX_G14_AD_PASSWORD`, `ERPX_G14_AD_CA_SHA256`, `ERPX_G14_AD_SERVER_NAME`, `ERPX_G14_AD_TLS_MODE` |
| UCS Provisioning Service | `ERPX_G14_UCS_TEST_DATABASE_URL`, `ERPX_G14_UCS_ENDPOINT`, `ERPX_G14_UCS_BASE_DN`, `ERPX_G14_UCS_SUBSCRIPTION`, `ERPX_G14_UCS_SUBSCRIPTION_PASSWORD`, `ERPX_G14_UCS_CA_SHA256`, `ERPX_G14_UCS_SERVER_NAME`, `ERPX_G14_UCS_TLS_MODE` |

Optional controls are also read by the current live harnesses:

- Generic LDAP: `ERPX_G14_GENERIC_LDAP_FEED` (`ldap_polling` or `rfc4533`),
  `PAGE_SIZE`, `MAX_IMPORT_USERS`, `FULL_IMPORT`, `FILTER`, and the
  `*_ATTRIBUTE` mapping variables.
- AD DS: `ERPX_G14_AD_PAGE_SIZE`, `ERPX_G14_AD_MAX_IMPORT_USERS`,
  `ERPX_G14_AD_FULL_IMPORT`, and `ERPX_G14_AD_FILTER`.
- UCS: `ERPX_G14_UCS_PAGE_SIZE`, `ERPX_G14_UCS_MAX_IMPORT_USERS`,
  `ERPX_G14_UCS_FULL_IMPORT`, and `ERPX_G14_UCS_FILTER`.

The readiness gate rejects empty/control-character values, cleartext or
scheme/mode mismatches, CA fingerprints other than 64 hexadecimal characters,
database URLs without `sslmode=require`, `sslmode=verify-ca`, or
`sslmode=verify-full`, invalid feed/mode values, and import limits outside
`1..=1,000,000` (page size `1..=5,000`). It prints names only; passwords,
bind identities, endpoints, database URLs, and certificate fingerprints are
never included in blocker output.

## Current blocker

No credentialed Generic LDAP, on-premises AD DS, or UCS environment was
available for this run. Therefore the following remain **not certified**:

- `CERT-DIR-LDAP-001`: real LDAPS/StartTLS handshake and certificate/hostname
  validation; paged import; RFC 4533 RefreshOnly/RefreshAndPersist cursor
  continuity; disable/delete; reconnect; rollback and rebuild.
- `CERT-DIR-AD-001`: real DirSync rights and cookie restart; USN advancement;
  disable/delete/rename; ACL denial; cookie invalidation; objectGUID
  immutable linking; measured revocation.
- `CERT-DIR-RFC4533-001`: real RefreshOnly/RefreshAndPersist, refresh-required
  response, deletion state, cursor replay, and TLS outage/reconnect.
- `CERT-DIR-UCS-001`: real HTTPS Provisioning Service credential,
  token/sequence restart, disable/delete, replay/order, outage/reconnect, and
  entryUUID immutable linking.
- `CERT-DIR-SCALE-001`: a real one-million-user bounded-memory import with
  restart, duplicate suppression, cursor continuity, reconciliation, and
  staged rollback.
- `CERT-DIR-REVOKE-001`: a real native disable/delete event measured to deny
  effective access within five minutes, including stale-feed fail-closed
  behavior and append-only role-revocation evidence.

Presence of all variables would make a live attempt eligible; it would not
make any of these rows green. The live worker tests must complete against the
real systems and their explicitly disposable PostgreSQL databases.

## Readiness and certification commands

Run from `erpx/` with secrets supplied out-of-band and never echoed:

```sh
cargo test -p erpx-migrator --test g14_external_readiness --offline
cargo test -p erpx-migrator --test g14_external_readiness --offline -- --ignored --nocapture
cargo test -p erpx-worker --test generic_ldap_live --offline -- --nocapture --test-threads=1
cargo test -p erpx-worker --test directory_vendor_live --offline -- --nocapture --test-threads=1
```

The first command is environment-independent and records the exact queued
blocker names. The second command is intentionally ignored in normal runs and
fails closed when the external configuration is incomplete. It is a gate, not
a certification result. The last two commands invoke the current registered
worker path and must fail on a real connection, TLS, bind, feed, persistence,
or reconciliation error; they must not be replaced with a mock or a schema-
only check.

Before accepting a vendor row, preserve redacted command output plus the
database evidence identifiers and verify, at minimum: full import/resume at
the one-million-user bound; native feed restart/replay; immutable-link conflict
rejection; directory-managed role grant/revoke; stale-feed denial; measured
five-minute revocation; append-only outbox/role/revocation facts; source/read
reconciliation; projector rebuild; and rollback preservation. Keep the
customer connector disabled until the row is independently reviewed.

## Safety boundary

This readiness package performs no database writes, vendor calls, directory
activation, production command-owner switch, Folio write freeze/retirement, or
cutover. Missing or invalid configuration is a blocker. Even a successful
real run is only certification evidence for the specific supplied environment;
it is not permission to activate customer directories or cut over production.

## Harness contract audit — 2026-07-27

The permitted live-harness audit covered only
`apps/worker/tests/generic_ldap_live.rs` and
`apps/worker/tests/directory_vendor_live.rs`. No production adapter, worker
registration, protobuf, manifest, migration, shared authorization, console,
or coordinator ledger file was changed.

The audit closed these readiness gaps:

- Both harnesses now accept PostgreSQL evidence databases only when the URL
  contains an exact `sslmode=require`, `sslmode=verify-ca`, or
  `sslmode=verify-full` query parameter; look-alike values such as
  `verify-fullx` are rejected.
- Generic LDAP, AD DS, and UCS endpoint checks now require a non-empty,
  whitespace-free authority after the expected TLS scheme.
- UCS page size is bounded by the documented `1..=5,000` contract, matching
  Generic LDAP and AD DS.
- A live run queues rather than implies certification unless it explicitly
  requests `FULL_IMPORT=true` and `MAX_IMPORT_USERS=1000000`. Generic LDAP
  additionally queues the RFC 4533 row unless `FEED=rfc4533` is selected.

Focused evidence from the current environment:

```text
cargo test -p erpx-worker --test generic_ldap_live --offline -- --nocapture --test-threads=1
2 passed; 0 failed

cargo test -p erpx-worker --test directory_vendor_live --offline -- --nocapture --test-threads=1
6 passed; 0 failed
```

The live targets reported exact missing references for Generic LDAP, AD
DS/DirSync, and UCS Provisioning Service. No endpoint or credential was
available, so no vendor handshake, native feed, import, revocation, scale, or
rollback acceptance is claimed. The certification rows above remain queued.
