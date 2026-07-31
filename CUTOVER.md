# ERPX Development Cutover Record

Cutover completed on 2026-07-31 in the local development environment. This
record does not assert production readiness or external vendor certification.

## Recovery boundary

- Final pre-cutover source baseline:
  `cc0887dd3f16fe8dbdc827c1655750d9fafd91d2`
  (`checkpoint: preserve pre-ERPX cutover bridge state`).
- The former Next.js/Bun application, Node tooling, bridge source and worker,
  bridge configuration, active historical documentation, Folio database files,
  and Folio-only service templates were deleted from the active tree.
- Existing ERPX implementation and acceptance evidence was promoted to the
  root `audits/` directory.
- Source recovery is available from the baseline commit. Folio database and
  object data were intentionally deleted without archival; there is no data
  rollback.

## Bounded development reset

- Dropped only these schemas from local `folio_db`: `ai`, `auth`, `chat`,
  `finance`, `folio`, `hook`, `hr`, `inventory`, `law`, `perm`, and `platform`.
- Preserved the `n8n` and `public` schemas and all host-native services.
- Deleted only the local `folio-storage` MinIO bucket and created the
  `erpx-demo` namespace.
- Preserved `.runtime/n8n/`. Its encryption configuration SHA-256 was
  `bfe114069c47d741f8e8e2de89dfe857bc6f9064bf2a1e31e348eb2215656cf6`
  before and after reset.
- Created separate `erpx_control_write`, `erpx_control_read`,
  `erpx_tenant_write`, and `erpx_tenant_read` databases.
- `cargo xtask reset-dev` is loopback-only, schema/bucket allow-listed,
  target-printing, and gated by the literal `--confirm RESET_ERPX_DEV`.

## ERPX authority and fixture evidence

- Persisted command ownership is `erpx_only`, established by
  `cutover:migration-0066`.
- Immutable ownership evidence ID:
  `4e1a4e86-f821-5f73-84e7-04f30bcc055c`.
- Append-only outbox event:
  `erpx.command_ownership.established.v1`.
- Idempotent demo state contains 22 product contexts, one seed receipt,
  22 deterministic context outbox events, four native identities, and four
  finite role assignments. Secrets are hash-only.
- Directory connector count is zero. LDAP/RFC4533, AD DS/DirSync, and UCS code
  remains compiled, while development readiness is explicitly `disabled`;
  activation fails closed and is audited.
- Approved protobuf descriptor SHA-256:
  `01fd64d239f0161dfabbfdbcfbe739de8dcaea6b2291656dbd52b56e3f95ab38`.

## Verification

`cargo xtask verify` passed after the cutover. It serially ran:

- `cargo fmt --all --check`;
- workspace clippy with all targets, offline, and warnings denied;
- the complete non-ignored workspace test suite;
- all locally runnable ignored SQLx tests against fresh disposable databases,
  including identity, tenant isolation, provisioning, G13 import and
  reconciliation, outbox/projector convergence, Finance candidate activation,
  Operations, support, MFA, and five-minute revocation paths;
- Dioxus web, desktop, host-mobile, iOS, and Android checks, plus host and
  Android bridge checks;
- a real Dioxus web build;
- protobuf descriptor validation and the repository legacy-runtime scan; and
- live `/health`, `/ready`, and port 3004 smoke checks.

The only excluded ignored gates require infrastructure not configured in this
development profile: credentialed HTTPS MinIO backup/restore, isolated
WAL-archive PITR, and credentialed Generic LDAP/AD DS/UCS certification
environments. Their exclusion is explicit and no success is synthesized.

The active runtime is ERPX: Tonic on `127.0.0.1:50051`, auth/health HTTP on
`127.0.0.1:50052`, and the Dioxus console on `0.0.0.0:3004`. Protected HTTP
routes reject unauthenticated access with ERPX identity semantics, and no Node
or Folio process serves port 3004.
