# G14 parallel-completion evidence package

Date: 2026-07-27  
Mode: independent, read-only audit

## Verdict

**ERPX is not G0–G14 gate-complete.** This package is an acceptance matrix and
reproducible evidence index. It does not promote packet-level implementation,
compilation, unit tests, or fail-closed seams into completion evidence.

The coordinator's 2026-07-27 ledger entry records that the directory/product
parallel wave did not close any G1–G14 gate. The current plan still requires
direct, independently checked evidence for every requirement, all Dioxus
targets, migration rollback, and final Folio write retirement.

## Evidence rules

A gate is green only when the evidence covers the authoritative behavior in
the plan: domain rules, SQLx write/read persistence, transactional outbox and
events, worker/projector and rebuild behavior, authorization, API, UI,
reconciliation, tests, and rollback where applicable. Mocks, schema-only
checks, compilation, and historical packet claims are insufficient by
themselves. Missing external systems are blockers, not passes.

Status meanings used below:

- **OPEN** — some implementation or packet evidence exists, but the gate's
  acceptance contract is not proven.
- **BLOCKED** — a named external or required environment is unavailable.
- **RE-AUDIT REQUIRED** — evidence exists, but it is historical or narrower
  than the current gate and must be rerun against the current tree.

## G0–G14 acceptance matrix

| Gate | Concrete acceptance contract | Direct evidence currently available | Missing proof / finding | Verdict |
|---|---|---|---|---|
| G0 | Reproducible current-state audit; deterministic migration fixtures; semantically compatible command/query/event/error/holding/actor/audit/idempotency contracts; generated protobuf compiles in server and Dioxus targets; all baseline failures explained. | `MASTER_PLAN.md` G0 section; `audits/G0-001_CURRENT_STATE_AUDIT.md`; `audits/G0-CURRENT-RE-AUDIT/README.md`; `audits/G0-001/evidence/`; `crates/migration`; `CONTRACT_POLICY.md`. | The current-tree re-audit and local compatibility checks are present. The independent protobuf descriptor comparison remains blocked because no approved baseline is configured; external/vendor/device, migration, and production evidence remain open elsewhere in the matrix. | **OPEN / BLOCKED** |
| G1 | Restart preserves authoritative data; no production in-memory repository; atomic write/outbox; worker claims, checkpoints, retries/DLQ, idempotent projectors, rebuild and replacement-read activation; boundary/auth/readiness tests; web/desktop/mobile target evidence as required. | `audits/G1-001/evidence/`, `audits/G1-003/evidence/`, `audits/G1-004/evidence/`, `audits/G1-005/evidence/`; plan G1 gate. | The plan explicitly leaves production mutation adoption, context handlers, physical replacement-read switching, complete boundary adoption, server authorization boundaries, and authenticated route integration open. | **OPEN** |
| G2 | No permanent role assignment; exact half-open expiry/cascade; every mutation uses authoritative effective access; MFA/high-risk approvals; role review delivery; approver identity; recovery receipts; immutable role history and revocation. | `audits/G2-IAM-003/`, `audits/G2-IAM-004/`, `audits/G2-IAM-005/`, `audits/G2-IAM-006/`, `audits/G2-IAM-007/`, `audits/G2-IAM-008/`; directory RBAC code/tests. | Existing evidence still leaves complete universal mutation adoption, worker delivery, approver/recovery integration, and full end-to-end authorization evidence open. Directory role mapping remains a coordinator/shared-RBAC integration boundary. | **OPEN** |
| G3 | Every global read is authenticated, scoped, customer-visible in the audit, redacted, and rebuildable from authoritative sources; Customer 360/global-read flows are complete. | `audits/G3-001/`, `audits/G3-002-003-CUSTOMER360-PROJECTIONS/`, `audits/G3-005-006-READMODELS-002/`, `audits/G3-006-CUSTOMER-360-TRANSPORT-049/`, `audits/G3-006-CUSTOMER360-PAGES/`, `audits/G3-CUSTOMER360-NOTES-READ/`, `audits/G3-CUSTOMER360-AUDIT-PAGING/`. | The plan leaves application/client adapters, multi-holding semantics, audit/notes backfill, binding revocation reconciliation, and authenticated deep-link/mobile surfaces open. The new packets close only local typed page/cursor adapters. | **OPEN** |
| G4 | Support authority is the intersection of identity, role chain, capability, scope, module, entitlement, billing, policy, SoD, represented-user authority, device/session state, and revocation; ticket/incident/diagnostics workflows are persisted, projected, transported, audited, and usable. | `audits/G4-SUP-008/`, `audits/G4-006-SUPPORT-INTERSECTION-005/`, `audits/G4-007-DIAGNOSTICS-REDACTION-018/`, `audits/G4-008-INCIDENT-TRANSPORT-032/`, `audits/G4-UI-INCIDENT-READ/`, `audits/G4-UI-DIAGNOSTICS-READ/`, `audits/G14-QA-G4G7-037/evidence/`. | Independent QA still lists approval containment, session/device resolution, cascades/workers, capability-bound diagnostics, incident projection/notifications, universal enforcement, authenticated Operations/support UI, customer-support journeys, and Folio reconciliation. The new diagnostics packet closes only a local platform read adapter. | **OPEN** |
| G5 | Stripe signatures are verified over exact raw bodies; duplicates/out-of-order events are harmless; catalog and entitlement state reconcile to Stripe; provider identity/API pinning is enforced; authorized mutation recovers after provider failure. | `audits/G5-002-STRIPE-CATALOG-WEBHOOK-041/`, `audits/G5-002-OFFICIAL-ADAPTER-003/`, `audits/G5-003-004-STRIPE-SEMANTIC-RECONCILIATION/`, `audits/G5-004-BILLING-COMMANDS-046/`; ledger Stripe verification entries. | Official generated client/API synchronization, live provider execution, remote catalog/entitlement reconciliation, authenticated replay controls, worker composition, and complete billing command adoption remain open. | **OPEN** |
| G6 | Holding registry, physical write/read provisioning, forced RLS, health/readiness, replacement-read rebuild/activation, PostgreSQL backup/PITR/restore, MinIO namespace/object integrity, retention, lifecycle, and Platform Operations are proven with real routed databases and rollback. | `audits/G6-PRV-005-059/` through `audits/G6-PRV-033/`; `audits/G6-G12-UI-010/` through `audits/G6-G12-UI-015/`; `audits/G6-SCHEDULER-REGISTRY-FENCE/`; `audits/G14-QA-061/` and `audits/G14-QA-062-FOLLOWUP/`. | The local scheduler lifecycle race is now fenced and current local persistence tests pass. Credentialed MinIO proof, true PostgreSQL PITR/WAL recovery, complete restore/candidate activation journeys, remaining Platform Operations actions, and independent full scheduler/projector acceptance remain open. | **OPEN / BLOCKED** |
| G7 | Operations is orchestration only; source-context events are idempotent and ordered; branch/holding isolation holds; exception/SLA/reassignment workflows, worker consumers, authenticated routes/UI, and Folio/ERPX totals reconcile. | `audits/G7-COMPLETION/`, `audits/G7-001/`, `audits/G7-002/`, `audits/G7-004-TRANSPORT/`, `audits/G14-QA-G4G7-037/evidence/`. | The bounded source-outbox detection, active-queue reassignment, due-SLA escalation, and transactional durable notification-handoff paths plus local SQLx persistence are current-tree evidence. Automatic policy publication, manager authority evaluation, downstream provider/device delivery, and Folio reconciliation remain open. | **OPEN** |
| G8 | Balanced journals; enforced period locks; AP/AR, cash/bank, budgets/SoD, consolidation/eliminations, Thai VAT/WHT/statutory reports, statements; source/read reconciliation and rollback are proven. | `audits/G8-COMPLETION/`, `audits/G8-FIN-011/` through `G8-FIN-048/`; `audits/G8-UI-FINANCE-JOURNAL-READ/`; `audits/G8-UI-FINANCE-CHART-READ/`; Finance domain/infrastructure/worker packet evidence. | Local typed Finance journal, close-review, subledger, banking, and chart read routes are present, but consolidation, Thai tax, provider certification, SoD/approval workflow, live two-database reconciliation, rollback, and full Finance completion remain open. | **OPEN / BLOCKED** |
| G9 | End-to-end Lead-to-Cash and Procure-to-Pay; customer/vendor and inventory workflows; weighted-average/recosting; accounting reconciliation; correct Operations events; transport, worker, read model, UI, and rollback evidence. | `audits/G9-SALES-001-052/`, `audits/G9-SALES-002-SQLX-054/`, `audits/G9-SALES-003-056/`, `audits/G9-SCM-002-052/`, `audits/G9-SCM-003-055/`, `audits/G9-SCM-004-056/`, `audits/G9-UI-SALES-READ/`, `audits/G9-G11-UI-PRODUCT-AGGREGATE-READ/`, `audits/G9-G11-COMPLETION/`. | G9 typed Sales/SCM/Asset transport, worker branches, bounded read models, and local read-only UI adapters are source-verified; full Lead-to-Cash/Procure-to-Pay workflow, logistics breadth, durable live scheduling, routed two-database reconciliation/rollback, and accounting/Operations acceptance remain open. | **OPEN / BLOCKED** |
| G10 | Consent and suppression are checked at schedule and delivery; retries/callbacks are idempotent and authenticated; custom domains are holding-bound; journeys, CRM, attribution, provider/live delivery, UI, and Finance revenue reconciliation are complete. | `audits/G10-MKT-046/` through `audits/G10-MKT-074/`; `audits/G9-G11-COMPLETION/`; ledger Marketing verification waves. | The packets explicitly leave vendor-native/live provider evidence, journeys/CRM UI, attribution-to-Finance reconciliation, some public/SSR/MinIO surfaces, and final external provider acceptance open. Existing G14 scheduler/read-projection findings also require a current independent re-audit. | **OPEN / BLOCKED** |
| G11 | Asset/GL and maintenance/custody reconcile; HR/payroll is isolated and authorized; Legal retention/version/access history works; AI/integrations cannot bypass authorization; POS, Projects, Service, Manufacturing, Quality, and remaining contexts are independently exercised end to end. | `audits/G11-AST-001-052/`, `audits/G11-AST-002-055/`, `audits/G11-AST-003-057/`, `audits/G11-PEO-001/` through `G11-PEO-004/`, `audits/G11-LEGAL-001/`, `audits/G9-UI-SALES-READ/`, `audits/G9-G11-UI-PRODUCT-AGGREGATE-READ/`, `audits/G11-UI-HR-EMPLOYEE-READ/`, `audits/G11-UI-LEGAL-CONTRACT-READ/`, `audits/G11-UI-POS-SALES-READ/`, `audits/G11-PRODUCT-CONTEXT-COMPLETION/`, `audits/G9-G11-COMPLETION/`. | The current tree has typed API/server authorization/read routing, read-only console surfaces, and all seven product projection registrations. Durable scheduling execution, MinIO/signed access, payroll/legal workers, live two-database reconciliation/rollback, vendor/device acceptance, and remaining ERP workflows are not complete evidence. | **OPEN** |
| G12 | Required role journeys on Web, desktop, Android, and iOS; secure local session storage; deep links, notifications, offline/conflict recovery, accessibility, localization, and mobile approvals/Operations/product surfaces. | `audits/G12-UI-QA-COMPLETION/`; `audits/G6-G12-UI-008/` through `G6-G12-UI-015/`; `.github/workflows/erpx-targets.yml`; `apps/console/src/product_completion.rs`. | Cross-target compilation is documented, but device execution, Android Keystore/iOS Keychain, camera/scanner, notifications, offline drafts, and complete role journeys remain explicitly open. | **OPEN / BLOCKED** |
| G13 | For every context: freeze, immutable ID map, write backfill, read projection, shadow replay, explained reconciliation, read-only pilot, security/workflow acceptance, rollback rehearsal, mutation freeze/delta import, approved command-owner switch, Folio read-only retention, and retirement. | `audits/G13-MIG-001/`, `audits/G13-MIG-002/`; `apps/migrator`; plan G13 sequence. | Current evidence is a read-only/dry-run CLI and extractor only. No live cohort backfill, shadow comparison, rollback rehearsal, mutation freeze, command-owner persistence/switch, Folio read-only transition, or retirement evidence exists. | **OPEN** |
| G14 | Independent hardening proves role boundaries/cascade/isolation, support approval, global-read disclosure/redaction, Stripe/consent/object/session/capability controls, outbox/projector/DLQ/rebuild/restore/rollback/reconciliation, journey resume, Operations recovery, every product workflow, and final cutover prerequisites. | `audits/G14-QA-061/`, `audits/G14-QA-061-FOLLOWUP/`, `audits/G14-QA-062/`, `audits/G14-QA-062-FOLLOWUP/`, `audits/G14-QA-G4G7-037/evidence/`. | Existing G14 audits are explicitly not gate-complete; follow-ups retain open scheduler composition/idempotency, callback/read-projection, registry race, support/Operations, device, vendor, migration, and Folio-retirement findings. This package is not a substitute for the final audit. | **OPEN** |

## Directory vendor and identity acceptance blockers

The local implementation contains real typed persistence and fail-closed
protocol seams; it does not contain a vendor acceptance run. The worker
registration refuses to claim directory work without a real adapter. The
following certification IDs are queued by `audits/DIRECTORY-SYNC/README.md`:

| ID | Vendor/protocol | Required real-environment acceptance | Current finding |
|---|---|---|---|
| CERT-DIR-LDAP-001 | Generic LDAP with RFC 4533 | LDAPS and StartTLS certificate/hostname validation; paged full import; stable ordering; RefreshOnly/RefreshAndPersist cookie continuity; disable/delete; timeout/reconnect; rollback and rebuild. | No credentialed generic LDAP server is available. **Not certified.** |
| CERT-DIR-AD-001 | On-prem Microsoft Active Directory Domain Services with DirSync | DirSync rights; real cookie across restart; USN advancement; disable/delete/rename; ACL denial; cookie invalidation; exact objectGUID immutable link; revocation measurement. | No on-prem AD DS environment/credential is available. **Not certified.** |
| CERT-DIR-RFC4533-001 | RFC 4533 native change feed | RefreshOnly and RefreshAndPersist; refresh-required response; deletion state; cursor persistence/replay; TLS outage/reconnect. | No RFC 4533 server is available. **Not certified.** |
| CERT-DIR-UCS-001 | Univention Corporate Server Provisioning Service | HTTPS service credential; token/sequence restart; disable/delete; replay/order; service outage/reconnect; immutable entryUUID link. | No UCS environment/credential is available. **Not certified.** |
| CERT-DIR-SCALE-001 | All three connector families | One million-user bounded-memory import; page restart; duplicate suppression; cursor continuity; source/canonical/outbox/read-model reconciliation; staged rollback; no link rebinding. | No live volume or routed disposable-directory proof is recorded. **Not certified.** |
| CERT-DIR-REVOKE-001 | All three connector families | Disable/delete native event causes access cut-off within five minutes, including stale-feed fail-closed behavior, append-only role revocation facts, and measurable timestamps. | Local SLA types/tests exist; no real-feed timing run is recorded. **Not certified.** |

Implemented locally but still requiring integration evidence: platform and
holding connector scope, TLS-only communication, high-risk MFA, local-only
break-glass, immutable identity linking, append-only directory-managed role
facts, SQLx import/checkpoint/member persistence, redacted source outbox, and
read-model reconciliation. The local PostgreSQL fixture now exercises
resumable full-import finalization with a grouped active user, grouped
disabled user, ungrouped user, three append-only role facts, and three hydrated
identity projections; both ignored directory SQLx tests passed against a fresh
disposable PostgreSQL database on 2026-07-28, which was dropped after the run.
Directory activation must remain disabled until all applicable certification
rows are green.

## Rollback and cutover guardrails

- Never activate a customer directory, change production command ownership,
  freeze Folio writes, retire Folio writes, or perform final cutover from this
  audit package. Each requires explicit product-owner approval and completed
  G13/G14 evidence.
- Directory rollback before terminal import finalization may delete only
  staged import members and mark the import `rolled_back`. After finalization,
  rollback is compensating/rebuild work: preserve immutable links, role facts,
  revocation facts, audit history, and outbox events.
- No identity link may be rebound after conflict. A conflicting immutable
  provider key blocks the import and queues reconciliation.
- Role grants and revocations are append-only. Disable/delete or stale-feed
  uncertainty cuts effective access fail-closed; it must not erase role
  history or silently reactivate a user.
- Directory transport is LDAPS or StartTLS with validated certificates and
  hostname/protocol policy only. There is no plaintext or remote-directory
  fallback. Break-glass is local-only, separately audited, and MFA-backed.
- Read models may be cleared/rebuilt only from authoritative write-side
  events. A non-green source/read reconciliation blocks activation and keeps
  the read surface stale/read-only until repaired and independently checked.
- Migration cutover must follow the ordered G13 state machine: source freeze,
  ID map, backfill, projection, shadow replay, explained diff, read-only pilot,
  workflow/security acceptance, rollback rehearsal, final delta, owner switch,
  Folio read-only retention, then retirement after the approved retention
  window.

## Required serialized verification

Run from `erpx/` in this order; full Cargo verification is coordinator-owned
and must not be inferred from focused packet runs:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace -- --ignored

# Generation/compile is not compatibility proof; compare the generated
# descriptor set with the approved prior descriptor using the repository's
# approved protobuf compatibility checker.
cargo check -p erpx-proto

cargo check -p erpx-console --no-default-features --features web
cargo check -p erpx-console --no-default-features --features desktop
cargo check -p erpx-console --no-default-features --features mobile
cargo check -p erpx-console --target aarch64-apple-ios --no-default-features --features mobile
cargo check -p erpx-console --target aarch64-linux-android --no-default-features --features mobile
```

The ignored database suite requires disposable, physically distinct database
URLs. Current tests use combinations of
`ERPX_TEST_CONTROL_WRITE_DATABASE_URL`, `ERPX_TEST_CONTROL_READ_DATABASE_URL`,
`ERPX_TEST_HOLDING_WRITE_DATABASE_URL`, `ERPX_TEST_HOLDING_READ_DATABASE_URL`,
`ERPX_TEST_ADMIN_DATABASE_URL`, `ERPX_TEST_HOLDING_DATABASE_URL_TEMPLATE`,
`ERPX_TEST_MARKETING_SCHEDULER_DATABASE_URL`, and the support-specific
`ERPX_SUPPORT_TEST_DATABASE_URL`. Supply only disposable values, redact them
from logs, run all ignored tests, inspect forced-RLS and cleanup, and report
every ignored test explicitly. A passing schema application is not a live
behavior proof.

From the Folio root, still run the required non-production-build checks:

```sh
bun run lint
bunx tsc --noEmit
curl -sS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:3004/operations
```

The expected Folio probe is an authentication redirect (normally `307`), and
the Next development server must remain running on port `3004`. Do not use a
production Next.js build as verification.

## Reproducible read-only check

Run:

```sh
./erpx/audits/G14-PARALLEL-COMPLETION/check_read_only.sh
```

The script checks only file existence, source wiring markers, required audit
references, command availability, audit-package scope, current branch, and an
optional local Folio HTTP status. It does not read secrets, connect to LDAP,
AD DS, UCS, PostgreSQL, MinIO, Stripe, or n8n; it does not mutate databases,
runtime state, migrations, registrations, or production ownership. A green
script result is structural evidence only, not a gate-completion claim.

## Exact package files

- `erpx/audits/G14-PARALLEL-COMPLETION/README.md` — this matrix, blockers,
  guardrails, and verification contract.
- `erpx/audits/G14-PARALLEL-COMPLETION/check_read_only.sh` — executable,
  non-mutating structural/runtime probe.

No source, manifest, lockfile, protobuf, migration, registration, shared
authorization, console shell, `MASTER_PLAN.md`, or `EXECUTION_LEDGER.md` file
was changed by this lane.

## Coordinator continuation — vendor acceptance blockers — 2026-07-27

Generic LDAP, AD DS/DirSync, and UCS acceptance tests now call the real
directory worker path when configured and enforce TLS-only transport, bounded
one-million-user imports, and explicit database/CA/server references. The
workspace run safely queued the exact missing references for all three vendor
lanes; no live certification pass was claimed. Local directory contracts,
SQLx persistence, outbox, worker, projection, authorization, and five-minute
revocation tests remain green. Production activation and cutover remain
unauthorized and were not performed.

## Coordinator current-state revalidation — 2026-07-27

The current tree’s serialized verification is now recorded in the coordinator
ledger: workspace formatting, warnings-denied Clippy, non-ignored and ignored
local SQLx targets, protobuf descriptor comparison execution, all Dioxus
targets, Folio lint/typecheck, authorization-route verification, and the live
port-3004 protected-route probe pass. The ignored run’s only failing target is
the deliberate credentialless G14 external-readiness preflight.

The protobuf comparator is executable and rejects incompatible descriptor
changes when an approved baseline is supplied; no approved baseline is
available here, so that comparison remains queued. Generic LDAP/RFC 4533, AD
DS/DirSync, and UCS Provisioning Service certification, measured scale and
revocation runs, physical device evidence, and G13 live Folio migration and
cutover evidence remain open. This package still does not authorize activation
or cutover.

## Coordinator current-tree verification — 2026-07-28

The coordinator reran the serialized current-tree checks after the G8 Finance
chart and G11 HR employee, Legal contract, and POS sales read adapters:

- `cargo fmt --all --check`, workspace Clippy with warnings denied, and
  `cargo test --workspace --offline --no-fail-fast` passed.
- The ignored database matrix passed every configured local disposable
  PostgreSQL target. The only nonzero target was the deliberate
  credentialless external-readiness preflight, which queued the exact missing
  Generic LDAP, AD DS/DirSync, and UCS references; no external certification
  pass was claimed.
- `cargo check -p erpx-proto --offline`, protobuf package tests, the G13
  descriptor-comparator acceptance, all Dioxus web/desktop/host-mobile/iOS/
  Android checks, `git diff --check`, and this structural probe passed. The
  descriptor comparator ran and left the comparison queued because the
  approved descriptor baseline is absent.
- Folio lint/typecheck passed, the protected port-3004 `/operations` probe
  returned 307, and no production Next.js build was run.

This is verification evidence, not G0–G14 closure. Vendor certification,
measured scale/revocation, approved descriptor comparison, physical mobile
execution, live Folio migration/cutover/retention/reconciliation, and explicit
production approval remain open. Activation, command-owner switching, Folio
write retirement, and production cutover remain unauthorized and were not
performed.

## Coordinator local SQLx re-audit — 2026-07-28

The coordinator reran the complete ignored workspace command with fresh,
disposable local PostgreSQL databases for the configured control-plane,
holding, commercial, scheduler, support, directory, and G13 source/target/
rollback lanes. The temporary databases were removed by the test-run cleanup
trap after completion.

- PASS: every configured local SQLx acceptance target, including finance
  candidate activation, backup retention/blocker behavior, customer 360,
  support expiry, Operations source-feed reconciliation, and the configured
  migration/import lanes.
- PASS: `operations_completion_authoritative::source_reconciliation_is_authoritative_and_scope_isolated`
  consumed one complete source-owned outbox envelope and rejected the wrong-
  holding envelope through the authoritative scope fence.
- BLOCKED/QUEUED: the sole nonzero target was
  `g14_external_certification_preflight_fails_closed_when_not_ready`. It
  named the missing live Generic LDAP/RFC 4533, on-prem AD DS/DirSync, and
  UCS Provisioning Service endpoints, credentials, CA/server identities,
  import/revocation observations, and evidence paths. No vendor, one-million-
  user, or five-minute-revocation certification was claimed.

This local re-audit strengthens database and worker evidence only. The
approved protobuf baseline, physical mobile execution, live Folio migration /
rollback / retention / cutover evidence, and explicit production approval
remain open. No directory activation or production state changed.

## Coordinator continuation — G6 PITR metadata persistence — 2026-07-28

The current tree now durably admits a validated PITR recovery point through
the platform contract/API, control write migration `0062`, control-read
migration `0013`, idempotency hash, backup job/run records, and worker claim
binding. A fresh disposable PostgreSQL acceptance persisted and projected
1,001 full jobs plus one PITR job and cleaned both databases.

This narrows, but does not close, G6: physical WAL archive capture, physical
cluster recovery to the requested point, and restored-cluster verification
remain fail-closed pending a credentialed executor. No production activation,
command-owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G7 expired-assignee scheduling — 2026-07-28

The Operations completion worker now drains the bounded active-exception read
model after its current event projection pass and builds reassignment requests
only from coordinator-supplied expired-principal rules. Each rule carries an
explicit replacement, manager, manager-chain proof, MFA-backed authorization,
and actor attribution; duplicate principals, scope escape, inactive proofs,
page exhaustion, queue overflow, and invalid identities fail closed. Request
idempotency and correlation identities are deterministic from exception ID,
read-model version, expired principal, and replacement principal. The write
side still enforces the expected version and expired-assignee guard before
appending the reassignment event/outbox row.

Unit tests and the ignored fresh-PostgreSQL Operations acceptance passed. The
acceptance projected the real assignment event, reproduced the same request on
two scans, persisted the reassignment through SQLx, and verified the updated
aggregate. This closes only the local bounded scheduler slice; identity
expiration evaluation, manager authority publication, authenticated route/UI,
notifications, Folio parity, and live vendor evidence remain open.

## Coordinator post-reassignment verification — 2026-07-28

- PASS: serialized formatting, workspace warnings-denied Clippy, and the
  complete non-ignored workspace tests passed after the scheduler change.
- PASS: the fresh-database ignored matrix passed every configured local SQLx
  target, including Operations detection/source reconciliation and the
  projection-backed reassignment scheduler acceptance.
- BLOCKED/QUEUED: the sole nonzero target was the deliberate external
  certification preflight; Generic LDAP/RFC 4533, on-prem AD DS/DirSync, and
  UCS endpoints, credentials, CA/server proofs, import observations,
  revocation observations, and evidence paths remain unavailable. No vendor
  certification or G0–G14 closure is claimed.

## Coordinator continuation — G7 due-SLA escalation scheduling — 2026-07-28

The Operations completion worker now scans the bounded active-exception read
model after projection and derives escalation requests only when a persisted
exception is assigned, due at or before the sweep time, and not already
escalated. The coordinator supplies the assigned-principal rule, manager
reference, manager-chain proof, MFA-backed authorization, and actor
attribution. Duplicate rules, scope escape, inactive proofs, missing due
times, pagination/queue exhaustion, and unsafe identities fail closed.
Escalation idempotency and correlation keys are bound to the exception,
aggregate version, and persisted due time; SQLx expected-version and due-time
checks remain authoritative before the append-only event/outbox transition.

The ignored fresh-PostgreSQL Operations acceptance now projects the assignment
and reassignment events, reproduces the escalation request on two scans,
persists the escalation, and verifies the aggregate status. This is local
scheduler/persistence evidence only; policy publication, notification
delivery, authenticated UI integration, Folio parity, and live certification
remain open.

## Coordinator continuation — G7 transactional SLA notification delivery — 2026-07-28

Escalation now enqueues a forced-RLS notification in the same holding-write
transaction as the lifecycle event, snapshot, and outbox. The registered
worker claims due rows, writes one deterministic
`operations.sla_notification_delivered.v1` handoff, acknowledges the exact
lease, and leaves failed provider handoffs retryable. The fresh disposable
PostgreSQL acceptance passed enqueue, claim, handoff, acknowledgement, and
replay without a duplicate event. This is local durable handoff evidence;
external provider/device delivery remains unconfigured and is not claimed.

## Coordinator continuation — physical PITR capture boundary — 2026-07-28

The new infrastructure capture adapter is registered in the crate API and
covered by focused tests. It uses the real `pg_basebackup` process boundary,
bounded non-symlink WAL archive traversal, MinIO put/get verification, and an
append-only recovery manifest. Restore remains an explicit unconfigured seam;
the absence of a physical replay verifier keeps worker execution fail-closed.

## Coordinator continuation — physical PITR replay executor seam — 2026-07-28

The infrastructure API now exposes a concrete, unregistered PostgreSQL replay
executor with resolver-owned target paths, manifest/object verification,
physical base/WAL rehydration, recovery configuration, `pg_ctl` lifecycle, and
`psql` recovery-mode verification. The default resolver remains fail-closed;
credentialed restore/replay and production certification are still queued.

## Coordinator verification delta — final local gates — 2026-07-28

- PASS: serialized format, workspace warnings-denied Clippy, complete
  non-ignored workspace tests, focused PITR/backup SQLx acceptance, protobuf
  and G13 registration, all Dioxus checks, Folio lint/typecheck, and the
  port-3004 guard.
- QUEUED: credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS, approved
  descriptor, physical mobile, physical PITR restore/replay, live Folio
  migration/retention/cutover, and explicit production approval. G0–G14 are
not claimed 100% closed; no activation, owner switch, Folio write retirement,
or production cutover occurred.

## Coordinator continuation — G12 native Android secure-session implementation — 2026-07-28

The mobile target gap is narrowed with a real Android implementation: a target-owned JNI bridge
uses the Dioxus/NDK context, AndroidKeyStore AES-GCM, and private SharedPreferences for bounded
encrypted session persistence. The console restores, saves, rotates, and clears supported-target
sessions through that adapter while unsupported targets remain fail-closed. UI tests and Android,
iOS, web, and desktop cross-target checks pass.

This does not certify a mobile device. Android SDK/NDK and ADB are unavailable locally, no Android
emulator is available, and no iOS simulator is available. Device Keystore/Keychain execution and
remaining native mobile journeys remain queued; G0–G14 are not claimed complete. No production
activation, owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G8 durable bank-provider scheduling — 2026-07-28

The G8 Finance lane added a real SQLx control source for authorized bank
statement imports. It persists bounded opaque request/authorization envelopes,
source-fenced leases and claim history, retry/dead-letter state, and append-only
reconciliation/rollback evidence. A separate rebuildable read migration
projects status only. Completion requires exact authoritative holding
transaction and import-outbox counts; the default worker registry stays
opt-in until provider, credentials, authorization, and routing are injected.

The disposable two-database PostgreSQL acceptance passed for enqueue
idempotency, status projection, claim/retry/reclaim, rollback evidence, and
reconciled completion. Focused worker/infrastructure tests and Clippy passed.
Provider SDK/credentialed live acceptance and the broader G8 gate remain open;
no external vendor pass is claimed. No directory activation, owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator continuation — G9–G11 local SQLx revalidation — 2026-07-28

Disjoint disposable holding write/read database pairs passed the available
Sales 2/2, SCM 5/5, HR/payroll 4/4, Legal 4/4, POS 5/5, and Marketing
CRM/delivery 5/5 acceptance targets. The runs supplied real source/outbox,
scoped read, idempotency, projection/rebuild, mismatch, rollback, and
retention evidence where defined. Temporary databases were removed after each
target. This does not certify external vendors, physical devices, complete
workflow reconciliation, or production controls; G9–G11 and G14 remain open.

## Coordinator verification — current tree after G8/G9–G11 packets — 2026-07-28

Serialized workspace format, warnings-denied Clippy, non-ignored tests,
protobuf/G13/G14 checks, Dioxus web/desktop/Android/iOS target checks, Folio
lint/typecheck, diff validation, and the port-3004 guard passed. New local G8
and G9–G11 disposable SQLx targets also passed and were cleaned up. The
aggregate ignored run still queues missing external/runtime resources;
credentialed directory certification, approved descriptor, scale/revocation,
physical mobile, MinIO/PITR, live Folio migration/cutover, and production
approval remain open. No activation, owner switch, Folio write retirement, or
cutover occurred.

## Coordinator continuation — G8 authenticated status API — 2026-07-28

The Finance service now provides an authenticated status-only read for durable
bank-provider import jobs. It enforces explicit scope, `finance.read`, exact
holding/holding matching, bounded lifecycle fields, and audit recording without
exposing request, authorization, credential, or provider payloads. Server
checks, strict Clippy, and 85/85 server tests passed. Console status UI,
credentialed provider certification, and the wider G8/G14 gates remain open;
no activation, owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G9 Sales and SCM lifecycle commands — 2026-07-28

Authenticated, explicitly scoped lifecycle command seams now exist for Sales
and Procurement/Inventory. Both require the product write role, positive
optimistic source versions, bounded domain transitions, source routing, audit
receipts, and atomic source/snapshot/shared-outbox/idempotency persistence; the
Dioxus surfaces refresh only from typed read models after success. The SCM
domain also rotates command identity on Purchase Request/Order/Goods Receipt
transitions, preventing creation-key collisions. Fresh disposable PostgreSQL
acceptances passed Sales Lead qualification and SCM Purchase Request
submit/approve through projection, reconciliation, clear-and-replay rebuild,
and scope evidence.

These are bounded G9 advances. Full product workflow breadth, accounting and
Operations reconciliation, runtime restart/lease evidence, vendor/device
acceptance, and the G14 gate remain open. No activation, owner switch, Folio
write retirement, or production cutover occurred.

## Coordinator current-tree correction — 2026-07-31

The historical certification table above is superseded only where the current
tree has stronger evidence. Generic LDAP/RFC 4533 now has a real local
OpenLDAP/LDAPS acceptance: bounded one-million-user configuration, real two-user
full import, durable RFC 4533 cookie continuity, provider-side disable through
LDAP, native incremental observation, immutable-link preservation, durable
disabled projection/outbox evidence, and an exact five-minute revocation-SLA
row. No synthetic ERPX disable was inserted.

The real PostgreSQL 18/TLS and HTTPS MinIO backup and physical PITR acceptances
also passed, including restore/rollback/cleanup. UCS has real TLS UDM
authentication evidence, but the official UCS Provisioning Service app/backend
installation is license-gated; no subscription, native message acknowledgement,
or ERPX feed acceptance is claimed. The prior Samba AD-compatible probe is
compatibility evidence only and is not Microsoft AD DS certification.

Still open and fail-closed: credentialed Microsoft AD DS/DirSync, licensed UCS
Provisioning Service, approved protobuf descriptor provenance, physical
Android/iOS execution, measured external one-million-user and five-minute
revocation runs for all connector families, full product/workflow and migration
cutover evidence, and explicit production approval. No customer directory was
activated and no command-owner or Folio-write transition occurred.
