# Deployment, Continuity, and Migration Requirements

## Deployment and runtime topology

### REQ-DPL-001 — Modular-monolith deployment

**Requirement.** ERPX shall remain a modular monolith deployed as a Console,
API server, durable worker, and optional customer data-plane agent rather than
one network service per bounded context.

**Rationale.** Business ownership can be enforced without adding distributed
transactions and customer-hosted service sprawl.

**Acceptance criteria.** Context boundaries are compile-time and contract
enforced; separately deployed processes use versioned contracts; no context
requires direct network calls to mutate another context; extraction requires
measurable scale, SLA, residency, or ownership evidence.

**Sources.** [Bounded-context decision](../../adr/ADR-014-cqrs-bounded-context-modular-monolith.md),
[workspace manifest](../../Cargo.toml).

### REQ-DPL-002 — Resumable bootstrap

**Requirement.** ERPX shall install a fresh schema-generation-v2 environment
through one governed, resumable bootstrap that validates non-secret resource
configuration, provisions new databases, and creates only the mandatory
initial authorities.

**Rationale.** Initial installation must be recoverable and must not import
unknown legacy authority.

**Acceptance criteria.** Bootstrap acquires an installation lock; records step
receipts; creates new control-write, control-read, first holding write/read
resources from migrations beginning at `0001`; refuses a conflicting or
non-v2 manifest; seeds only Root, Holding Owner, registry, canonical units, and
neutral brand; generated secrets are shown once and stored only as hashes;
pre-cutover databases and `.runtime/n8n/` are not mutated or deleted.

**Sources.** [Bootstrap application](../../apps/bootstrap/src/main.rs),
[fresh-state bootstrap](../../apps/bootstrap/src/fresh_state.rs),
[bounded-context decision](../../adr/ADR-014-cqrs-bounded-context-modular-monolith.md),
[FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md).

### REQ-DPL-003 — Verified runtime handoff

**Requirement.** ERPX shall start the clean-rebuild runtime only after
validating a write-once non-secret manifest for baseline 2.0.0, schema
generation v2, the 65-service catalog, singular routes, event namespaces,
database receipts, active tenant registry, rollback boundary, and n8n
configuration fingerprint.

**Rationale.** A manifest file alone must not become authority.

**Acceptance criteria.** Missing, retired, or mismatched catalog, generation,
receipt, registry, resource, authority, or fingerprint fails startup;
successful handoff skips legacy fixtures and data import; runtime
reconciliation never changes the preserved databases, n8n state, or encryption
key.

**Sources.** [Project runtime contract](../../README.md),
[cutover manifest](../../apps/bootstrap/src/cutover_manifest.rs),
[server migration composition](../../apps/server/src/migration.rs),
[FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md).

### REQ-DPL-004 — Web production delivery

**Requirement.** ERPX shall deliver the Web console as a same-origin,
optimized Dioxus application with protected BFF/session boundaries and a
verifiable production artifact.

**Rationale.** The primary channel needs reproducible performance and security
properties.

**Acceptance criteria.** Production build uses the pinned Rust/Dioxus toolchain;
one optimized WebAssembly module and its permitted static assets are produced;
credentials never enter asset URLs; BFF, HTTP health, and gRPC-Web boundaries
are smoke-tested before release.

**Sources.** [Project overview](../../README.md),
[Console Dioxus config](../../apps/console/Dioxus.toml),
[Web verification](../../xtask/src/wasm.rs).

### REQ-DPL-005 — Customer-hosted deployment equivalence

**Requirement.** ERPX shall require a customer-hosted data plane to pass
database, object, directory, worker, backup, restore, migration, agent, and
authorization preflight before activation.

**Rationale.** Data custody choice must not weaken product correctness or
recovery.

**Acceptance criteria.** Preflight validates matching typed contracts and
resource capabilities; customer approval is recorded; observed state matches
desired version; failed activation leaves the prior managed route authoritative
and retains recovery evidence.

**Sources.** [Customer data-plane decision](../../adr/ADR-011-customer-data-plane-agent.md),
[data-plane agent](../../apps/data-plane-agent/src/lib.rs),
[data-plane API](../../crates/proto/proto/erpx/platform_administration/v1/service.proto).

## Continuity and recovery

### REQ-BCP-001 — Scoped backups

**Requirement.** ERPX shall create tenant-, holding-, or branch-scoped backups
with immutable job identity, source boundary, encryption, checksums, retention,
and outcome.

**Rationale.** Recovery evidence must identify exactly what can be restored.

**Acceptance criteria.** Backup runs against an authorized physical route;
object and database inventory is captured; failed or partial jobs are not
reported complete; credentials remain out of job records; retention does not
delete an active or held recovery point.

**Sources.** [Platform Operations API](../../crates/proto/proto/erpx/platform_administration/v1/service.proto),
[backup runtime](../../apps/worker/src/backup_runtime.rs),
[backup tests](../../apps/worker/tests/backup_retention.rs).

### REQ-BCP-002 — Point-in-time recovery

**Requirement.** ERPX shall maintain and verify PostgreSQL point-in-time
recovery chains with bounded WAL retention and recoverable LSN/time markers.

**Rationale.** A database backup without a usable transaction-log chain cannot
meet precise recovery needs.

**Acceptance criteria.** Recovery points identify cluster, base backup, WAL
range, time, checksum, and expiry; gaps are detected; rehearsal restores to a
disposable target and verifies expected marker; cleanup is allow-listed and
does not affect active data.

**Sources.** [PITR infrastructure](../../crates/infrastructure/src/pitr.rs),
[cluster PITR worker](../../apps/worker/src/cluster_pitr_runtime.rs),
[recovery runbook](../../ops/runbooks/web-production-recovery.md).

### REQ-BCP-003 — Restore rehearsal

**Requirement.** ERPX shall rehearse database and object restoration before a
route, migration, or customer-hosted activation depends on it.

**Rationale.** Backup completion does not prove recoverability.

**Acceptance criteria.** Rehearsal uses isolated targets; verifies migrations,
checksums, scope, application reads, and recovery markers; records RPO/RTO and
pass/fail; cleans only disposable artifacts and retains evidence.

**Sources.** [Recovery runbook](../../ops/runbooks/web-production-recovery.md),
[backup runtime](../../apps/worker/src/backup_runtime.rs),
[cluster PITR](../../crates/infrastructure/src/cluster_pitr.rs).

### REQ-BCP-004 — Non-destructive rollback inventory

**Requirement.** ERPX shall retain pre-cutover databases, object namespaces,
binaries, environment evidence, and routing metadata until the approved
rollback window ends.

**Rationale.** Rollback must not depend on rebuilding already destroyed source
authority.

**Acceptance criteria.** Migration and bootstrap do not drop prior resources;
old resources remain offline and non-authoritative; expiry and deletion are
separately approved and target-specific; `.runtime/n8n/` is never a rollback
deletion target.

**Sources.** [Bounded-context decision](../../adr/ADR-014-cqrs-bounded-context-modular-monolith.md),
[cutover record](../../CUTOVER.md),
[project safety](../../README.md).

### REQ-BCP-005 — Degraded read-model operation

**Requirement.** ERPX shall continue valid authoritative writes when a
replaceable read database is unavailable and shall expose degraded query state
without reading write tables.

**Rationale.** Projection failure should not create write downtime or unsafe
fallback.

**Acceptance criteria.** Writes commit with outbox while read side is down;
queries report unavailable or not-ready; worker recovery replays from durable
position; rebuilt projection converges before activation.

**Sources.** [CQRS decision](../../adr/ADR-002-cqrs.md),
[bounded-context decision](../../adr/ADR-014-cqrs-bounded-context-modular-monolith.md).

### REQ-BCP-006 — Durable worker recovery

**Requirement.** ERPX shall resume projectors, schedulers, imports,
notifications, and long-running workflows from durable checkpoints after
process restart.

**Rationale.** Background business processing must not depend on process
memory.

**Acceptance criteria.** Claim identity and lease are durable; restart does not
duplicate completed effects; retries are bounded; unrecoverable work enters a
visible dead letter; replay requires governed authorization.

**Sources.** [Worker application](../../apps/worker/src/app.rs),
[CQRS decision](../../adr/ADR-002-cqrs.md),
[tenant projection scheduler](../../apps/worker/src/tenant_projection_scheduler.rs).

## Migration and cutover

### REQ-MIG-001 — Schema-generation-v2 manifest

**Requirement.** ERPX shall identify every clean-rebuild database with an
immutable schema-generation-v2 manifest bound to baseline, migration-set,
context catalog, and artifact digests.

**Rationale.** A clean database must not be confused with the incompatible
pre-cutover generation.

**Acceptance criteria.** Fresh provisioning begins at migration `0001` for
control-write, control-read, tenant-write, and tenant-read roles; every startup
verifies the exact generation and manifest digest; missing, legacy, or
conflicting generation fails before serving traffic.

**Sources.** [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md),
[migration composition](../../crates/migration/src/lib.rs).

### REQ-MIG-002 — New database provisioning

**Requirement.** ERPX shall provision clean-rebuild control and customer data
planes as new explicitly named database pairs without importing application
records from or mutating pre-cutover databases.

**Rationale.** The approved baseline is a clean start, while preserved data is
needed for recovery and investigation.

**Acceptance criteria.** Provisioning resolves only allow-listed new targets;
an existing or broad target is rejected; initialized pairs have separate write
and read roles; no legacy identity/data mapping runs; operation receipts prove
that old databases, objects, and n8n state were not changed.

**Sources.** [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md),
[project safety](../../README.md).

### REQ-MIG-003 — Clean schema verification

**Requirement.** ERPX shall verify normalized context schemas, RLS, write/read
separation, command receipts, audit, outbox, projection checkpoints, and typed
catalog consistency before a new data plane becomes eligible.

**Rationale.** Successful SQL execution alone does not prove the architectural
or isolation baseline.

**Acceptance criteria.** Verification exercises create/query/deny/rollback and
projection rebuild paths; schema/catalog digests reconcile; JSONB is limited to
approved document payloads; a missing constraint, RLS policy, role separation,
or architecture artifact leaves the data plane inactive.

**Sources.** [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md),
[CQRS decision](../../adr/ADR-002-cqrs.md),
[branch routing](../../adr/ADR-013-holding-branch-database-and-directory-authority.md).

### REQ-MIG-004 — Atomic cutover fence

**Requirement.** ERPX shall establish a bounded release fence and verify the
signed artifact, route/service/event catalogs, schema-generation-v2 receipts,
empty target-state policy, and recovery point before atomic owner switch.

**Rationale.** A hard cutover must not expose a mixture of incompatible
generations.

**Acceptance criteria.** Fence start/end and active session handling are
recorded; no application-data delta import or dual write occurs; target health
and manifests must match exactly; any mismatch releases no owner switch and
keeps or restores the preserved environment as sole owner.

**Sources.** [No dual writes](../../adr/ADR-003-no-long-term-dual-writes.md),
[FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md).

### REQ-MIG-005 — Atomic command-owner switch

**Requirement.** ERPX shall switch all approved route, service, event, worker,
and database ownership to one schema-generation-v2 artifact atomically and
shall reject legacy generation traffic afterward.

**Rationale.** Authority must never be ambiguous during activation.

**Acceptance criteria.** Switch records approval, recovery point, manifests,
and exact scope; routing changes atomically; first target write proves owner
enforcement; probes for `/workspaces`, `/api/bff`, `/references/:id`, retired
services/events, and old schema generation are rejected; old environment is
not concurrently writable.

**Sources.** [No dual writes](../../adr/ADR-003-no-long-term-dual-writes.md),
[cutover record](../../CUTOVER.md),
[branch migration](../../adr/ADR-013-holding-branch-database-and-directory-authority.md),
[FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md).

### REQ-MIG-006 — Rehearsed rollback

**Requirement.** ERPX shall rehearse and authorize rollback to the preserved
pre-cutover environment, or a governed forward-recovery procedure, without
merging incompatible write histories.

**Rationale.** Rollback must not merge two uncontrolled write histories.

**Acceptance criteria.** Rehearsal proves fencing, route reversal, artifact and
database selection, application access, and handling of any accepted v2 writes;
live recovery records all decisions and verification; mismatch keeps the
system fenced; rollback never deletes either generation or `.runtime/n8n/`.

**Sources.** [No dual writes](../../adr/ADR-003-no-long-term-dual-writes.md),
[branch rollback](../../adr/ADR-013-holding-branch-database-and-directory-authority.md),
[migration recovery tests](../../apps/migrator/tests/g13_deployment_recovery.rs),
[FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md).

### REQ-MIG-007 — Legacy retention and retirement

**Requirement.** ERPX shall preserve pre-cutover databases as inactive recovery
evidence for an approved retention window while preventing the clean runtime
from treating them as routable data planes.

**Rationale.** Immediate deletion removes investigation and recovery evidence;
indefinite legacy availability recreates split authority.

**Acceptance criteria.** New-runtime read/write attempts against old manifests
fail; recovery access is purpose-, role-, and time-bounded; any later disposal
is a separately approved destructive action with explicit targets;
bootstrap/cutover never deletes databases, objects, or n8n state.

**Sources.** [No dual writes](../../adr/ADR-003-no-long-term-dual-writes.md),
[cutover record](../../CUTOVER.md),
[project safety](../../README.md),
[FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md).

## Observability and operational response

### REQ-OBS-001 — Health and readiness

**Requirement.** ERPX shall expose separate bounded health and readiness for
API, identity HTTP, Console/BFF, workers, projections, and required
infrastructure.

**Rationale.** Process presence alone does not prove the product can safely
serve work.

**Acceptance criteria.** Health confirms process response; readiness verifies
required dependencies and authority; sensitive topology is redacted; an
unready dependency prevents false-ready while unrelated optional providers are
reported separately.

**Sources.** [Server operational metrics](../../apps/server/src/operational_metrics.rs),
[project runtime](../../README.md),
[incident runbook](../../ops/runbooks/web-production-incident.md).

### REQ-OBS-002 — Projection freshness monitoring

**Requirement.** ERPX shall monitor projection checkpoints, lag, freshness,
failure, and replacement state by tenant and physical route.

**Rationale.** A healthy API can still serve stale business information.

**Acceptance criteria.** Metrics identify projection and opaque scope without
credentials; threshold breach alerts; dashboards and clients receive the same
freshness classification; rebuild progress is distinguishable from failure.

**Sources.** [Platform Operations API](../../crates/proto/proto/erpx/platform_administration/v1/service.proto),
[production alerts](../../ops/observability/erpx-web-production-alerts.yml).

### REQ-OBS-003 — Worker heartbeat and queue state

**Requirement.** ERPX shall monitor worker heartbeat, claim age, retries,
dead letters, scheduler progress, and queue backlog.

**Rationale.** Silent background failure can leave business work incomplete
while interactive services appear healthy.

**Acceptance criteria.** Stale heartbeat and stuck claim alert; retry and dead
letter counts are visible; metrics use bounded labels; operator repair links to
the governed replay or runbook path rather than direct database editing.

**Sources.** [Worker application](../../apps/worker/src/app.rs),
[incident runbook](../../ops/runbooks/web-production-incident.md),
[production alerts](../../ops/observability/erpx-web-production-alerts.yml).

### REQ-OBS-004 — Redacted operational telemetry

**Requirement.** ERPX shall emit logs, metrics, traces, and alerts with stable
correlation and bounded context while excluding secrets and unnecessary
customer business data.

**Rationale.** Operational evidence must not increase data-exposure risk.

**Acceptance criteria.** Telemetry uses opaque tenant/resource IDs and stable
error categories; tokens, credentials, provider payloads, and sensitive fields
are redacted; correlation connects request, command, event, worker, and
projection without exposing raw secret material.

**Sources.** [Server operational metrics](../../apps/server/src/operational_metrics.rs),
[tenant isolation](../../adr/ADR-001-tenant-isolation.md),
[audited reads](../../adr/ADR-005-global-read-auditing.md).

### REQ-OBS-005 — Governed incident and recovery response

**Requirement.** ERPX shall maintain executable runbooks for unavailability,
HTTP and authentication failures, projection lag, dead letters, worker
failure, backup restore, PITR, and object recovery.

**Rationale.** Recovery outcomes should not depend on undocumented operator
memory.

**Acceptance criteria.** Runbooks identify diagnosis, safe action, validation,
rollback, escalation, and evidence; destructive steps are bounded to exact
targets and confirmations; exercises record RPO/RTO and pass/fail outcomes.

**Sources.** [Incident runbook](../../ops/runbooks/web-production-incident.md),
[recovery runbook](../../ops/runbooks/web-production-recovery.md),
[project runtime](../../README.md).

## Service-quality baseline

| ID | Title | Normative requirement | Rationale | Acceptance paths | Source | Scenarios |
|---|---|---|---|---|---|---|
| REQ-OBS-006 | Projection-lag gate | ERPX shall keep governed projection lag at or below 60 seconds for the release/operating gate. | Users need bounded read-model staleness. | Success: measured lag is within gate; Denial: stale projection cannot claim healthy; Failure: missing measurement is a failed gate. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-16 |
| REQ-BCP-007 | Recovery objectives | ERPX shall provide RPO of at most 24 hours and RTO of at most four hours for the baseline recovery class. | Customers need explicit recovery bounds. | Success: exercise meets both measures; Denial: unexercised claim is rejected; Failure: miss records corrective evidence. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-14 |
| REQ-BCP-008 | Backup-age gate | ERPX shall maintain a verified backup no older than 25 hours for each protected scope. | Backup scheduling needs an enforceable bound. | Success: verified receipt stays within age; Denial: unverified object is not counted; Failure: age breach alerts and fails continuity status. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-14 |
| REQ-BCP-009 | PITR rehearsal gate | ERPX shall complete a verified point-in-time recovery rehearsal within every 31-day window. | PITR must be demonstrated. | Success: isolated rehearsal records target/result; Denial: destructive production rehearsal is rejected; Failure: overdue/failed exercise is visible. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-14 |
| REQ-OBS-007 | Reported non-gates | ERPX shall measure uptime, latency, throughput, concurrency, and WASM size without treating them as hard release gates until a later accepted decision. | Metrics are useful before thresholds are justified. | Success: release evidence reports values; Denial: undocumented threshold cannot block release; Failure: missing measurement is disclosed. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-30 |

## Clean-rebuild release governance

| ID | Title | Normative requirement | Rationale | Acceptance paths | Source | Scenarios |
|---|---|---|---|---|---|---|
| REQ-REL-001 | Signed release manifest | ERPX shall sign a manifest of source, toolchain, artifacts, schema generation, interfaces, routes, events, flags, tests, and provenance. | Promotion needs an exact unit. | Success: signature/digests verify for baseline 2.0.0; Denial: mismatched or legacy artifact cannot promote; Failure: incomplete manifest blocks release. | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | E2E-30 |
| REQ-REL-002 | Environment promotion | ERPX shall promote the same verified artifact through governed environments without rebuilding it. | Rebuilds can change content. | Success: digest remains constant; Denial: environment-built replacement is rejected; Failure: configuration incompatibility stops promotion. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-30 |
| REQ-REL-003 | Approval and SoD | ERPX shall require role-based release approval and segregation for production promotion and rollback. | High-impact release authority needs independence. | Success: eligible approver decides exact manifest; Denial: maker/self-approver is rejected; Failure: expired approval blocks action. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-30 |
| REQ-REL-004 | Coherent generation | ERPX shall require application, worker, contracts, routes, events, projections, and data planes in the clean environment to match the signed schema-generation-v2 manifest. | Mixed generations create ambiguous authority. | Success: the complete v2 matrix matches before activation; Denial: any old/new combination is rejected; Failure: detected mismatch halts cutover without partial promotion. | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | E2E-15, E2E-30 |
| REQ-REL-005 | Atomic activation | ERPX shall activate the verified clean-rebuild generation as one atomic ownership change without expand-contract, application-data backfill, compatibility alias, or dual write. | The approved clean start prioritizes unambiguous authority over rolling compatibility. | Success: one switch activates all v2 boundaries; Denial: partial activation cannot receive traffic; Failure: the fence returns to the preserved environment or governed forward recovery. | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | E2E-15, E2E-30 |
| REQ-REL-006 | Feature flag governance | ERPX shall version flag owner, purpose, audience, scope, default, expiry, dependency, audit, and removal. | Flags can become hidden permanent behavior. | Success: eligible cohort receives intended behavior; Denial: flag cannot override authority; Failure: control outage uses safe default. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-30 |
| REQ-REL-007 | Pre-cutover gates | ERPX shall validate the signed artifact through internal, production-like, and isolated rehearsal gates before the single production ownership switch. | Risk must be reduced without serving incompatible generations concurrently. | Success: each non-production gate produces manifest-bound evidence; Denial: failed gate prevents cutover; Failure: rehearsal isolation preserves active users and data. | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | E2E-30 |
| REQ-REL-008 | Rollback | ERPX shall define and rehearse application, flag, contract, projection, provider, and data rollback or forward-recovery actions. | Rollback is multi-layered. | Success: approved action restores supported state; Denial: destructive rollback lacking evidence is rejected; Failure: non-reversible layer uses governed recovery. | [FSD-002](../decisions/FSD-002-shared-platform-capabilities.md) | E2E-15, E2E-30 |
| REQ-REL-009 | Release observability | ERPX shall correlate artifact, schema generation, route/service/event catalog, flag versions, and cutover phase with health, errors, lag, queues, provider outcomes, and business receipts. | Operators need causal evidence. | Success: regression maps to exact generation and phase; Denial: telemetry excludes secrets/business payloads; Failure: observability gap stops or reverses cutover. | [FSD-006](../decisions/FSD-006-clean-rebuild-architecture-and-cutover.md) | E2E-30 |
| REQ-REL-010 | Release close | ERPX shall close a release only after promotion, migration, reconciliation, rollback readiness, evidence, and flag follow-up are recorded. | Deployment completion is more than startup. | Success: all gates and owners sign off; Denial: unresolved failure blocks close; Failure: missing receipt remains open work. | [FSD-001](../decisions/FSD-001-baseline-and-governance.md) | E2E-30 |
