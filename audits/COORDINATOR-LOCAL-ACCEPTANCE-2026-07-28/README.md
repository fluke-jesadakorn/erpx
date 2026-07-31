# Coordinator local acceptance evidence — 2026-07-28

## Scope

This packet records real, isolated local acceptance runs completed by the
coordinator against disposable PostgreSQL databases and a disposable HTTPS
MinIO instance. No fake directory, provider, database, object store, or
process transport was injected.

## Passed evidence

- G6: PostgreSQL 18 `pg_dump`/`pg_restore` logical backup and restore through
  HTTPS MinIO with a trusted disposable CA, candidate forced-RLS verification,
  artifact inventory, deletion, and database cleanup.
- G13: disposable source/target/read/rollback migration acceptance and target
  import acceptance, including safe rerun and rollback evidence.
- G14 local: identity schema separation, session/device/recovery/passkey/TOTP
  persistence, directory scope rejection, local directory SQLx acceptance,
  and role-mapping scope rejection.
- G8: journal posting/replay, banking write/read, budgeting, chart write/read,
  close persistence/evidence/projection, subledger handoff/read, backfill,
  provider import jobs, and candidate activation.
- G9–G11: Operations, Sales, POS, SCM, Legal, Asset/Maintenance, HR Payroll,
  Marketing CRM/content/journey/delivery/consent/erasure/provider/onboarding,
  Customer 360, access audit, support expiry, role review, Stripe semantic
  handling, scheduler/projector routing, backup jobs/retention, platform
  completion, and tenant registry pagination.

All focused tests passed with fresh database names per test. Temporary
databases and the disposable MinIO instance were removed after the runs.

## Explicit blockers

The G14 external preflight still fails closed because no credentialed Generic
LDAP/RFC 4533, on-premises AD DS/DirSync, or UCS Provisioning Service
environment was supplied. The required approved protobuf descriptor baseline,
measured one-million-user/five-minute vendor evidence, true PITR/WAL,
physical mobile execution, and production Folio migration/cutover approval
also remain external acceptance gates. These are queued certification
blockers, not completion claims.

## Rollback

This packet changed no application schema, registration, authorization,
server ownership, worker ownership, customer directory, Folio write path, or
production state. The only temporary resources were disposable local test
databases, temporary TLS material under `/tmp`, and a temporary MinIO data
directory; all database and MinIO processes/resources were stopped or removed
after verification.
