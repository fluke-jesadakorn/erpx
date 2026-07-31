# G14 local SQLx acceptance wave — 2026-07-28

## Scope

This evidence records the coordinator's local, database-backed acceptance
wave for the ERPX paths whose external resources were available. Each target
used an exact-name disposable PostgreSQL database or write/read pair created
from the host-native PostgreSQL 18 service. No Folio database, customer
database, directory endpoint, provider account, or production credential was
used.

## PASS evidence

The focused ignored targets passed for identity and directory SQLx/RBAC;
Support and attachment expiry; Customer 360 and access-audit projections;
role review; Finance banking/import/candidate/close/journal/chart/budget/
subledger; HR; Legal; Marketing content, CRM, delivery, provider callback,
journey, erasure, onboarding, and abuse; Operations; POS; SCM; organization;
tenant provisioning, replacement-readiness, registry, projection scheduler
and status; G13 migration and target import; control-plane SQLx; and worker
runtime paths. The worker and infrastructure library ignored tests were also
run with their required tenant/control URLs after the initial missing-variable
guard was corrected.

The real backup/restore acceptance passed with PostgreSQL 18 `pg_dump`/
`pg_restore` and a temporary HTTPS MinIO instance using a dedicated CA and
server certificate. The HTTP-only resident MinIO service was rejected by the
TLS-only boundary and was not used. The temporary bucket and test resources
were removed after the run.

The five-case directory volume suite processed the one-million-user bound in
51.60 seconds. This is local bounded algorithmic evidence for import,
checkpoint/replay, cursor, revocation, and local break-glass behavior; it is
not a claim of real LDAP/AD/UCS throughput or vendor certification.

## Isolation and rollback

The acceptance databases were disposable `erpx_goal_*` resources. Cleanup is
limited to those exact names; no broad PostgreSQL or filesystem cleanup is
permitted. The temporary MinIO data, certificate directory, bucket, and dump
artifacts were removed. No application migration, customer connector, command
owner, Folio write path, or production state was changed.

## Queued certification blockers

The following remain fail-closed and must be supplied by the environment owner
before certification can be accepted:

- credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, and UCS
  Provisioning Service endpoints with their disposable TLS evidence databases;
- approved protobuf descriptor baseline;
- provider-side one-million-user/five-minute revocation measurements;
- true PITR/WAL and production migration/rollback evidence;
- physical iOS/Android execution; and
- explicit approval for directory activation, command-owner switching, Folio
  write retirement, and production cutover.

Missing resources are blockers, not green acceptance results. The external
directory harnesses remain non-mock and fail closed when these references are
absent.

## Coordinator refresh — 2026-07-28

The local wave was rerun with fresh, exact-name databases per acceptance
target. The previously resource-gated local targets all passed, including the
10-test infrastructure library, identity SQLx persistence, Finance
consolidation, Marketing delivery projection/provider/scheduler, provisioning,
control-plane, support, role-review, worker recovery, server owner seeding,
and both G13 disposable PostgreSQL suites. Finance consolidation now persists
both authoritative company chart snapshots before posting the immutable
journals; the acceptance passes with two companies and balanced totals.

The backup round-trip passed using PostgreSQL 18 `pg_dump`/`pg_restore` and a
temporary HTTPS MinIO instance with a dedicated CA/server certificate. The
resident HTTP-only MinIO endpoint was rejected by the TLS-only policy and was
not used. All exact disposable databases created for this refresh were
removed after their evidence was captured.

The only remaining failed acceptance is the deliberate G14 external-readiness
preflight: no credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync,
or UCS Provisioning Service environment and revocation handoff evidence are
available. This remains a certification blocker, not a mock or a pass.

## Final serialized verification — 2026-07-28

The required serialized checks passed for formatting, warnings-denied
workspace Clippy, the complete non-ignored workspace suite, protobuf and G13
compatibility/recovery suites, all Dioxus target checks, Folio lint and
TypeScript, and the port-3004 health probes. The exact ignored workspace sweep
was also run without injecting resources and exited 101 with exactly 68
environment-gated targets. Those failures are expected fail-closed guards for
missing disposable URLs, MinIO/PITR inputs, approved descriptor data, or
credentialed external integrations; they are not acceptance passes.

Fresh exact-name database-backed reruns documented above passed the local
targets, including HTTPS-MinIO backup round-trip and the one-million-user
directory volume proof. External Generic LDAP/RFC 4533, AD DS/DirSync, and
UCS Provisioning Service certification remains queued until real environments
and operator revocation evidence are supplied.
