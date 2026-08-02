# ERPX local data-plane and directory cutover evidence

Status: implemented and accepted for local development on 2026-08-01.

Scope: local development only. This record is not a production-readiness statement and does not claim Microsoft AD, OpenLDAP, or Univention vendor certification.

## Pre-cutover evidence

- Recorded Git baseline: `dfce4c3c2538f4d5f79be11d4a0e6ae9e046d074` on `codex/erpx-parallel-completion`.
- The source PostgreSQL cluster remains at loopback port `5432`; it was not deleted or repurposed.
- Source inventory contained **392** `erpx_*` databases totaling **4,570,158,200 bytes**. The earlier plan's 396-database figure was an assumption and was not reproduced by the live inventory.
- The migration manifest is retained outside Git at `.runtime/erpx/migration/manifest.json`, including per-database source size, dump SHA-256, and logical checksum evidence.
- The n8n encryption configuration SHA-256 before cutover was `bfe114069c47d741f8e8e2de89dfe857bc6f9064bf2a1e31e348eb2215656cf6`.
- EPSX MinIO remained outside ERPX orchestration on ports `9100/9101`.

## Active local data plane

- PostgreSQL 18 TLS cluster: `127.0.0.1:55432`, data root `/Users/fluke/erpx-postgres-data`.
- Verified WAL spool: `/Users/fluke/erpx-wal-spool`.
- HTTPS MinIO: API `9000`, console `9001`, data root `/Users/fluke/erpx-minio-data`.
- Allow-listed isolated restore root: `/Users/fluke/erpx-pitr-restore`; restore ports are restricted to `55440–55479`.
- All **392/392** source databases were migrated and checksum-verified. The four authoritative runtime databases are `erpx_control_write`, `erpx_control_read`, `erpx_tenant_write`, and `erpx_tenant_read`; the other **388** databases are archived development fixtures and are denied to the application role.
- Tenant objects, logical backups, and platform physical PITR use distinct MinIO namespaces.
- ERPX runtime storage readiness requires a successful certificate-validating HTTPS probe.

## Directory capability evidence

- OpenLDAP uses real LDAPS and RFC 4533 controls on local port `10636`.
- Samba AD DS uses real LDAPS and DirSync controls on local port `11636`.
- Both connectors completed TLS/bind/feed preflight, bounded import, role mapping, explicit canonical-principal linking, native cursor restart, and directory-user authentication.
- The shared demo identities are explicitly bound to canonical principal `local:demo-operations`; no username/email auto-merge path is used.
- UCS is persisted as `supported / unconfigured`; it has no worker or synthetic endpoint, and activation without a genuine Provisioning Service is rejected and audited.
- Directory secrets are referenced by persisted `credential_ref` values and resolved from ignored private files. Secret bundle values and CA bytes are not accepted by the directory administration RPCs or console.

## Physical PITR evidence

- Completed rehearsal: `68059dc2-092a-4dd5-9bbc-1851ea934b78`.
- Cluster/chain: `erpx-local-pg18` / `local-chain`.
- Base backup object: `platform/erpx-local-pg18/chains/local-chain/base/68059dc2-092a-4dd5-9bbc-1851ea934b78.tar.gz`.
- Base backup size: **525,937,224 bytes**; SHA-256: `ff97c234f07f1190321f46c5b58f529ee325c0316149688ad81fc1d16ce7770c`.
- The rehearsal verified `pg_verifybackup`, all 392 database names, runtime migrations, command ownership, tenant routing, forced RLS, exact LSN recovery, exact timestamp recovery, isolated ports, and cleanup.
- Target LSN: `0/F6002000`; target timestamp: `2026-07-31T18:08:37.899145+00:00`.
- Restores used ports `55440` and `55441` and were cleaned after verification.
- Tenant `kind=pitr` remains rejected; physical recovery is cluster-scoped.
- Promotion remains local-only, requires the runtime to be stopped and literal confirmation `PROMOTE_ERPX_PITR`, and cannot accept a caller-supplied path, URL, or port.

## Preservation and rollback

- The source data plane remains unchanged as the immediate rollback source.
- Neither data plane is deleted by cutover or stop commands.
- `.runtime/n8n/` and its encryption configuration remain preserved.
- `cargo xtask status` and `cargo xtask stop` operate only on recorded ERPX processes and the dedicated ERPX infrastructure.

## Acceptance verification

- `cargo fmt --all --check` passed.
- `cargo clippy --workspace --all-targets --offline -- -D warnings` passed.
- `cargo test --workspace --offline --no-fail-fast` passed.
- `cargo xtask verify` passed, including the serialized SQLx acceptance suite, descriptor baseline, native/web/desktop/mobile target checks, Dioxus web build, and runtime HTTP smoke checks.
- A real deny-wins exercise suspended Generic LDAP and observed both LDAP and AD login paths denied, then restored the connector and observed both paths healthy.
- Native login remained available to an unjoined principal and was rejected for the explicitly directory-linked demo principal. Platform Root linking was rejected and audited.
- The SQLx acceptance harness uses the dedicated administrative role only for disposable DDL/RLS fixtures; detached ERPX runtime processes continue to use the least-privilege application role.
