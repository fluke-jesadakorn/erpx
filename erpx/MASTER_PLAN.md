# ERPX Master Plan

## Mission and completion rule

ERPX is the production Rust/Tonic/SQLx/PostgreSQL/MinIO successor to Folio. Folio remains the operational legacy pilot until a context has passed its migration, security, reconciliation, workflow, and rollback gates. There is one persistent ERPX goal; milestones and support packets are not separate goals.

Completion requires direct, independently checked evidence for every requirement in this plan and the user-supplied execution contract, including all required ERP modules, all Dioxus targets, security/reliability audits, migration rollback, and final Folio write retirement. Passing compilation or a prototype never closes a requirement.

## Target architecture

```text
Dioxus Web / Desktop / Android / iOS
                 │ gRPC / gRPC-Web
                 ▼
        ERPX Rust/Tonic modular monolith
                 │
       ┌─────────┴─────────┐
       ▼                   ▼
erpx_control_write/read   tenant_<opaque>_write/read
       │                   │
       └── outbox/workers ──┴── MinIO namespaces
```

The control plane owns identity, expiring RBAC, customers, global-read audit, support capabilities, catalog, Stripe state, entitlements, tenant registry, provisioning, backup metadata, and platform operations. Each holding owns its tenant aggregate write database and rebuildable read database. PostgreSQL aggregate state is authoritative; projections are derived and replaceable. Stripe is authoritative for payment and subscription state. Operations consumes source-context events and never owns sales, purchase, stock, invoice, or journal source records.

The single server process exposes Tonic gRPC/gRPC-Web, HTTP auth endpoints, raw-body Stripe webhooks, signed object access, health/readiness, metrics, tracing, and redaction. The worker owns outbox publishing, projections, retries/DLQ/replay, expiry, notifications, journeys, reconciliation, diagnostics, and migration jobs.

## Execution waves and gates

### G0 — Audit, contracts, and fixtures

| ID | Deliverable | Owner | Status |
|---|---|---|---|
| G0-001 | Current-state audit and baseline evidence | Main | Complete — `audits/G0-001_CURRENT_STATE_AUDIT.md` |
| G0-002 | Master plan, ledger, and architecture decision records | Main | Complete |
| G0-003 | Shared command/query/event/error/tenant/actor/audit/support/version/idempotency contracts and protobuf compatibility policy | Main | Complete — domain/protobuf contracts compile in server and Dioxus targets |
| G0-004 | Deterministic anonymized migration fixtures and fixture verification | Main with AG-MIG/AG-QA review | Complete — `crates/migration` and `fixtures/migration/deterministic.json` |

G0 gate: audit is reproducible, fixtures are deterministic, contracts compile in server and Dioxus, and all known baseline failures are explained.

### G1 — Production Rust foundation

| ID | Deliverable | Owner | Dependencies |
|---|---|---|---|
| G1-001 | SQLx control write/read pools, migrations, transactions, optimistic concurrency, typed errors; in-memory only in unit tests | AG-FND | G0 complete; foundation delivered and independently verified |
| G1-002 | Atomic aggregate/outbox persistence with complete envelope | AG-FND + Main | G1-001, G0-003; transaction primitives, support command adoption, and role lifecycle command adoption delivered; remaining context mutations pending |
| G1-003 | Worker, projectors, checkpoints, claiming, ordering, idempotency, bounded retries, DLQ/replay, read rebuild/switch | AG-FND + Main | G1-002; durable claim leases, aggregate ordering, bounded retry/DLQ/replay, strict checkpoint sequencing, and context filtering integrated; remaining context handlers, physical replacement-read adapter wiring, replay audit history, and production replacement-read orchestration pending |
| G1-004 | Auth/scope/correlation/tracing/authorization/support interceptors, stable errors, health/readiness/metrics, raw-body boundary, redaction | Main + AG-FND | Signed access-token transport, write-side session lookup/revocation, recursive role permission checks, platform-read/audit enforcement, canonical support scope, actor binding, customer scope binding, serialized HTTP and current-Tonic ErrorContext envelopes, conflict-checked correlation aliases, request latency tracing, durable worker heartbeat publication, tenant route probes, typed projection/backup freshness composition, health/readiness/metrics, raw-body SHA-256/correlation retention, and commercial/SoD/support-aware signed object-access transport delivered; deployment configuration, real backup/restore execution, and universal adoption across future modules remain pending |
| G1-005 | Shared Dioxus shell, router, target adapters, i18n/theme/forms/tables/dialogs and all state variants | AG-UI | G0-003 complete; shared shell, typed route/session-state integration, restoring/session-store abstraction, redacted credential types, login/refresh/revoke adapter, and console auth UI delivered; native secure-keystore adapters, Android/iOS target evidence, deep-link history, and all target product surfaces remain open |

G1 gate: restart preserves data; projections rebuild; no production path uses an in-memory repository; web/desktop compile; Android/iOS compile in CI; all boundary tests pass. The current wave has not passed this gate: persistence primitives and the Customer 360 projector are verified, while remaining production mutation adoption, context handlers, physical replacement read switching, expanded readiness probes, full gRPC error-boundary adoption, server authorization boundaries, and authenticated route integration remain open.

### G2 — Identity and universal expiring RBAC

AG-IAM owns G2-001 identity/recovery and G2-002 role aggregate. Main owns shared contracts and integration. The bounded `erpx-identity` primitive crate, write-only migration, SQLx identity store, persisted passkeys with atomic counters/revocation, persisted one-time recovery codes, device listing/revocation, HTTP login/refresh/revoke endpoints, signed transport validation, session revocation lookup, real `webauthn-rs` browser ceremony verification with server-side provider state, passwordless passkey session issuance, encrypted TOTP enrollment/verification, one-time recovery-session exchange, G2-002 role lifecycle, deterministic expiry review classifications/notifications, reviewed renewal invariants, finite final Platform Root/Holding Owner recovery materialization, authenticated scope-filtered role review transport, and authenticated final recovery transport with authoritative start time are delivered. Persistent review queues, worker notification delivery, authoritative approver identity resolution, full effective-access intersection, recovery replay persistence, and complete universal mutation adoption remain open. Implement Argon2id, WebAuthn, encrypted TOTP, hashed recovery codes, device/session revocation, five-minute access tokens, rotating hashed refresh tokens, immutable role assignment versions, exact half-open time windows, parent containment/cascade, two-person high-risk approvals, review queues, and final Platform Root/Holding Owner recovery.

G2 gate: clock-boundary and cascade tests pass; every mutation checks authoritative write-side access; no permanent assignment can be created. Persisted recovery/device primitives, durable login/recovery throttling, passkey persistence, real provider-aware fail-closed HTTP ceremony routing, server-side ceremony-state persistence, validated RP/origin configuration, passwordless passkey session issuance, encrypted TOTP enrollment/replay protection, one-time recovery sessions, customer/platform role-renewal policy, and durable role-review snapshot/notification/recovery-receipt persistence are integrated; worker delivery, approver identity resolution, recovery command receipt integration, and full universal mutation adoption remain open.

### G3–G6 — Platform control plane

After G2 contracts are stable, parallelize disjoint packets. The current
integration wave has delivered the Customer 360/global-read domain slice,
support write-side persistence, and commercial/billing/Stripe inbox
persistence; these are partial packets and do not close their gates:

- AG-CUS: customer account, organization/commercial/onboarding/health projections, global-read gateway, customer-visible audit, and Customer 360 APIs/pages. Enriched deterministic projection, active organization scope-binding derivation, clean rebuild evidence, typed read-pool detail methods, complete typed detail sections with freshness/absence/unsupported states, authoritative write-pool scope checks, deterministic redacted notes paging, authenticated audited `GetCustomer360Detail`, access-audit/notes read models, audited `GetCustomerProjection`, additive authenticated `GetCustomerAccessAudit`, scoped customer-security audit filtering, and route-aware read-only pages are integrated; application/client adapters, UI pagination, subtree/multi-holding semantics, audit/notes worker backfill, binding revocation reconciliation, and authenticated deep-link/mobile surfaces remain open.
- AG-SUP: ticket lifecycle/features, customer-approved scoped support capability, incidents, diagnostics, revocation, and support UI. Domain rules, write-side persistence, module-bound hash-only capability issuance, immutable approval binding with approver-scope containment recheck, authenticated session-device resolution at approval and signed-object access, terminal-ticket recurrence linkage, audience-separated customer replies/internal notes, bounded assignment/escalation/SLA commands, replay-safe authoritative ticket command persistence, authenticated ticket recurrence/detail-read/reply/internal-note transport, customer-safe reply response mapping, bounded attachment metadata with opaque references/content bounds/visibility/expiry, durable attachment metadata insert/list/revoke/expiry persistence, default attachment expiry worker registration, durable SLA notification inbox with deterministic key conflict handling, lease claims, delivery acknowledgement, real repository worker-source implementation, injected SLA notification worker seam, SLA warning/breach classification and deterministic audience notification policy, authenticated platform-scope incident create/transition/detail-read/bounded-list transport, bounded scope-filtered `ListTickets` queue transport and persistence, exclusive typed persistence cursors with opaque bounded transport encoding, terminal-ticket capability revocation, the authoritative capability intersection at signed-object boundaries, a bounded allow-list diagnostics/redaction boundary, capability-bound diagnostics with platform/customer-support mode separation, authenticated platform-scope diagnostics gRPC, safe incident read projection storage and registered outbox projector, expiry persistence/worker registration, session-revocation cascade, bounded role/customer/ticket cascade repository operations, and shared support/incident presentation models are integrated; cursor/filter binding at UI data adapters, ticket SLA producer invocation, provider/channel delivery, production cascade triggers/workers, live incident producers/broadcasts, signed-object transport adoption, object bytes, and native/live-data UI are open.
- AG-BIL: immutable catalog, pinned Stripe adapter, raw-body inbox/webhooks/reconciliation, billing state, grandfathering, entitlements and allocations. Catalog invariants/reconciliation persistence, billing/allocation persistence, raw-body inbox with digest/correlation metadata, deterministic typed catalog publication plans/responses, verified raw-body webhook parsing, bounded worker claiming/retry/dead-letter/replay with durable replay count, typed semantic event application, stale/duplicate suppression, authoritative entitlement checks, typed provider identifiers, endpoint/API-pin validation, bounded typed Stripe HTTP client, safe provider-error redaction, typed Product-before-Price live provider execution, server HTTP ingress adoption of the strict typed webhook insertion path, provider-only billing/entitlement command contexts, seven-day grace/read-only policy, capacity allocation/activation constraints, deterministic provider-event replay semantics, and transactional SQLx command methods are integrated; pinned official generated Stripe client/API synchronization, remote catalog/entitlement reconciliation, metadata onboarding, authenticated replay controls, worker integration, and full billing command runtime adoption remain open.
- AG-PRV: opaque tenant registry, physical database/storage provisioning, RLS, health, backups/PITR/restore tests, replacement read DB and platform operations UI. The registry, active-tenant enumeration, physical PostgreSQL factory, separate tenant migration sets, forced-RLS schema, health gates, injected namespace verification, backup metadata, typed tenant-bound backup/restore executor seam, bounded tenant-scoped restore-test scheduling/claim/retry/history policy, typed durable tenant projection scheduling/claim/retry/rebuild persistence, control-plane role authorization plus bounded scheduler audit context, explicit fail-closed worker scheduler/executor seam, routed multi-tenant composition helper, validated server-only routing configuration, Operations projector implementation, worker-binary registration, replacement-read state machine, tenant router, MinIO/S3-compatible namespace/presign adapter, server object-access transport, fail-closed durable readiness composition, G6-PRV-007 real physical provisioning/replacement-read evidence, bounded safe Platform Operations tenant registry/readiness RPC, G6-PRV-010 safe tenant detail registry adapter, authenticated GetTenant detail read, G6-PRV-011 bounded payload-free projection status adapter, G6-PRV-012 authenticated/audited bounded ListProjectionStatuses transport, G6-PRV-013 rebuildable control-read projection status storage/read adapter, G6-PRV-014 validated version-ordered scheduler status projection writer, G6-PRV-015 explicit opt-in worker status publisher with cadence yield, G6-PRV-016 publication timestamps and exact bounded freshness classification, G6-PRV-017 durable source-update keyset checkpointing with idempotent page commits, G6-PRV-018 failure-safe replacement-read abort orchestration, G6-PRV-019 authenticated Platform Operations rebuild intent, G6-PRV-020 server-bound candidate provisioning and Operations projector binding, generation-fenced candidate completion/abort/activation, global candidate ownership checks, physical identity/RLS probing, authenticated audited activation, G6-PRV-021 source high-watermark verification, G6-PRV-022 durable candidate dispatch through the authenticated server intent and explicitly registered routed worker, and G6-G12 Platform Operations route rendering are integrated; external backup/restore adapters, complete provisioning/backup/integration/audit/settings actions, and full platform operations journeys remain open.

The authenticated Dioxus tenant list/detail adapter is also integrated for
`/platform/tenants` and `/platform/tenants/:id`; its remaining Platform
Operations routes stay read-only/empty until their authoritative adapters are
implemented. Evidence: `audits/G6-G12-UI-010/README.md`.

The authenticated Dioxus projection-status adapter is integrated for
`/platform/projections` with bounded opaque-cursor pagination and a safe
display allow-list. Rebuild commands, filtering, and the remaining Platform
Operations routes remain open. Evidence: `audits/G6-G12-UI-011/README.md`.

The authenticated Dioxus backup-history adapter is integrated for
`/platform/backups` with a bounded tenant/history fan-out and a safe display
allow-list. Scheduling, restore, physical MinIO/PITR evidence, and the
remaining Platform Operations routes remain open. Evidence:
`audits/G6-G12-UI-012/README.md`.

Each packet must supply the prescribed task/dependency/path/interface/database/test/acceptance/rollback declaration and the prescribed handoff evidence. Main independently reviews and integrates it.

G3 gate: every global read is audited and customer-visible; no secret leaks. G4 gate: support authority is the intersection of identity/role/capability/scope/module/entitlement/billing/policy/SoD/represented-user authority. G5 gate: duplicate/out-of-order Stripe events are harmless and recovery restores authorized mutation. G6 gate: tenant read rebuild and restore tests complete without write downtime.

### G7 — Organization and Operations

AG-OPS owns organization aggregates, RLS/scope, event-driven Operations projections, OperationalPlan, OperationalCommitment, FulfillmentPlan, CapacityPlan, ExceptionCase, WorkQueuePolicy, OperatingCalendar, Shift, ShiftHandover, SOP/checklist, DispatchBatch, planning, branch queue, SLA/escalation/reassignment, and all `/operations` pages. The transport-neutral organization/Operations aggregate contract, immutable source references, closed typed source-event domains for Sales/Procurement/Inventory/Finance/Maintenance/Support/Integrations, scope containment, lifecycle rules, canonical event metadata, deterministic reference serialization, dependency-ready aggregate constructors/transitions, tenant SQLx write/read persistence, holding-bound ordered event pagination, bounded source-reference reconciliation pages, scoped projection rebuild preparation, tenant-routed worker read/rebuild seam, production Operations projector with closed aggregate decoding and contiguous idempotent checkpoints, durable multi-tenant scheduler registration, bounded typed active-exception read/write persistence, and versioned assign/reassign/acknowledge/escalate/resolve commands with SLA and active-principal proofs are integrated; source producers, scheduling policies, detection producers, expired-user reassignment lookup, manager hierarchy resolution, Folio reconciliation, authenticated routes, and the full UI remain open. Operations is orchestration only.

Gate: event processing is idempotent; branch isolation holds; source transaction ownership remains with Sales/Procurement/Inventory/Finance/Maintenance/Support/Integrations; Folio/ERPX totals reconcile.

### G8 — Finance

AG-FIN owns chart of accounts, journals/posting/ledger/periods, AP/AR, cash/bank import/reconciliation, budgets/controls/approval limits, company books/intercompany, consolidation/eliminations, Thai VAT/WHT/tax invoices/statutory reports, statements/drill-down, and finance migration/reconciliation. The validated chart, hierarchy, journal-line, balanced-posting, period-lock/close, immutable posted-journal, idempotency, scope-binding, deterministic trial-balance boundary, validated pure banking import/reconciliation domain, tenant-routed SQLx write persistence, Finance AP/AR and cash-control SQLx aggregates, atomic snapshots/outbox/receipts, separate read projections with forced RLS, optimistic versions, atomic period locking/posting, replay-safe journal idempotency, bounded Finance journal source pages, idempotent read projection checkpoints, tenant-routed clear/replay worker orchestration, migration 0023 assertions, authenticated bounded posted-journal read plus typed SaveSubledger and SaveAllocationBatch transports, the G8-FIN-011 tenant-scoped banking account/import/transaction/reconciliation write boundary with forced-RLS migration 0028 and bounded source-event pages, the G8-FIN-012 forced-RLS banking read migration 0029, typed idempotent read projection/rebuild adapter, and routed worker seam, the G8-FIN-013 provider-neutral opaque-reference adapter boundary, the G8-FIN-014 authenticated banking command/read RPCs with selectable durable scheduler executor, G8-FIN-015 scope-aware banking read-list transport, G8-FIN-016 deterministic legacy journal-fact backfill and authoritative bank-match persistence, G8-FIN-017 budget controls and immutable commitment decisions, G8-FIN-018 pure close/reporting rules, G8-FIN-019 authoritative close-review persistence, G8-FIN-020 source-owned close-control evidence publication, G8-FIN-021 control-scoped source close-control outbox publication, G8-FIN-022 exact control-plane bank-provider account routing with opaque credential-key references, G8-FIN-023 authenticated bounded bank-match proposal/decision transport, and G8-FIN-024 bounded worker-side provider statement import coordination are integrated; provider SDKs/credential retrieval, automatic bank-to-GL proposal generation, journal posting/application, close transport/workers/UI, AP/AR completion, SoD/approval enforcement, consolidation, Thai tax, statements, and migration/reconciliation remain open.

G8-FIN-032 adds the company-scoped authoritative Chart of Accounts slice. It
enforces bounded canonical account codes/names, category and normal-balance
facts, duplicate-code prevention, parent containment, cycle prevention,
optimistic command versions, explicit account lifecycle changes, and
posted-journal usage protection for structural edits. Migration 0042 persists
normalized chart/account/idempotency rows with deferred hierarchy constraints,
typed shared-envelope outbox events, immutable identity/scope triggers, and
forced holding RLS. Server transport, journal-posting adoption, read
projection, legacy-chart reconciliation, and the broader G8 gate remain open.

G8-FIN-033 freezes and implements the additive Finance Chart command boundary:
`SaveFinanceChart` carries bounded typed chart metadata, an explicit expected
version, and the shared command context; its typed receipt contains only chart
identity/version/idempotency/event metadata. The authenticated server method
requires active identity, exact actor/context and company scope binding,
`finance.write` RBAC, entitlement/billing/SoD authorization, tenant write
routing, atomic source persistence, and customer audit publication. Live
auth/database transport evidence, Chart read projection, journal-posting
adoption, and the broader G8 gate remain open.

G8-FIN-034 adds the rebuildable tenant-read Chart of Accounts projection and
typed read transport. The projection reads only the tenant-write outbox,
validates the complete shared envelope, enforces contiguous aggregate
versions, records idempotent receipts/checkpoints, replaces normalized chart
and account rows atomically, supports clear/rebuild, and applies forced
holding RLS. The routed worker provides bounded incremental and rebuild runs
with redacted failures. `ListFinanceCharts` exposes only bounded scalar chart,
account, and source-metadata fields after authenticated contained company
scope and `finance.read` authorization, routes only to the tenant read
database, and records a customer audit event. Independent disposable
two-database evidence covers projection, replay, scope isolation, gap
rejection, clear/rebuild, and forced RLS. Journal-posting adoption, durable
scheduler registration, and the broader G8 gate remain open.

G8-FIN-035 makes the normalized write-side Finance Chart authoritative for
legacy journal posting. Before posting, the tenant-write path locks and
reconstructs the chart/accounts from normalized source rows, enforces
holding/company identity and active-account membership, and fails closed when
the source chart or account is missing, inactive, or out of scope. Journal,
immutable account-usage facts, aggregate snapshot, outbox event, and command
idempotency are committed together; replay returns the original receipt.
Migration 0043 adds forced-RLS immutable usage facts and extends structural
account guards for posted usage. Provider/chart migration and the broader G8
gate remain open.

G8-FIN-036 registers the Finance Chart incremental/rebuild runner with the
durable multi-tenant projection scheduler. The `finance_chart` job kind uses
bounded payloads and cursors, rejects unsupported source fences, routes only
through the existing tenant router, and maps malformed jobs, retryable
failures, leases, and dead letters through the established scheduler. Full
deployment scheduling and the broader G8 gate remain open.

G8-FIN-037 registers the Finance AP/AR and cash-control subledger projection
with the same durable scheduler. The `finance_subledger` executor uses only
the existing tenant-write outbox and tenant-read projection APIs, supports
bounded incremental and rebuild runs, validates allowed aggregate cursors,
rejects unsupported source fences, routes through the tenant router, and
redacts retryable failures. Source-fenced replacement-read activation remains
open because the current subledger store has no source high-watermark API.

G8-FIN-038 adds the scoped Finance subledger read boundary. The read store
now exposes deterministic keyset pages with company/branch predicates,
projection-row identity checks, and forced-RLS-compatible holding routing.
`ListFinanceSubledgerReadRows` returns bounded typed metadata plus bounded
aggregate payloads, requires authenticated `finance.read` access and customer
scope membership, opens only the tenant read pool, rejects broad/composite or
cross-company filters, uses opaque cursors, and records a customer-visible
audit event. A disposable two-database proof covers projection, pagination,
company/branch isolation, cross-scope rejection, and forced RLS. Live
authenticated transport proof and the broader G8 gate remain open.

G8-FIN-040 adds a target-neutral shared Dioxus Finance presentation
contract. It provides the ten Finance routes, bilingual Thai/English labels,
bounded opaque identifiers, scalar-only display fields, projection freshness,
and explicit loading/empty/ready/denied/stale/read-only/conflict/offline/error
states. It has no networking, protobuf, database, token, or authorization
logic; target adapters and live Finance journeys remain open.

G8-FIN-039 adds a read-only Finance subledger source high-watermark API and
uses it to fence rebuilds. Matching rebuild fences are checked before the
read model is cleared, source movement after replay returns
`CandidateSourceMoved`, and incremental fences remain rejected. This makes
source movement fail closed for the normal subledger executor. Candidate
replacement-read activation still needs scheduler registration for this
projection, and the broader G8 gate remains open.

G8-FIN-041 registers the existing Finance close projection runner with the
durable tenant scheduler as `finance_close`. It supports bounded incremental
and unfenced rebuild jobs, validates UUID/version cursors and tenant
references, routes through the physical write/read pair, and redacts
retryable failures. Close source-fenced jobs remain rejected because the
close store has no source high-watermark API; the broader G8 gate remains
open.

Gate: balanced journals, enforced period locks, reconciled trial balance/statements, consolidation/tax fixtures, and SoD evidence.

G8-FIN-025 adds a fail-closed secret-manager credential resolver seam for
provider composition. It intentionally does not provide a deployment secret
manager, provider SDK, import command, or Finance completion evidence; the G8
gate remains open.

G8-FIN-026 adds the injected authorized worker import workflow: exact
control-plane routing, credential resolution, bounded provider coordination,
and separate tenant write/read persistence. Scheduler/transport registration,
deployment adapters, and database-backed command evidence remain open.

G8-FIN-027 adds the bounded exact-key provider adapter registry. It remains
empty until deployment-specific provider SDK factories are supplied and cannot
select a fallback provider.

G8-FIN-028 adds the authenticated `SaveFinanceCloseReview` command transport:
bounded typed period/review decoding, explicit company scope, authenticated
actor and embedded-context binding, Finance entitlement enforcement, tenant
write routing, optimistic expected-version checks, typed receipts, and audit
publication. Close read/report transport, close workers, UI, database-backed
RPC evidence, and the broader G8 gate remain open.

G8-FIN-029 adds a read-only close-review snapshot/report adapter. It preserves
holding/company/period identity, lifecycle status, aggregate version,
idempotency metadata, and bounded statement/account/control summaries while
excluding raw JSONB facts and command contexts. It reports missing close rows
and stale/missing Finance journal projection facts explicitly. No migration or
transport is added; authenticated server composition, close projection
storage, workers, UI, and the broader G8 gate remain open.

G8-FIN-030 adds authenticated `GetFinanceCloseReview` query transport with
explicit holding/company/branch scope, `finance.read` authorization, customer
scope membership, typed close/report/metadata responses, customer audit
publication, and fail-closed broad/cross-company rejection. The dedicated
close-review read migration/projector, UI, and broader G8 gate remain open.

G8-FIN-031 adds the dedicated tenant read-side Finance close projection
schema: bounded review/report summaries, account summaries, per-period
checkpoints, source event/version metadata, and forced holding RLS. The main
agent added the tenant-routed source-event projector and rebuild worker,
including contiguous aggregate versions, idempotent replay, atomic account
replacement, and disposable projection clearing. `GetFinanceCloseReview` now
reads only this physically separate tenant read model; the read adapter no
longer opens or queries the tenant write database. Live disposable-database
projection evidence and Finance UI remain open.

### G9 — Sales, procurement, inventory, logistics

AG-SCM owns customer/vendor masters, leads/opportunities, quotes/orders/credit/delivery/invoice/collections, purchase request/approval/RFQ/comparison/PO/receipt/bill/payment handoff, products/warehouses/locations/availability/reservations/movements/transfers/counts/replenishment/weighted-average cost/recosting.

The first Sales contract and SQLx persistence slices are integrated:
tenant-bound customer references, leads, opportunities, quotes, sales orders,
delivery/invoice handoffs, and collection state with deterministic
references/idempotency keys, ordered lifecycles, integer totals, and
company/branch containment; separate tenant write/read migrations with forced
RLS; optimistic versions; atomic aggregate snapshots, outbox events, and
idempotency receipts; and a read-pool projection seam with contiguous version
checks, idempotent replay, checkpoints, and rebuild clearing. The bounded
Procurement/Inventory domain slice is also integrated: scoped vendor/product/
warehouse references, approval-gated purchasing, receipt matching, stock
movement/transfer/reservation rules, and checked weighted-average costing.
Sales transport, SCM transport/projectors, logistics, source events, and
end-to-end reconciliation remain open. The SCM write/read persistence seam
with optimistic versions, idempotency, atomic outbox delivery, source-event
projection replay, checkpoints, and rebuild clearing is integrated. A
tenant-routed Sales source-event page and clear/replay projector runner are
also integrated; durable per-tenant claiming/scheduling and workflow
reconciliation remain open. The standalone tenant-routed SCM projector,
bounded catch-up, and clear/replay rebuild runner are also integrated.

Gate: end-to-end Lead-to-Cash and Procure-to-Pay, inventory/accounting reconciliation, and correct Operations events.

### G10 — Marketing and CRM

AG-MKT owns profiles/deduplication/consent/suppression/double-opt-in/retention/erasure, audiences, campaign/journey lifecycle, approved n8n actions, MinIO content/templates/landing pages/forms/domains/SEO/publication rollback, channels, CRM lead handoff/opportunities/activities, attribution models, ROAS and Finance reconciliation. Tenant-bound profiles, deterministic deduplication, purpose/channel/policy-version consent, pending double-opt-in/revocation, suppression, bounded audience membership, campaign lifecycle, send-time idempotent authorization, tenant-routed SQLx profile/consent/suppression/audience persistence, separate read projections, forced RLS, optimistic versions, idempotent mutation receipts, a pure durable channel-delivery contract, tenant-routed dispatch persistence, a named worker source/provider seam with atomic idempotency reservation, consent/suppression recheck, claim-UUID-owned attempts, bounded retries, explicit provider-accepted versus delivered states, a pure authenticated callback contract, raw-body HMAC ingress, active control-plane provider-account routing, exact delivery/provider-reference binding, tenant callback inbox deduplication/order classification, correlation retention, atomic callback state/outbox persistence, the G10-MKT-019 typed Marketing-to-CRM handoff/attribution contract, G10-MKT-020 tenant-scoped CRM handoff/opportunity/attribution write persistence, G10-MKT-021 CRM read projection/event receipts, G10-MKT-022 production worker scheduler composition, G10-MKT-023 bounded scope-filtered CRM read transport, and G10-MKT-024 authenticated CRM command transport are integrated; provider-specific adapters, typed field-level CRM contracts, journeys, CRM UI, attribution projections, Finance reconciliation, and UI remain open.

Gate: consent checked at schedule and delivery, suppression stops queued sends, retries are idempotent, custom domains are tenant-bound, revenue attribution reconciles with Finance.

G10-MKT-026 adds shared target-neutral CRM display models and explicit
projection/UI state handling, but does not claim CRM page rendering, client
transport, journeys, consent/delivery, or Finance attribution reconciliation.

### G11 — Remaining ERP contexts

AG-AST owns assets and maintenance. AG-PEO owns HR/payroll and legal/documents. Main coordinates the remaining POS, Projects, Service, Manufacturing, Quality, AI permission-aware tools, integrations/public APIs, and cross-context contracts. Every context gets source/write aggregates, read projections, authoritative access, audit, migrations, fixtures, and rollback evidence.

The bounded Assets/Maintenance domain slice is integrated: scoped asset
service/maintenance/impairment/revaluation/disposal, checked accounting
arithmetic, immutable custody/location history, preventive/corrective work
orders, parts/labor/downtime/reliability metrics, and an explicit Operations
capacity-impact handoff. Tenant write/read SQLx persistence, forced RLS,
optimistic/idempotent mutations, append-only custody storage, source-owned
capacity handoffs, projection checkpoints, and rebuild clearing are
integrated. A bounded tenant source-event page and clear/replay projector
runner are integrated. Durable scheduling/claiming, GL posting, transport, HR,
Legal, and remaining ERP modules remain open.

G11-PEO-001 adds the pure HR/payroll foundation: scoped employee lifecycle,
leave, expenses, bounded payroll reconciliation, separated preparer/reviewer/
poster transitions, and explicit expired-role workflow reassignment. It has no
persistence, transport, or provider integration; authoritative RBAC/SoD,
payroll calculation, read projections, and the broader G11 gate remain open.

G11-LEGAL-001 adds pure Legal/document contracts: scoped contract parties,
review/approval/retention lifecycle, versioned opaque document metadata,
expiring download authorization metadata, and immutable access history. No raw
document bytes, secrets, tokens, or storage calls enter the domain; persistence,
MinIO integration, authorization, and the broader G11 gate remain open.

G11-PEO-002 adds tenant-write SQLx persistence for employees, leave, expenses,
payroll runs/lines, and expired-role reassignment metadata. It uses typed
columns, forced holding RLS, optimistic versions, idempotency replay/conflict
binding, atomic outbox events, terminal-state database triggers, approval
separation, and payroll reconciliation. HR transport, read projections,
expiry-worker integration, and live database evidence remain open.

G11-PEO-004 adds the first authoritative HR command boundary:
`SaveHrEmployee` uses the shared command context, authenticated actor binding,
explicit holding/company/branch scope, HR entitlement/RBAC/billing/SoD
authorization, bounded typed employee decoding, expected-version binding,
tenant-write routing, and customer audit publication. Leave, expense, payroll,
and reassignment transports plus HR read projections and UI remain open.

G11-PEO-003 adds tenant read migration 0032 for typed employee, leave, expense,
payroll run/line, and expired-role reassignment projections, with source
version/event metadata, checkpoints, event receipts, bounded constraints, and
forced holding RLS. The main agent added a tenant-routed HR projector and
incremental/rebuild worker covering all five HR aggregate types, contiguous
versions, duplicate event receipts, typed field-only upserts, payroll line
replacement, and disposable read-model clearing. HR read transport and live
database evidence remain open.

G11-PEO-005 adds an authenticated `ListHrEmployees` read boundary. It requires
an active session, explicit contained holding/company/branch scope, the `hr.read`
role, bounded keyset pagination, customer scope binding, and a customer-visible
audit event. It routes only to the tenant read database and exposes typed
projection fields with source-version/event and freshness metadata; it does not
open a tenant write pool or expose raw payloads. Leave/expense/payroll read
surfaces, live projection evidence, expiry-worker integration, UI, and the
broader G11 gate remain open.

G11-PEO-006 extends the HR read adapter with typed, read-database-only pages for
leave requests, expense claims, payroll runs/lines, and expired-role
reassignment metadata. Each list is scope-bound and bounded before database
access, uses deterministic exclusive keyset ordering, and keeps payroll line
limits explicit with truncation metadata. No protobuf/server route or mutation
claim is made by this adapter slice; live projection evidence and the broader
G11 gate remain open.

G11-PEO-007 adds the authenticated `HrService.ListHrLeaveRequests` query. It
reuses the employee read boundary's active-session, explicit contained scope,
`hr.read`, customer-scope binding, read-only route, and customer-auditable
purpose/correlation requirements. Leave rows are typed, bounded, and keyed by
an opaque deterministic date/UUID cursor. Expense, payroll, reassignment
queries, HR mutations beyond employees, UI, and the broader G11 gate remain
open.

G11-PEO-008 adds the authenticated `HrService.ListHrExpenseClaims` query with
the same fail-closed session, contained scope, `hr.read`, entitlement/customer
scope, read-only routing, and customer-audited query boundary. Expense rows are
typed and bounded, and the UUID keyset cursor is opaque at the transport edge.
Payroll, reassignment, HR command completion, UI, and the broader G11 gate
remain open.

G11-PEO-009 adds the authenticated `HrService.ListHrPayrollRuns` query. It
enforces the same session/scope/RBAC/customer-audit/read-route boundary,
separately bounds parent and line page sizes, returns typed payroll totals and
normalized line fields, preserves explicit line truncation metadata, and uses
an opaque date/UUID parent cursor. Reassignment reads, HR mutations, payroll
write workflows, UI, and the broader G11 gate remain open.

G11-ERP-POS-001 adds a pure typed POS domain slice with scoped sale references,
bounded sale lines and payments, currency/totals invariants, closed lifecycle
transitions, optimistic version/idempotency semantics, and typed audit metadata.
Payment instruments, secrets, provider tokens, and raw JSON are excluded. POS
persistence, outbox/projection, authorization transport, Finance refund
handoff, and the broader G11 gate remain open.

G11-ERP-POS-002 adds normalized tenant-write POS persistence for sales, lines,
payments, and command idempotency. The sidecar has forced holding RLS, typed
payment-method and scope columns, optimistic versioning, atomic outbox events,
deferred totals/payment reconciliation, and terminal-sale immutability. It does
not provide POS read projection, worker scheduling, authenticated transport,
UI, Finance refund handoff, or broader ERP completion evidence.

G11-ERP-POS-003 adds the typed rebuildable POS read projection and worker seam.
Migration 0033 has forced holding RLS for sale, line, payment, checkpoint, and
event-receipt rows. The projector allow-lists `pos_sale`, enforces source
identity/version and contiguous checkpoints, treats receipts as idempotent, and
clears disposable read state for rebuilds. The worker provides bounded
incremental/rebuild routing with redacted errors. POS read RPC/UI, production
scheduler activation, live replay evidence, and the broader G11 gate remain
open.

G11-ERP-POS-004 adds the authenticated `PosService.ListPosSales` read gateway.
It requires an active session, explicit contained company/branch scope,
`pos.read`, customer-scope binding, bounded page and child-row limits, and a
customer-auditable `pos.sale.read` event. It routes only to the tenant read
database and exposes typed sale, line, and non-secret payment summaries with
opaque keyset cursors; command transport, live read replay, UI consumption,
Finance refund handoff, and the broader G11 gate remain open.

G11-PEO-010 adds the authenticated `HrService.SaveHrLeave` command boundary.
It requires an active command context, explicit contained scope, tenant
entitlement/RBAC/billing/SoD authorization, write-database-only routing,
command/aggregate identity binding, bounded typed JSON input, optimistic
version matching, atomic idempotent persistence, and an auditable
`hr.leave.saved` event. Expense/payroll/reassignment command transport, HR UI,
and the broader G11 gate remain open.

G11-ERP-POS-005 adds the target-neutral shared Dioxus POS presentation contract:
the seven required allow-listed routes, opaque sale identifiers, bilingual
labels, explicit loading/empty/denied/stale/read-only/conflict/offline/error
states, and scalar-only redacted sale/line/payment-summary models. It performs
no network or database work and does not claim authorization; consuming
adapters, POS commands, and the broader G11 gate remain open.

G11-PEO-011 adds authenticated `HrService.SaveHrExpense` and
`HrService.SaveHrPayroll` command boundaries. Both require bounded typed input,
explicit contained scope, expected-version matching, authoritative HR
entitlement/RBAC/billing/SoD authorization, write-database-only routing,
atomic idempotent persistence, and auditable save events. Payroll retains
domain-enforced line and approval separation; reassignment command/read
transport, HR UI, and the broader G11 gate remain open.

G11-PEO-012 adds the authenticated `HrService.ListHrReassignments` query. It
uses explicit contained scope, `hr.read`, customer-scope binding, bounded
read-only tenant routing, customer-auditable access, typed reassignment facts,
and an opaque timestamp/workflow cursor. Reassignment commands, expiry-worker
execution, HR UI, and the broader G11 gate remain open.

G11-PEO-013 adds the target-neutral shared Dioxus HR/Payroll presentation
contract: six allow-listed routes, bounded opaque identifiers, Thai/English
labels, scalar-only redacted employee/leave/expense/payroll/reassignment
models, and explicit loading/empty/denied/stale/read-only/conflict/offline/error
states. It performs no network or database work and cannot authorize
mutations; authenticated adapters and complete HR product journeys remain
open.

G11-ERP-POS-006 adds the authenticated `PosService.SavePosSale` command. It
requires a bounded typed sale aggregate, exact command/idempotency/scope
binding, expected-version matching, authoritative POS entitlement/RBAC/
billing/SoD authorization, tenant write-database routing, atomic normalized
persistence, and an auditable `pos.sale.saved` event. Refund handoff, command
UI, durable projection activation, and the broader G11 gate remain open.

G11-ERP-POS-007 adds the target-neutral POS command presentation contract for
sale entry, refund review, receipt issuance, and shift-close approval. It uses
bounded opaque IDs, scalar-only redacted inputs/summaries, bilingual labels,
and explicit non-ready/read-only/conflict/offline states. Main exported the
presentation module after review; it contains no network, database, payment
instrument, provider, or authorization behavior. Command consumers, refund
accounting, and the broader G11 gate remain open.

G11-ERP-POS-008 adds a bounded refund-review domain aggregate. It validates
completed-sale/refundable-line facts, exact company/branch scope, positive
quantities and amounts, requester/reviewer separation, contiguous versions,
and the closed review-to-handoff lifecycle with exact replay fingerprints.
Payment execution, Finance posting, persistence, transport, and authoritative
authorization remain open.

G11-LEG-001 adds tenant-write SQLx persistence for the bounded Legal/document
source slice. Contracts, parties, approval facts, retention state, documents,
opaque version metadata, and append-only access history use typed normalized
tables with forced holding RLS, optimistic source versions, idempotency
receipts, atomic outbox events, and immutable database triggers. Its main-agent
remediation binds every mutation to the shared command/event context, enforces
draft-only party mutation and the closed lifecycle, preserves creators,
rejects version overflow, requires opaque storage references and exact SHA-256
digests, and adds the shared envelope columns to the tenant outbox. Raw
document bytes, credentials, tokens, and provider state remain excluded. The
source proof now covers versioned document/access writes, exact replay, stale
version rejection, persisted idempotency, envelope fields, forced-RLS
cross-holding denial, and immutability triggers; Legal read projection,
transport, MinIO signed access, and the broader G11 gate remain open.

G11-LEG-003 now adds a tenant-read Legal projection and typed read adapter.
It consumes only the complete shared event envelope and closed typed Legal
payloads, rejects legacy/null envelope rows, validates exact digests and
opaque references, persists normalized contracts/parties/approval/retention,
document versions, access history, receipts, and checkpoints under forced
holding RLS, and supports bounded scoped reads plus clear/rebuild. The live
proof covers source ingestion, three-event projection, exact replay, typed
freshness, cross-scope empty reads, rebuild, and legacy-envelope rejection.
Read transport, MinIO signed document access, retention workers, and the
broader G11 gate remain open.

G11-LEG-004 adds the infrastructure-only Legal MinIO presign seam. It accepts
the existing typed `SignedDownloadAuthorization`, enforces non-expired
one-second-to-seven-day lifetime and 64 MiB bounds, validates the opaque object
key through the shared storage adapter, and returns only a bounded signed GET
URL. Identity, tenant scope, RBAC, retention policy, and access-history
recording remain authoritative server responsibilities; tenant-specific
transport wiring and durable multi-tenant scheduling remain separate work.

The main agent froze an additive `LegalService` protobuf contract with typed
write receipts, scoped contract/document/access-history queries with bounded
cursors, source-event/freshness metadata, and redacted object metadata only.
The generated proto crate compiles; it contains no raw document bytes,
credentials, tokens, or provider payloads.

G11-LEG-005 adds authenticated Legal gRPC transport for all six frozen
methods. Writes require active authentication, explicit company/branch scope,
the authoritative Legal entitlement/RBAC decision, tenant write routing,
typed source persistence, receipts, correlation metadata, and customer audit
events. Reads require explicit contained scope and `legal.read`, route only to
the tenant read database, map bounded typed rows/cursors, and emit
customer-visible audit events. Live auth/database integration is still
required; signed object endpoint wiring, durable retention scheduling, Legal UI,
and the broader G11 gate remain open.

G11-LEG-006 adds the tenant-write retention sweep. It bounds each batch to
100 contracts, reconstructs active due contracts as typed domain commands with
a system-worker actor, persists the Expired transition through the normal
optimistic/idempotent source path and shared outbox envelope, and reports
optimistic conflicts for the next sweep. A fresh disposable PostgreSQL proof
shows one due contract expires exactly once and a second sweep is empty. Object
deletion and document retention policy beyond contract expiry remain open; an
explicit tenant-routed worker handler is now integrated, while durable
multi-tenant enumeration/scheduling remains open.

G11-LEG-008 adds the explicit write-only `legal_retention` worker handler. It
routes only a validated tenant/holding pair to the tenant write database,
passes a bounded batch to the source sweep, redacts routing/source failures,
and exposes no read-database, MinIO, deletion, or authorization behavior.
Explicit tenant environment configuration is supported; durable scheduler
claims and tenant enumeration remain open.

G11-LEG-007 adds the target-neutral Dioxus Legal presentation contract for
five Legal routes. It contains bounded opaque identifiers, scalar-only
redacted models, Thai/English labels, and explicit loading/empty/ready/denied/
stale/read-only/conflict/offline/error states. It has no network, database,
authorization, signed URL, or mutation behavior; authenticated adapters and
target-specific rendering remain open.

Gate: asset/GL and maintenance/custody history reconcile; HR/payroll isolation; legal retention/version/access history; AI cannot bypass authorization; remaining module workflows are independently exercised.

### G12 — Dioxus product completion

AG-UI owns shared Web/macOS/Windows/Linux/Android/iOS surfaces: full platform/tenant navigation, scope selector, persistent access banners, secure sessions, deep links, notifications, signed files, camera/scanner, approved offline drafts, keyboard navigation, WCAG 2.2 AA, Thai/English, freshness/conflict recovery, and mobile approvals/Operations/stock/support/expense/leave/maintenance. Shared typed support/incident route surfaces, authenticated Customer 360 detail transport consumption, safe display redaction, explicit non-ready/read-only states, and the 23-entry bilingual Marketing/CRM route scaffold are integrated; live data adapters for every section, native secure storage, notifications, and complete product journeys remain open.

Gate: required role journeys on every approved target, secure local token handling, and CI target evidence.

### G13 — Migration and cutover

AG-MIG owns deterministic ID mappings, context backfills, shadow projections, reconciliation/difference explanations, pilot cohorts, rollback rehearsal, mutation freeze/delta import, ERPX command-owner switch, Folio read-only retention, and retirement. AG-QA independently verifies every cohort. No long-term dual writing.

For every context: freeze source contract → map IDs → backfill write → project reads → replay shadow changes → compare counts/statuses/totals/files/roles/audit → explain differences → read-only pilot → workflow/security acceptance → rollback rehearsal → freeze/import delta → switch owner → keep Folio read-only → retire after retention.

G13-MIG-001 adds the isolated `erpx-migrator` dry-run CLI. It emits bounded
fixture summaries, deterministic ID mappings, explained/unexplained
reconciliation reports, and the complete cutover plan without database,
network, Folio, or command-owner mutation. JSON/UTF-8 size limits, secret
redaction, unknown-option rejection, and relative non-symlink input/output
paths are enforced before any file is read or written. Live source adapters,
backfill, shadow replay, cohort orchestration, rollback execution, and final
cutover remain open.

### G14 — hardening and final audit

Main and AG-QA prove exact role boundaries/cascade/isolation/support approval/global-read disclosure/redaction/Stripe signatures/consent/object access/session/capability revocation; outbox atomicity/projector idempotency/DLQ/rebuild/restore/rollback/reconciliation/journey resume/Operations exception recovery; and every product workflow listed in the acceptance contract. Only then may the persistent goal be marked complete.

## Delegation protocol

The role map is AG-FND, AG-IAM, AG-CUS, AG-SUP, AG-BIL, AG-PRV, AG-OPS, AG-FIN, AG-SCM, AG-MKT, AG-AST, AG-PEO, AG-UI, AG-MIG, and AG-QA. Packets have non-overlapping write scopes and must state Task ID, objective, dependencies, allowed/forbidden paths, interfaces consumed/produced, database effects, tests, acceptance evidence, and rollback. Handoffs must report behavior, files, contracts, migrations, exact tests, unfinished work, security concerns, and integration order. Main owns shared contracts, cross-context integration, review, verification, ledger updates, and final audit.

## Operational invariants

- Folio remains running on port 3004 during development handoff.
- `.env`, `.env.local`, `infra/.env`, `.runtime/`, and the n8n encryption key remain outside Git and are never reset or regenerated.
- Existing user changes are preserved.
- Tenant credentials never reach customers or Dioxus client code.
- Every role and support capability expires; no permanent assignment API exists.
- Read models are disposable and rebuildable; write state is authoritative.
- No production service uses an in-memory repository.
- No completion claim is based on Folio still covering an unimplemented ERPX requirement.

## Packet amendment 2026-07-25 — G10-MKT-027

The shared Dioxus console now exposes the exact existing Marketing/CRM route
allow-list with bilingual navigation and a typed, read-only client boundary
for the existing `ListCrmReadRows` contract. Handoff, opportunity, and
attribution routes map to the already-supported CRM aggregate types; other
allow-listed routes render an explicit unavailable state without invented
data. The client rejects platform/subtree scope and remains unavailable until
an authoritative holding/company/branch scope provider exists. Payloads are
bounded through the G10-MKT-026 shared display models, raw JSON is not
rendered, and all mutations remain server-authorized. CRM scope selection,
freshness/cursor continuation, remaining CRM/Marketing adapters, journeys,
consent/delivery, attribution reconciliation, and the broader G10/G12 gates
remain open.

## G8-FIN-042 — Finance close source-fenced rebuilds

The Finance close tenant projection now exposes a holding-scoped source
high-watermark and fences replacement-read rebuilds both before clearing the
candidate and after replay. Source movement fails closed as
`CandidateSourceMoved`; incremental jobs carrying a source fence remain
permanently rejected. Bounded cursors, redacted worker failures, physical
tenant write/read routing, and unfenced rebuild behavior remain intact. A
generic candidate activation seam and live PostgreSQL proof remain open.

## G8-FIN-043 — authoritative Finance journal handoffs

Finance AP/AR and cash subledger writes now validate every carried
`FinanceJournalHandoff` against the immutable tenant-write
`finance_posted_journals` row, including journal identity, period, holding,
company, accounting date, source record binding, and immutable posted state.
The validation occurs within the same write transaction before aggregate
snapshot/outbox persistence. Allocation-batch mutations repeat the check for
locked authoritative payment/invoice state and every resulting mutation,
preserving atomicity and idempotent receipts. Draft/unposted records may omit
the handoff. Missing or mismatched journal state fails with no aggregate,
snapshot, outbox, or receipt rows. The broader Finance AP/AR, cash, close,
tax, consolidation, and candidate-activation gates remain open.

## G8-FIN-044 — Finance replacement-read activation integration

Finance subledger and Finance close projectors now implement the routed
projection boundary used by the generic replacement-read scheduler. The
adapters validate exact tenant/holding identity and physical write/read side,
consume only supplied pools, preserve bounded cursors and source fences, map
source movement to `CandidateSourceMoved`, and keep activation/source locks in
the scheduler. Control-plane source watermark routing now covers Operations,
Finance Banking, Finance subledger, and Finance close. Finance scheduler
composition preserves explicit unfenced rebuilds while candidate rebuilds use
the source-locked activation path. Live candidate activation and the broader
Finance/product gates remain open.

## G8-FIN-046 — Finance write transport read-pool isolation

Authenticated Finance subledger and allocation-batch commands now route only
the tenant write database. Their persistence adapter receives the write pool
for both internal handles because these mutations never execute read-side
queries; the read database is no longer opened or required for a write
command. Read transports remain tenant-read-only and separately routed.
Authenticated live transport proof and the broader Finance workflow gates
remain open.

## G8-FIN-045 — Finance candidate read rebuild and activation proof

The worker test `apps/worker/tests/finance_candidate_activation.rs` provides
an independently runnable disposable PostgreSQL proof for the Finance
subledger replacement-read path. It creates unique control, tenant-write,
tenant-read, and candidate databases; migrates each required schema; registers
one active tenant; seeds one authoritative payable-invoice source event; and
invokes the public generation-fenced `RoutedTenantProjectionExecutor` and
Finance subledger projector. The proof verifies the projected candidate row,
physical candidate database, checkpoint event/version, unchanged source
watermark, activated switch generation, and active tenant registry target,
then terminates and drops every disposable database. Database failures are
redacted to generic test messages and no URL or secret is emitted.

The exact live command is:
`cargo test -p erpx-worker --test finance_candidate_activation -- --ignored --exact finance_subledger_candidate_rebuild_activates_generation_and_projects_checkpoint`.
The proof does not claim the durable scheduler's `claim_due` loop, multi-worker
lease competition, or production worker-loop orchestration; it directly
exercises the public routed candidate executor because those scheduler claims
remain separately gated.

## G5-STRIPE-LINT-001 — typed Stripe provider DTO lint boundary

The low-level Stripe HTTP boundary now documents narrow `dead_code` allowances
for entitlement-feature and billing-meter request/response DTOs that are
constructed by external catalog integrations or serde rather than by the
transport crate's internal call graph. No endpoint, serialization, credential,
redaction, or billing behavior changed. This clears the workspace Clippy gate
without hiding unrelated code; Stripe provider SDK pinning, remote catalog
reconciliation, metadata onboarding, authenticated replay controls, and full
billing runtime adoption remain open.

## G8-FIN-045 — Finance candidate activation proof

A public-API disposable PostgreSQL integration proof now seeds an authoritative
Finance subledger source, executes the routed candidate rebuild, verifies the
candidate generation is activated, and checks the projected row/checkpoint on
the physically distinct candidate read database. The proof creates and drops
all databases and redacts database failures. The broader Finance workflow,
authenticated live gRPC transport, and G0-G14 completion gates remain open.

## G5-BIL-045 — typed Stripe entitlement and meter boundary

The bounded Stripe HTTP adapter now exposes typed create/list/retrieve
operations for Entitlement Features and Billing Meters, with fixed HTTPS
endpoints, pinned API version, bounded form fields, idempotency on mutations,
typed responses, and the existing redacted provider-error boundary. This is
only the provider transport slice; catalog publication/reconciliation,
webhooks, billing state, entitlements, and the broader Billing gate remain
open.

## G5-BIL-046 — remote Stripe catalog reconciliation

The typed Stripe catalog provider now performs bounded Product/Price list
reconciliation using ERPX catalog metadata, validates pinned API/account
response envelopes, reconstructs typed provider prices and identifiers, and
returns deterministic reconciliation snapshots. No live credentials are
embedded and provider failures remain redacted. Scheduled worker integration,
metadata onboarding for legacy remote objects, webhook replay controls, and
full Billing runtime adoption remain open.

## G10-MKT-046 — bounded Marketing journey domain

The domain now defines a tenant-scoped Journey graph with the campaign
lifecycle, bounded trigger/wait/channel/branch/segment-change/CRM-handoff/
conversion/action nodes, deterministic ordering/reachability, optimistic
transition facts, and resume-safe execution cursors with stable idempotency
keys. It intentionally has no provider side effects; durable worker
execution, consent/delivery enforcement, CRM handoff persistence, and UI
remain open.

## G10-MKT-047 — durable Marketing Journey tenant-write persistence

The authoritative tenant-write boundary now persists the complete Journey
aggregate with bounded status/scope/version columns and lossless JSON state.
Migration 0044 adds forced-RLS journey and command-idempotency tables. Typed
SQLx create/load/transition methods atomically commit the aggregate row,
generic snapshot, complete shared outbox envelope, and idempotency receipt;
exact retries return the original event receipt and optimistic lifecycle
versions fail closed. Journey creation starts at positive version one to match
the existing tenant-write/outbox contract. Live disposable PostgreSQL,
cross-holding isolation, forced-RLS metadata, workspace Clippy/tests, and
Folio verification passed. Durable worker execution, read projection, consent
delivery enforcement, providers, CRM side effects, UI, and the G10 gate remain
open.

## G5-BIL-047 — fail-closed remote Price ownership

Remote Stripe Price reconciliation now requires the complete ERPX ownership
binding (`erpx_catalog_revision`, `erpx_catalog_provider_ref`, and
`erpx_catalog_item`) before an active Price can be adopted. Unmanaged active
prices are ignored and conflicting item metadata fails closed. Focused
commercial tests and strict infrastructure Clippy pass. Worker scheduling,
metadata onboarding, webhook replay controls, full Billing runtime adoption,
and the broader G5 gate remain open.

## G10-MKT-048 — durable Marketing Journey execution

Tenant-write Journey execution cursors now have a durable version-zero seed,
bounded multi-worker leases, retry/dead-letter state, deterministic worker
idempotency, and atomic cursor/snapshot/outbox writes. The explicit worker
handler is tenant-routed from the server process and defaults to linear/wait
execution only; branch decisions remain fail-closed until a permission-aware
decision provider exists. Live PostgreSQL evidence covers lease takeover,
stale-owner rejection, exact replay, pause/resume, forced RLS, and isolation.
Consent delivery, providers, CRM effects, read projection, UI, and the wider
G10 gate remain open.

## G10-MKT-049 — consent and suppression delivery enforcement evidence

Independent live PostgreSQL evidence now proves that the tenant-write claim
boundary rechecks consent and suppression before provider authorization. Late
suppression cancels queued work, while revoked and exact-expiry consent produce
no provider claim. Append-only consent and forced-RLS contracts are asserted;
provider integrations, campaign/journey effects, CRM attribution, UI, and the
wider G10 gate remain open.

## G10-MKT-050 — bounded provider-neutral delivery adapter

The Marketing worker now has a provider-neutral HTTPS adapter with one
explicit route per supported channel, bounded response parsing, retry/permanent
classification, provider-message-reference enforcement, and zeroizing,
redacted credential material. The worker executable can be explicitly routed
to a tenant holding's physically separated write/read databases and refuses to
construct the delivery handler without paired route and credential
configuration. The adapter's local listener test proves an accepted provider
response becomes an explicit provider reference.

This packet deliberately does not close provider execution. The next
account-bound packet must replace the pilot environment resolver with
control-plane provider-account routing and a secret-manager resolver, add
provider idempotency headers and ambiguous-timeout reconciliation, and make
callback verification derive tenant scope from the authoritative delivery
rather than caller-provided headers. Vendor-specific adapters, campaign/
journey effects, CRM attribution, Finance reconciliation, Marketing UI, and the
wider G10 gate remain open.

## G10-MKT-051 — account-bound provider and callback routing

The control plane now has additive outbound provider identity and opaque
credential-reference fields. The explicit Marketing delivery worker requires
an active account binding for its tenant/holding before registration and
rechecks the exact account reference, provider key, credential reference, and
binding version before every provider call. Provider requests include the
delivery idempotency key header. Callback ingress resolves tenant/holding from
the provider account, loads the authoritative delivery from tenant write
state, derives scope from that delivery, and then applies the existing exact
provider-reference/inbox/outbox boundary. Customer-supplied scope headers are
not trusted.

The pilot environment credential resolver, vendor-specific adapters, full
account endpoint/onboarding lifecycle, ambiguous-timeout reconciliation,
end-to-end delivered callback proof, journeys, CRM/attribution, Marketing UI,
and the wider G10 gate remain open.

## G10-MKT-052 — end-to-end delivery and callback evidence

A disposable PostgreSQL proof now exercises the complete authoritative delivery
state path from granted consent through queue claim, explicit provider
acceptance, authenticated callback, and delivered state. It verifies exact
provider-reference binding, version progression, callback inbox/outbox effects,
forced-RLS scope, physically distinct tenant write/read databases, and exact
cleanup. The proof does not claim vendor adapters, secret-manager resolution,
journey effects, campaign scheduling, CRM/attribution, content, UI, or the
wider G10 gate.

## G10-MKT-053 — Marketing content and digital experience foundation

The domain contract and first persistence slice now cover tenant-scoped,
versioned assets and bilingual template revisions with bounded SEO/social
metadata, lifecycle transitions, rollback, expiring/revocable previews, forms,
and custom-domain scope rules. Tenant-write SQLx stores lossless snapshots,
immutable revision facts, optimistic versions, shared outbox envelopes, and
idempotency receipts, including form and custom-domain aggregate snapshots. A
separate tenant-read migration provides rebuildable content rows, checkpoints,
and event receipts. This packet remains open for domain binding/verification,
form submission commands, MinIO authorization, preview/publication workers,
transport, SSR, UI, and end-to-end digital experience behavior.

## G10-MKT-054 — Marketing signed content-object access

Marketing asset versions now have a tenant-scoped signed upload/download seam
over the existing MinIO adapter. It requires an already-authoritative access
context, binds the requested operation to the stored opaque object reference
and tenant namespace, rejects platform/cross-scope access, and bounds signed
URL lifetime to one hour. Live MinIO, server transport/audit integration,
preview workers, domain binding/verification, form submission, SSR/UI, and the
broader G10 gate remain open.

## G10-MKT-055 — Marketing content workflow persistence

Tenant-write persistence now covers published-version preview issue/revoke,
idempotent published-form submissions, hashed custom-domain challenge and
verification, and active-domain binding to a currently published template
revision. These commands append attributed outbox events and idempotency
receipts without placing raw preview tokens or submitted values in event
payloads. Public transport, preview expiry workers, workflow read projections,
typed form semantics, MinIO/server integration, SSR/UI, and the wider G10 gate
remain open.

## G10-MKT-056 — Marketing workflow read projection and expiry foundation

Tenant-read migration 0037 and replacement rebuild now project bounded
preview, form-submission, domain/binding, checkpoint, and event-receipt state
without token or verification hashes. A bounded tenant/holding expiry worker
seam and main-owned SQLx adapter use deterministic operation keys and
retry-stable command identity. Executable scheduling/registration, public
transport, verification adapters, typed semantics, MinIO/server integration,
SSR/UI, and the wider G10 gate remain open.

## G10-MKT-057 — Authenticated Marketing form submission transport

The Tonic server now exposes an authenticated `SubmitMarketingForm` command. It enforces session, command-context, tenant-scope, entitlement, and RBAC checks; rejects platform and subtree scopes; bounds and type-checks submitted JSON; routes through the tenant database router and SQLx content store; preserves idempotent write receipts; emits a customer-visible audit record; and returns redacted, correlation-linked errors. The service is registered in the modular server.

Evidence is recorded in `erpx/audits/G10-MKT-057/README.md`. Anonymous/public HTTP submission and active-domain lookup, preview and domain transport, executable worker scheduling, an application-layer Marketing port, semantic field validation, retention/erasure, live MinIO, SSR/UI, vendor/secret adapters, the remaining ERP contexts, migration/cutover, and Folio write retirement remain open.

## G10-MKT-058 — Public Marketing form and preview transport

The raw HTTP server now provides an anonymous form-submission route and expiring preview route. Public requests are tenant-registry routed, Host-bound to an active custom-domain binding, and served without tenant credentials. Form writes load the authoritative published form, enforce domain scope containment, use a bounded service-attributed command context, and retain idempotent SQLx/outbox behavior. Preview reads verify only a hash of the bearer token against write state, require the exact bound template revision, and read published content from the replacement read database. The executable worker registers bounded preview expiry polling through the tenant SQLx adapter.

Evidence is recorded in `erpx/audits/G10-MKT-058/README.md`. Public rate limiting/abuse controls, semantic field validation, SSR/UI, MinIO rendering, control-plane host-only tenant discovery, durable retry/DLQ, issuance/revocation and domain-management transport, the application-layer Marketing port, the remaining ERP contexts, migration/cutover, and Folio write retirement remain open.

## G10-MKT-059 — Marketing administration transport and application port

Authenticated Tonic commands now issue/revoke previews, issue/verify domain challenges, and bind verified domains to scoped template revisions. They use active authentication, command context, entitlement/RBAC, tenant routing, SQLx, customer-visible audit, redacted errors, exact published/binding rules, and random 256-bit secrets whose hashes alone are persisted. The application crate now defines typed, transport-neutral Marketing write commands and an object-safe orchestration port for form, preview, and domain workflows.

Evidence is recorded in `erpx/audits/G10-MKT-059/README.md`. The SQLx adapter is not yet mapped behind the application port; live all-command transport proof, public abuse controls, semantic validation, SSR/UI, MinIO, host-only discovery, durable retry/DLQ, remaining Marketing/ERP scope, migration/cutover, and Folio write retirement remain open.

## G10-MKT-060 — Application-to-SQLx Marketing write adapter

`SqlxMarketingContentWriteAdapter` now implements the application Marketing write port for form, preview, and domain workflows. It maps all receipts to transport-neutral metadata and redacts persistence errors. Infrastructure exports the adapter, and authenticated server Marketing writes now construct application commands and invoke the port while the SQLx store remains authoritative for re-reads, transactions, idempotency, outbox, and forced-RLS behavior. A disposable two-database proof exercises the adapter on form submission.

Evidence is recorded in `erpx/audits/G10-MKT-060/README.md`. Worker composition through the application port, live all-command transport proof, semantic validation, privacy, public abuse controls, SSR/UI, MinIO, host-only discovery, durable retry/DLQ, remaining ERP contexts, migration/cutover, and Folio write retirement remain open.

## G10-MKT-061 — Marketing form semantics and privacy retention

Form field validation now rejects malformed email, non-canonical phone, non-
finite/non-decimal number, non-exact consent, and required blank values at the
domain boundary without carrying submitted values in errors. Tenant-write form
submissions receive bounded retention metadata. A holding-scoped worker sweep
irreversibly replaces expired values with `{}`, records redaction metadata, and
appends a non-sensitive attributed redaction event. Replacement read rebuilds
carry the redacted state and cannot restore the submitted values. The worker is
registered as `marketing_content_privacy` and routes only tenant write state.

Evidence is recorded in `erpx/audits/G10-MKT-061/README.md`. Profile-level
consent and erasure, suppression and delivery enforcement, public abuse
controls, SSR/UI, live MinIO, host-only discovery, durable retry/DLQ, remaining
Marketing/ERP contexts, migration/cutover, and Folio write retirement remain
open.

## G10-MKT-062 — Marketing profile erasure and suppression

Marketing profiles now have a typed, contact-free erasure tombstone. The
authoritative write path removes contact and attribute data, freezes the
profile, creates permanent profile-erasure suppression, and scrubs all
replayable profile outbox payloads while preserving event identity and audit
metadata. The replacement read projector handles scrubbed historical events
from an empty read database. Evidence is recorded in
`erpx/audits/G10-MKT-062/README.md`.

The application/Tonic authorization boundary must still invoke the domain
scope check before exposing profile erasure. Queued delivery payload scrubbing,
public abuse controls, SSR/UI, live MinIO, host-only discovery, durable
retry/DLQ, remaining Marketing/ERP contexts, migration/cutover, and Folio write
retirement remain open.

## G10-MKT-063 — Marketing profile-erasure application and Tonic authorization

Profile erasure is now exposed through a transport-neutral application port and
an authorized Tonic command. Scope containment, bounded metadata, current
authentication/RBAC/entitlement/billing/SoD, tenant routing, customer-visible
audit, and redacted receipts/errors are enforced before the authoritative SQLx
redaction transaction. Evidence is recorded in
`erpx/audits/G10-MKT-063/README.md`.

Queued delivery payload scrubbing, public abuse controls, SSR/UI, live MinIO,
host-only discovery, durable retry/DLQ, remaining Marketing/ERP contexts,
migration/cutover, and Folio write retirement remain open.

## G10-MKT-050 — Account-bound Marketing provider delivery

The Marketing worker now validates exact account/tenant/holding/provider
bindings, propagates provider idempotency and non-secret account context,
rejects unsafe routes/redirects, bounds response streaming, redacts provider
failures, and fails closed on binding changes or unbound account calls.
Evidence is recorded in `erpx/audits/G10-MKT-050/README.md`.

Production secret-manager composition, vendor-specific adapters, durable
account-bound callback routing, and full provider reconciliation remain open.

## G10-MKT-064 — Anonymous Marketing abuse controls

The anonymous Marketing form and preview ingress now has a pure domain abuse
policy and a durable control-plane resource fence. The policy binds tenant,
resource, and hostname-derived domain identities, accepts only bounded opaque
fingerprints, handles honeypot/challenge signals, and redacts diagnostics. The
server derives fingerprints with a configured HMAC key and fails closed when
the key is absent. Control migration 0047 atomically counts normalized
tenant/holding/hostname/resource minute windows without storing IPs, contacts,
tokens, or request bodies, and the quota is consumed before tenant routing.

Evidence is recorded in `audits/G10-MKT-064/README.md`. Trusted-proxy
deployment configuration, an external challenge provider, and scheduled
counter cleanup/reconciliation remain open; the wider G10 gate, remaining ERP
contexts, migration/cutover, and Folio write retirement remain open.

## G10-MKT-064-B — Durable Marketing abuse-window cleanup worker

The worker now exposes an opt-in, bounded control-plane cleanup handler with a
24-hour retention cutoff. It calls only the durable abuse-window purge port,
returns cleanup counts, rejects invalid worker/batch configuration, and maps
database failures to redacted worker errors. The executable registers it only
for `ERPX_WORKER_HANDLER=marketing_public_abuse_cleanup`; tenant databases and
client identity are never accessed.

Evidence is recorded in `audits/G10-MKT-064-B/README.md`. Durable scheduling,
lease/DLQ integration, trusted-proxy deployment configuration, and external
challenge provider integration remain open; the wider G10 gate, remaining ERP
contexts, migration/cutover, and Folio write retirement remain open.

## G10-MKT-065 — Marketing callback secret-resolution boundary

Marketing provider callbacks now validate an opaque secret-manager reference,
resolve it through an injected boundary, verify the exact raw request body, and
zeroize the resolved material after signature verification. The control-plane
provider-account binding remains authoritative and tenant routing starts only
after verification. The host-native pilot supports only `env:NAME`; manager
schemes fail closed until a production adapter is injected.

Evidence is recorded in `audits/G10-MKT-065/README.md`. Production
secret-manager integration and rotation/reconciliation remain open; the wider
G10 gate, remaining ERP contexts, migration/cutover, and Folio write
retirement remain open.

## G10-MKT-066 — Production Marketing callback secret-manager adapter

The server can now compose a bounded HTTPS secret-manager gateway for
`http`/`secret-manager` callback resolver modes. The gateway uses a fixed
versioned path and one encoded opaque-reference segment, disables redirects,
limits timeout and response size, accepts only an allow-listed JSON response,
and zeroizes resolved material. Pilot `env` and explicit unconfigured modes
remain available.

Evidence is recorded in `audits/G10-MKT-066/README.md`. Vendor-native AWS/Vault
adapters, endpoint health/rotation reconciliation, deployment identity
binding, and live TLS proof remain open; the wider G10 gate, remaining ERP
contexts, migration/cutover, and Folio write retirement remain open.

## G10-MKT-067 — Marketing delivery secret-manager bridge

The Marketing delivery worker now accepts an injected credential resolver and
shares the production callback secret-manager boundary. Pilot env,
production HTTP manager, and explicit unconfigured modes are separate and
fail closed. Delivery credentials accept validated manager references and
retain zeroizing/redacted handling while existing account binding and
idempotency checks remain authoritative.

Evidence is recorded in `audits/G10-MKT-067/README.md`. Vendor-native provider
SDKs, channel onboarding, provider reconciliation, durable delivery DLQ
scheduling, and live external provider evidence remain open; the wider G10
gate, remaining ERP contexts, migration/cutover, and Folio write retirement
remain open.

## G10-MKT-068 — Multi-tenant Marketing delivery execution

The Marketing delivery executable now enumerates active holdings and outbound
provider accounts from authoritative control-plane state, applies bounded
multi-tenant/global-batch execution, routes separate tenant write/read pools,
and runs the existing account-bound durable delivery handler for each account.
Environment variables no longer select a single tenant or provider account.

Evidence is recorded in `audits/G10-MKT-068/README.md`. A dedicated
control-plane scheduler job/lease/DLQ for the Marketing delivery handler,
vendor-native provider reconciliation, channel onboarding, live provider
evidence, and the wider G10/ERP migration gates remain open.

## G10-MKT-069 — Durable Marketing delivery scheduler

The Marketing delivery worker is now driven by a durable control-plane
recurring job. The control migration and repository provide idempotent
scheduling, bounded `SKIP LOCKED` claims, half-open lease renewal, recurring
completion, bounded retry backoff, terminal dead-lettering, and explicit
dead-letter replay. The worker wrapper renews the control lease around the
existing multi-tenant tenant-write delivery pass; tenant delivery state remains
authoritative in each holding's write database.

Evidence is recorded in `audits/G10-MKT-069/README.md`. Vendor-native provider
reconciliation, channel onboarding, live external provider evidence, and the
wider G10/ERP migration gates remain open.

## G10-MKT-070 — Marketing provider idempotency reconciliation

Provider routes may now declare a separate bounded HTTPS reconciliation
endpoint. Account-bound ambiguous submissions query provider idempotency state
with only dispatch identity, idempotency key, channel, and non-secret account
metadata; they never resend the dispatch record. A strict status allow-list and
bounded accepted provider reference keep malformed or unsafe responses fail
closed.

Evidence is recorded in `audits/G10-MKT-070/README.md`. Vendor-native provider
SDKs, account onboarding, live external-provider evidence, channel completion,
and the wider G10/ERP migration gates remain open.

## G10-MKT-071 — Marketing provider onboarding and channel capability lifecycle

The control plane now owns a bounded provider-account onboarding lifecycle.
Configuration stores only opaque secret-manager references, starts accounts
`suspended`/`pending`, normalizes and persists the enabled email, SMS, social,
push, and webhook channels, and invalidates verification when configuration
changes. An explicit audited verification command records only a hash of the
external verification reference; activation is rejected until the account is
verified. Suspension and reactivation use optimistic versions. Tonic exposes
configure, verify, and state commands, and the delivery boundary rejects a
dispatch outside the account's onboarded channel set.

Evidence is recorded in `audits/G10-MKT-071/README.md`. Vendor-native SDKs,
external provider verification, live provider evidence, broader Marketing and
ERP scope, migration/cutover, and Folio write retirement remain open.

## G10-MKT-072 — Provider account-bound channel routes

Provider onboarding now persists normalized, per-account channel route metadata
in control-plane state. Each configured channel requires one credential-free
HTTPS endpoint, with an optional credential-free HTTPS reconciliation endpoint.
The worker constructs its account-bound route map from the authoritative
control-plane binding; route query strings, fragments, userinfo, non-HTTPS
schemes, duplicate channels, and capability/route mismatches fail closed.

Evidence is recorded in `audits/G10-MKT-072/README.md`. Vendor-native SDKs,
live provider verification, production secret-manager deployment, broader
Marketing and ERP scope, migration/cutover, and Folio write retirement remain
open.

## G10-MKT-073 — Channel-scoped Marketing delivery claims

The tenant delivery repository now applies each active provider account's
normalized channel set while claiming due work. The multi-tenant worker wires
one filtered source per control-plane account, so an account cannot lease a
different channel's dispatch before provider execution. Filters are bounded to
the five supported channels and canonicalized before SQL binding.

Evidence is recorded in `audits/G10-MKT-073/README.md`. Multiple-account
priority policy, vendor-native SDKs, live provider evidence, broader Marketing
and ERP scope, migration/cutover, and Folio write retirement remain open.

## G10-MKT-074 — Deterministic multi-account channel selection

Provider-account discovery now orders active accounts by bounded ascending
priority and then opaque provider-account reference. Priority is persisted in
the control-plane onboarding aggregate, exposed only as safe routing metadata,
and validated before worker construction. A zero transport value maps to the
bounded default priority for compatibility; all other values must be within
the explicit range.

Evidence is recorded in `audits/G10-MKT-074/README.md`. Live provider failover,
weighted traffic policy, vendor-native SDKs, broader Marketing and ERP scope,
migration/cutover, and Folio write retirement remain open.

## G2-IAM-003 — Universal effective-access intersection

The shared write-side evaluator now binds the resolved session principal to the
role assignment and applies authoritative purchased-entitlement state plus
company/branch activation state for scoped access. Company and branch requests
fail closed without an allocation snapshot; branch requests additionally
require explicit branch activation. Existing half-open role windows, parent
cascade, approval limits, billing/read-only policy, support capability, and
segregation-of-duties checks remain in the same evaluator. Server authorization
paths now pass the control-plane allocation book into this boundary.

Evidence is recorded in `audits/G2-IAM-003/README.md`. Durable session-active
resolution, recursive parent-chain loading, authoritative approver identity
resolution, support-capability lookup, universal adoption by every remaining
mutation, role-review notification delivery, recovery replay persistence, and
the broader G2 gate remain open.

## G2-IAM-004 — Recursive parent-chain authorization

The effective-access boundary now accepts and rechecks the complete
immediate-to-root assignment chain. Every parent edge is validated at the
request instant for identity, expiry, scope containment, authority, and
permission containment; missing ancestors and cycles fail closed. The SQLx
control repository returns the validated chain from authoritative write-side
rows/snapshots, while legacy callers retain the immediate-parent adapter. The
universal server authorization paths use the complete chain, so an expired or
revoked ancestor immediately invalidates every descendant.

Evidence is recorded in `audits/G2-IAM-004/README.md`. Support-intersection
parent-chain transport, complete universal mutation adoption, durable session
resolution, authoritative approver identity resolution, role-review delivery,
recovery replay persistence, and the broader G2 gate remain open.

## G2-IAM-005 — Durable role-review notification delivery

Role-expiry review notifications now have an explicit worker handler boundary.
The control-plane persistence reclaims expired leases, reconciles successfully
published notification keys under the original claim, and releases unfinished
keys for retry. The handler uses bounded horizon/lease configuration and an
injected publisher, so no provider credential or unverified external transport
is embedded in the worker. The default registry remains fail-closed; an
explicit runtime composition method is required to install a publisher.

Evidence is recorded in `audits/G2-IAM-005/README.md`. Production email/push
provider delivery, notification template/localization, recipient policy,
universal mutation adoption, authoritative approver identity resolution,
recovery replay persistence, and the broader G2 gate remain open.

## G2-IAM-006 — Authoritative approver identity resolution

High-risk role renewal and final-role recovery now resolve each supplied
approver/reviewer reference against active identity state and current
write-side role assignments in the requested scope. Approvers require
`role.assignment.approve`; independent reviewers require
`role.assignment.review`. Self-approval, inactive principals, missing roles,
and out-of-scope identities fail closed before the role command is dispatched.

Evidence is recorded in `audits/G2-IAM-006/README.md`. A separate authenticated
approval-artifact workflow, universal adoption by every remaining mutation,
role-review delivery policy, recovery replay persistence, and the broader G2
gate remain open.

## G2-IAM-007 — Durable recovery approval receipts

Final-role recovery now records an approved, hash-bound receipt in the
control-plane write database after the authoritative role command succeeds.
The receipt binds the recovery request, assignment version, target, scope,
requester, independently authorized approver/reviewer, decision, command
idempotency key, actor, and correlation. Receipt-version allocation is
transactionally serialized per request. Reusing an idempotency key with a
different recovery request, assignment version, or decision fails closed;
replaying the exact command returns the existing receipt without another row.

Evidence is recorded in `audits/G2-IAM-007/README.md`. This packet does not
claim a separate pre-authorization workflow for ordinary role renewal,
notification provider policy, or completion of G2-G14.

## G2-IAM-008 — Universal mutation authorization route audit

The complete authenticated server mutation inventory is now checked against
the authoritative boundary. Tenant mutations use the shared entitlement/RBAC
intersection; platform/control-plane mutations use explicit platform role
authorization; role lifecycle commands use their dedicated lifecycle policy.
The public Marketing form is intentionally a separate bounded acquisition
ingestion path with consent, suppression, deduplication, and abuse controls.

Evidence is recorded in `audits/G2-IAM-008/README.md` and its executable route
inventory. The G2 authorization-adoption requirement is closed. Remaining
G2-G14 product, platform-target, migration, and Folio-retirement requirements
remain open.

## G8-FIN-047 — Authenticated Finance period and journal posting transport

Finance now exposes authenticated Tonic commands for period persistence and
immutable journal posting. Both require an explicit non-platform scope, bind
the journal/period/chart identity to the command context, enforce Finance
entitlement/RBAC through the shared write-side boundary, route only the tenant
write database, and write audit records. The posting command delegates to the
existing atomic SQLx posting transaction, which locks the period, loads the
authoritative chart/accounts, rejects closed periods and invalid journals,
persists the posted journal/snapshot/outbox/idempotency receipt, and returns
safe scalar receipt metadata.

Evidence is recorded in `audits/G8-FIN-047/README.md`. AP/AR, bank/cash,
close, consolidation, Thai tax, migration reconciliation, Finance UI, and the
broader G8 gate remain open.

## G13-MIG-001 — Ordered migration run and cutover guard

The migration crate now contains a bounded, serializable `MigrationRun` state
machine covering source freeze, deterministic ID mapping, write backfill, read
projection, shadow replay, reconciliation, read-only pilot, workflow
acceptance, rollback rehearsal, mutation freeze, final delta, ERPX command
ownership, Folio read-only retention, and final Folio retirement. It advances
one phase at a time, derives command ownership from the phase, validates the
source-contract hash and opaque references, and refuses retirement until an
explicit retention window has elapsed. This is a transport-neutral cutover
guard; it does not claim live Folio extraction, physical backfill, cohort
orchestration, or production command-owner persistence.

Evidence is recorded in `audits/G13-MIG-001/README.md`.

## G8-FIN-048 — Bounded authenticated Finance trial-balance read

Finance now exposes `GetFinanceTrialBalance` as a bounded, authenticated
read. It requires explicit tenant/holding/company scope, `finance.read`, and
customer-scope containment; rejects platform and subtree scopes; and routes
only to the tenant read database. A bounded typed chart snapshot is
cross-checked account-for-account against the Finance chart read projection
before the existing SQLx trial-balance calculation runs. The response exposes
deterministic scalar account rows, debit/credit totals, chart identity,
read-only state, projection timestamp/freshness, source version, and
correlation metadata. Persistence details and secrets are redacted, and the
read is audit-recorded.

Evidence is recorded in `audits/G8-FIN-048/README.md`. Finance UI, AP/AR,
bank/cash, close, consolidation, Thai tax, migration reconciliation, and the
broader G8 gate remain open. The legacy domain currency and 500-journal
bounded-read limitations remain explicitly identified for later infrastructure
work.

## G13-MIG-002 — Read-only Folio source extractor

The migrator now has a live, read-only PostgreSQL extractor for the legacy
Folio database. It runs one `SET TRANSACTION READ ONLY` transaction, records
bounded table counts, status distributions, money totals, object-byte totals,
missing-table inventory, and deterministic ID mappings represented only by
source fingerprints and opaque ERPX UUIDs. A canonical source-contract hash
binds the aggregate snapshot while ignoring capture time. The CLI accepts only
an environment-variable name for the database URL and never emits that URL or
source payloads. No Folio or ERPX write is performed.

Evidence is recorded in `audits/G13-MIG-002/README.md`. Physical backfill,
shadow replay, explained target reconciliation, cohort orchestration,
mutation freeze, command-owner persistence, and Folio write retirement remain
open.

## G6-PRV-023 — PostgreSQL/MinIO backup and restore-test adapter

The infrastructure boundary now contains a concrete, credential-redacting
`PostgresMinioBackupAdapter` and worker `BackupRuntime`. Full backups use
argv-only `pg_dump`, bounded private artifacts, MinIO presigned transfers,
tenant/chain/backup-key binding, checksum-verified replay, and typed failure
classification. Restore tests use a distinct candidate database, bounded
`pg_restore`, schema/RLS verification, and the existing provisioner-owned
fenced read rebuild/activation path. Missing configuration fails closed.

Focused fake-backed evidence is recorded in `audits/G6-PRV-023/README.md` and
`crates/infrastructure/tests/backup_adapter.rs`.

This packet intentionally rejects `BackupRunKind::Pitr` because the current
shared contract has no recovery point/LSN, WAL archive manifest, or physical
cluster restore target. The adapter does not mislabel a logical dump as PITR.
True PostgreSQL PITR, live PostgreSQL/MinIO integration, durable backup-job
registration in the worker executable, complete Platform Operations actions,
and the G6 gate therefore remain open.

## G6-PRV-024 — Authenticated tenant provisioning transport

Platform Operations now exposes bounded `BeginTenantProvisioning` and
`ProvisionTenantResources` commands. Both require an authenticated platform
scope, actor binding, the dedicated `platform.tenant.provision` role, and a
platform audit record. The first persists the opaque registry/provisioning
state; the second invokes server-only PostgreSQL write/read provisioning and
the existing MinIO namespace verification boundary. Responses contain only
safe state/readiness metadata.

Evidence is recorded in `audits/G6-PRV-024/README.md`. Tenant activation,
suspension/recovery, owner seeding, backup/PITR scheduling/history, executable
worker recovery, integrations/settings/audit reads, and the G6 gate remain
open.

## G6-PRV-025 — Durable backup scheduling, history, and worker execution

Control-plane backup jobs now have durable idempotent scheduling, bounded
database-clock leases, SKIP-LOCKED worker claims with exact tenant/holding
binding, retry/dead-letter transitions, durable job/attempt/run attribution,
and cursor-paged safe read-side history projection. The `tenant_backup` worker
executes full backup jobs through the PostgreSQL/MinIO adapter and restore-test
jobs through a tenant-bound candidate database and fenced routed read
activation. Platform transport requires the dedicated provisioning permission
and returns only safe metadata.

Evidence is recorded in `audits/G6-PRV-025/README.md`. True PITR remains
intentionally fail-closed because WAL/recovery-point and physical restore
contracts are not yet present. Tenant lifecycle completion, retention/cleanup,
live disposable restore evidence, remaining Platform Operations surfaces, and
the G6 gate remain open.

## G6-PRV-026 — Authenticated tenant lifecycle transitions

Tenant lifecycle transitions now update the registry and provisioning records
atomically. Activation is health-gated; suspension and recovery use dedicated
platform permissions, actor binding, and audit records. Suspended and recovery
tenants are excluded from new backup-worker claims until activation, and tenant
routing checks the active registry state.

Evidence is recorded in `audits/G6-PRV-026/README.md`. Owner seeding,
activation-time organization initialization, recovery restore orchestration,
retention cleanup, remaining Platform Operations surfaces, and the G6 gate
remain open.

## G6-PRV-027 — Bounded backup retention metadata cleanup

The control plane now has a tenant/holding-bound, idempotent retention action
aggregate with validated cutoff, row, attempt, dry-run, and lease bounds. It
can select only terminal backup job history and delete linked metadata in one
transaction, returning bounded secret-free counts and a rebuildable read
projection. Disposable two-control-database SQL evidence covers idempotency,
scope binding, dry-run behavior, cleanup, and redaction.

This packet is intentionally partial. Read projection pagination and stale-row
clearing, a durable failure/retry transition, worker execution, physical
MinIO object deletion, true PITR, and the G6 gate remain open. Evidence is
recorded in `audits/G6-PRV-027/README.md`.

## G6-PRV-028 — Retention lifecycle hardening and worker registration

The retention projection now rebuilds atomically through all keyset pages,
completion validates leases with the database clock, and claimed actions have
an authoritative retry/terminal-failure transition. The `tenant_backup_retention`
worker claims due actions with bounded control-plane queries and records
processed/retry/terminal counters. Fresh disposable control-plane proofs cover
the SQL lifecycle and worker execution.

Physical MinIO object deletion remains fail-closed because metadata counts are
not object-delete authority. True PITR, owner seeding, remaining Platform
Operations actions, and the G6/G14 gates remain open. Evidence is recorded in
`audits/G6-PRV-028/README.md`.

## G6-PRV-029 — Tenant-bound backup artifact retention

The retention worker now selects claim-bound backup artifact locators and
requires the configured PostgreSQL/MinIO adapter for every non-dry-run action.
Deletion is namespace/tenant/chain/reference bound, short-lived, redacted, and
404-idempotent; metadata completion follows successful object cleanup. The
production backup target also carries the registry-owned storage namespace.
Evidence is recorded in `audits/G6-PRV-029/README.md`.

Credentialed live MinIO artifact proof, PostgreSQL PITR/WAL recovery, backup
reconciliation, owner seeding, remaining Platform Operations actions, and the
G6/G14 gates remain open.

## G6-G12-UI-013 — Authenticated Platform audit read surface

`/platform/audit` now has an authenticated Tonic/gRPC-Web read adapter backed
by an allow-listed control-plane audit query. Platform scope and the existing
tenant-read permission are enforced server-side; the read is itself audited
and pagination uses a bounded `(recorded_at, audit_id)` cursor. Payload JSON
and customer identifiers are excluded from the protobuf and Dioxus models.
Loading, denied, error, empty, and read-only states are explicit. Evidence is
recorded in `audits/G6-G12-UI-013/README.md`.

Remaining integrations/settings routes, mobile target builds, and the
G6/G12/G14 gates remain open.

## G6-PRV-030 — Retention claim renewal during physical cleanup

Retention cleanup now renews the database-clock claim immediately before
each bounded tenant-bound object deletion. Renewal rechecks the exact claim,
worker, tenant/holding binding, active state, and unexpired lease; a lost
claim stops further physical work and uses the existing redacted retry path.
Evidence is recorded in `audits/G6-PRV-030/README.md`.

Credentialed MinIO proof, object inventory reconciliation, PostgreSQL
PITR/WAL recovery, owner seeding, remaining Platform Operations actions, and
the G6/G14 gates remain open.

## G6-G12-UI-014 — Platform integrations and settings read surfaces

`/platform/integrations` and `/platform/settings` now use authenticated,
audited Tonic/gRPC-Web reads. Integrations expose only categorical health and
capability labels; settings expose only safe architecture metadata. Neither
surface exposes endpoints, credentials, tokens, database keys, provider
payloads, mutable flags, or operational commands. Evidence is recorded in
`audits/G6-G12-UI-014/README.md`.

Provider onboarding/live proof, mobile targets, owner seeding, true PITR/WAL,
and the remaining G6/G12/G14 gates remain open.

## G6-PRV-031 — Tenant-bound backup object inventory reconciliation

Retention now obtains a bounded, signed S3-compatible inventory for the
tenant's backup-chain prefix, rejects partial provider pages, compares the
inventory with control-plane references selected by the active retention
claim, and persists redacted matched/drifted/unavailable evidence. The report
is rebuildable into the physically separate control read database. Evidence is
recorded in `audits/G6-PRV-031/README.md`.

Credentialed live MinIO proof remains open because backup credentials are not
configured in the current environment. True PostgreSQL PITR/WAL recovery,
owner seeding, provider onboarding, mobile targets, and the remaining G6/G12/G14
requirements remain open.

## G6-G12-UI-015 — Mobile target compilation evidence

The shared Dioxus console now has direct cross-target compilation evidence for
`aarch64-apple-ios` and `aarch64-linux-android`, and repository CI preserves
both checks. This closes compilation evidence only; device execution, secure
keystore/keychain persistence, camera/scanner, notifications, offline drafts,
and mobile role journeys remain open. Evidence is recorded in
`audits/G6-G12-UI-015/README.md`.

## G6-PRV-032 — Two-person tenant owner seeding

Tenant provisioning now requires an owner principal and an independent active
platform approver. Control migration 0057 persists immutable, tenant-bound
owner-seed metadata. Resource provisioning atomically creates a
recovery-required identity, a hashed one-time recovery code, and a finite
90-day holding-owner assignment with independent approval. Replays do not
reissue recovery material, and the control metadata is absent from the control
read database. Evidence is recorded in `audits/G6-PRV-032/README.md`.

Organization aggregate initialization in the per-holding tenant write schema,
credentialed provider proof, true PITR/WAL recovery, and the G6/G12/G14 gates
remain open.

## G6-PRV-033 — Activation-time organization root initialization

Provisioning now initializes the tenant-local holding root after the physical
write/read databases migrate. The write side stores one deterministic
`organization.holding` snapshot and event; the separate read side stores the
matching Operations projection row and checkpoint under forced holding RLS.
Replay is idempotent and the event remains the source fact, so this bootstrap
does not create a long-term dual-write path. Evidence is recorded in
`audits/G6-PRV-033/README.md`.

Legal company, headquarters, branch, department, team, and delegated
administration workflows remain open along with the G6/G7/G14 gates.

## G7-001 — Organization hierarchy command/read boundary

The tenant-local `OrganizationService` now exposes authenticated save/list
transport for typed Holding, Company, Headquarters, Branch, Department, and
Team aggregates. Commands require the active organization role and contained
tenant scope, route to the physical holding write/read databases, use
deterministic idempotent Operations events with optimistic versions, and
project only rebuildable read rows. Holding identity is checked against the
opaque tenant registry reference and read rows are rejected if their holding
reference disagrees with the route. Evidence is recorded in
`audits/G7-001/README.md`.

Reporting lines, delegated administration, composite constraints, and the
remaining G7 Operations surfaces remain open.

## G7-002 — Operations query boundary and reconciliation

`OperationsService` now provides an authenticated, tenant-scoped summary and
bounded active-exception read. The summary explicitly separates write-side
event/provenance counts from read-side projection/checkpoint counts, providing
the first successor reconciliation surface while preserving source-context
ownership. The service routes only through the opaque tenant registry to the
physical holding write/read database pair and fails closed on scope, state,
holding, route, cursor, and persistence errors. Evidence is recorded in
`audits/G7-002/README.md`.

Source-context KPI adapters, Folio/ERPX total comparison fixtures, the tenant
Dioxus Operations surface, exception lifecycle workers, and remaining G7/G14
requirements remain open.

## G7-004 — Operations exception lifecycle transport

`TransitionOperationsException` now reaches the existing tenant SQLx lifecycle
transaction through authenticated Tonic/gRPC-Web. It enforces tenant scope,
actor matching, `operations.exception.manage`, positive optimistic versions,
and persisted exception scope containment. Actor, assignee, and manager
validity windows are resolved from the authoritative control-plane role chain;
the client cannot extend or fabricate them. Assignment SLA fields are bounded,
and the transport returns only transition metadata. Evidence is recorded in
`audits/G7-004-TRANSPORT/README.md`.

Detection producers, SLA workers/notifications, expired-user reassignment,
manager hierarchy, projection/UI adapters, Folio reconciliation, and the
remaining G7/G14 requirements remain open.

## ERPX parallel completion wave — directory and product reconciliation

The coordinator integrated the directory identity boundary for generic LDAP,
on-premises Microsoft Active Directory Domain Services, and Univention
Corporate Server. The write-side identity migration now persists platform and
holding connector metadata, bounded import jobs/pages/members, immutable links,
append-only role facts, five-minute revocation SLA rows, rebuildable identity
projection state, and redacted directory outbox events. Native feed contracts
cover RFC 4533, AD DirSync, and UCS Provisioning Service; the worker registers
`directory_sync` but refuses to claim work until a real external adapter is
configured. Authenticated DirectoryService RPCs cover connector reads/writes,
import start, and role-mapping reads. Evidence is recorded in
`audits/DIRECTORY-INTEGRATION/README.md`.

The G8 Finance, G9-G11 product reconciliation, and G12 UI/QA lanes delivered
bounded slices with coordinator integration hooks. These slices do not claim
live disposable two-database reconciliation, Android/iOS device execution,
or external LDAP/AD/UCS vendor certification until those environments and
credentialed acceptance runs are available. Production directory activation,
Folio write retirement, command-owner changes, and final cutover remain
explicitly prohibited without product-owner approval.

## ERPX parallel completion verification — 2026-07-27

The completion wave also integrated the G3-G6 platform lifecycle slice and the
G7 Operations completion worker seam. Platform lifecycle, tenant replacement
read fencing, backup/PITR fail-closed handling, connector revocation, health
reconciliation, Operations outbox repair, bounded projection catch-up, expired
principal reassignment, SLA escalation, and reconciliation contracts are
present with SQLx/projector/API/UI evidence packages. The worker composition
root registers `operations_completion` only when it receives an explicitly
serialized authorized pass; missing proof fails closed.

The current verification wave passes workspace formatting, warnings-denied
workspace Clippy, workspace tests, protobuf/package and G13 structural
compatibility checks, Dioxus web/desktop/mobile/iOS/Android target checks,
`bun run lint` (four pre-existing warnings, zero errors), and `bunx tsc
--noEmit`. The complete ignored suite was exercised with `--no-fail-fast` and
queued 50 database-backed targets because the required disposable control,
tenant, read, and rollback database references are unset. Native iOS/Android
device execution, live LDAP/AD DS/UCS certification, million-user volume,
five-minute live revocation timing, G13 cutover rehearsal, and G14 acceptance
remain open. Folio is still the operational write owner and is live on port
3004; no customer directory was activated and no production cutover occurred.

## ERPX coordinator integration continuation — 2026-07-27

The worker composition root now registers the G3-G6 platform completion
runtime, G8 Finance completion runtime, and G11 HR/payroll completion runtime
behind explicit proof/configuration boundaries. The tenant projection scheduler
also has fail-closed product kinds for SCM, POS, Sales, Asset/Maintenance,
Legal/Documents, HR, and Marketing; each uses the existing tenant-routed SQLx
runner, bounded cursors/pages, retry classification, and disposable read-model
rebuild path. Candidate/replacement payloads are rejected by product runners
that do not expose a source-fenced activation contract.

The server composition now constructs the platform completion API from the
control-plane store and maps the authenticated platform tenant detail read to
its proof-bound reconciliation read model, preserving the safe categorical
protobuf response. No database keys, credentials, provider material, or raw
SQL errors cross the transport boundary.

These changes close local composition gaps only. Live two-database SQLx runs,
credentialed LDAP/AD DS/UCS certification, one-million-user volume, native
device execution, five-minute revocation timing, provider acceptance, and G13
rollback/cutover rehearsal remain queued. Folio remains the operational write
owner and all activation, command-owner, and production cutover actions remain
approval-gated.

## ERPX coordinator transport and directory UI continuation — 2026-07-27

The coordinator added typed Sales, SCM, and Asset/Maintenance aggregate
transports to the protobuf contract and registered authenticated server RPCs.
Commands decode bounded domain aggregates and route source writes through the
tenant write database; queries route to the read database, enforce explicit
scope and product permissions, record audit events, and return bounded JSON
read facts with source versions. Existing SQLx stores remain responsible for
validation, idempotency, outbox delivery, projection, and reconciliation.

The Dioxus console now exposes the authorized directory administration route.
The WebAssembly adapter loads platform and holding connector summaries plus
directory-managed role mappings, validates vendor-native feeds and TLS-only
transport, and renders only the redacted read-only contract surface. Non-web
targets fail closed until a target-specific transport exists.

Sales, SCM, and Asset/Maintenance reconciliation packets added source/read
integrity checks, forged-event rejection, and ignored live acceptance harnesses.
Their live two-database evidence remains queued when the required test database
references are absent. No external directory was activated and no command-owner
or production cutover action was taken.

## ERPX coordinator G11 product-context shell completion — 2026-07-27

The console now wires Sales, Procurement, Inventory, Asset/Maintenance, HR/Payroll,
Marketing/CRM, POS, and Legal/Documents into actual shell route state, route tabs,
deep-link paths, and AppShell navigation. Each route consumes the authenticated
projection-status RPC and renders the bounded `ProductCompletionSurface` with
explicit loading, empty, error, offline, stale-freshness, and read-only states.
The surface is marked as an authoritative projection-status read model; it does
not fabricate product rows or expose mutations. The G11 acceptance verifier now
requires context-specific route markers and fails on missing shell wiring rather
than logging an `OPEN` status.

This closes the local G11/G12 shell integration gap. Product-specific live SQLx
read evidence, external vendor certification, native device execution, and
production/cutover evidence remain queued under the existing blockers. Folio
remains the operational write owner.

## ERPX coordinator final local verification wave — 2026-07-27

The coordinator reran format, warnings-denied workspace Clippy, the full
non-ignored workspace suite, protobuf compilation, G11/G13/G14 focused acceptance,
all Dioxus web/desktop/host-mobile/iOS/Android target checks, Folio lint and
TypeScript checks, and the port-3004 runtime probe on the current tree. The
non-ignored suite is green. The ignored suite was exercised with `--no-fail-fast`
and 51 database-backed targets stopped at their explicit missing disposable
database URL guards; this is recorded as an environment blocker, not accepted
as live SQLx evidence.

Live LDAP/AD DS/UCS certification, one-million-user volume, measured five-minute
revocation, disposable two-database reconciliation, G13 migration/rollback
rehearsal, native device execution, and final G14 vendor acceptance remain open.
No customer directory was activated, Folio writes were not retired, and no
production command-owner or cutover action was performed.

## ERPX final live disposable verification — 2026-07-27

The coordinator provisioned fresh disposable PostgreSQL control-write,
control-read, tenant-write, tenant-read, and isolated commercial-worker
databases and reran the complete ignored workspace suite with
`--no-fail-fast -- --ignored --nocapture --test-threads=1`. The suite exited
successfully. Identity persistence, commercial semantics, Finance legacy
backfill and posting, Operations, Customer 360, all G9-G11 product persistence
and projection paths, provisioning/replacement, support, scheduler, server
owner seed, and worker projection targets all passed against live SQLx
databases. The isolated commercial database is selected by the optional
`ERPX_TEST_COMMERCIAL_DATABASE_URL` test setting; it prevents unrelated Stripe
fixtures from contaminating the worker semantic acceptance test.

The final local wave also passed format, warnings-denied workspace Clippy, the
non-ignored workspace suite, `cargo check -p erpx-proto --offline`, the G11
Asset/Product/Sales focused acceptance tests, G13 deployment/recovery and
parallel acceptance, G14 directory acceptance, all five Dioxus targets,
`bun run lint` (four pre-existing warnings and zero errors), `bunx tsc --noEmit`,
and `git diff --check`. Folio `/operations` returned the authenticated HTTP
307 boundary while the development server remained on port 3004.

This live disposable run closes local SQLx evidence; it does not close vendor
or production acceptance. Credentialed generic LDAP/RFC 4533, on-prem AD DS /
DirSync, and UCS Provisioning Service certification; one-million-user volume;
measured five-minute revocation; native iOS/Android device execution; vendor
provider acceptance; G13 source/target/rollback rehearsal; and final G14
acceptance remain queued. No customer directory was activated, no production
command owner changed, Folio writes were not retired, and no production
cutover was performed.

## ERPX coordinator G13 durable execution continuation — 2026-07-27

The coordinator closed the current local G13 persistence/composition gap. Control
migration 0058 and control-read migration 0011 provide durable run, transition,
idempotency, checkpoint/work-item, reconciliation, rollback-evidence, and
rebuildable status tables. The SQLx store enforces phase/version fences,
request hashes, append-only evidence, MFA, local-only break-glass, and explicit
cutover approval. A fresh disposable control write/read PostgreSQL acceptance
passed for start, transition, checkpoint, reconciliation evidence, projection,
and read-model retrieval.

The registered `g13_migration` worker projects the status read model, the
server exposes an audited read-only migration status RPC, and the Dioxus
console displays those statuses under Platform Settings. No cutover or
command-owner mutation is exposed. The corrected deterministic one-million
user directory volume/restart/replay suite and live SCM two-database cursor,
outbox, projection, and reconciliation suite are also part of the current
local evidence.

External LDAP/RFC 4533, AD DS/DirSync, and UCS Provisioning Service credentials
and environments remain unavailable, as do physical mobile execution,
approved protobuf descriptor comparison, live Folio source/target/rollback
rehearsal, measured five-minute external revocation, and production cutover
approval. These remain queued blockers; Folio remains the operational write
owner and no directory was activated.

## ERPX coordinator final verification confirmation — 2026-07-27

The final tree passed `cargo fmt --all --check`, workspace Clippy with warnings
denied, the non-ignored workspace suite, the complete ignored workspace suite
against fresh disposable control/tenant/commercial/scheduler/support databases,
protobuf compilation, G13/G14 focused acceptance, all Dioxus web/desktop/
host-mobile/iOS/Android target checks, `bun run lint`, `bunx tsc --noEmit`, and
`git diff --check`. The ignored suite initially exposed a commercial snapshot
wire-state mismatch; the authoritative snapshot projector now persists
`BillingState` in its bounded snake_case read form, and the complete rerun
passed. Disposable databases were removed after the run.

Folio remains operational on port 3004 and remains the command/write owner.
Generic LDAP/RFC 4533 and AD DS/DirSync remain transport certification seams
until credentialed environments are supplied; UCS HTTPS transport is bounded
and TLS-only but also lacks live vendor certification. External vendor
acceptance, measured million-user external import, measured five-minute
external revocation, physical mobile execution, approved protobuf descriptor
comparison, live Folio source/target/rollback rehearsal, retention expiry,
post-cutover write rejection, and explicit production cutover approval remain
queued blockers. No directory activation, owner switch, Folio write retirement,
or production cutover was performed.

## Coordinator continuation — live directory transports and G13 work leases — 2026-07-27

- Added coordinator-owned `ldap3 0.12.1` with `default-features = false` and
  `tls-rustls-ring`; the directory worker does not compile a native-TLS or
  plaintext path.
- Directory synchronization now executes bounded LDAPS/StartTLS LDAP paging,
  RFC 4533 refresh-only Sync controls, AD DS critical DirSync BER controls,
  and the UCS Provisioning API `GET .../messages/next` plus post-commit
  `PATCH .../messages/{seq}/status` over HTTPS with subscription Basic auth.
  Pages use the last committed SQLx checkpoint, then commit identity state,
  append-only role facts, outbox, and read projection atomically. Missing
  credentials, malformed BER/JSON, stale native cursors, and connector
  mismatches fail closed; empty UCS queues do not claim work.
- Added durable G13 work enqueue, lease reclaim, bounded claim,
  completion, retry/dead-letter, and work outbox primitives. The registered
  G13 runtime remains a status projector and reports zero migration claims
  until a real Folio source/ERPX target executor and live migration references
  are configured; no unexecuted migration is marked succeeded.
- Focused worker/infrastructure tests, offline checks, and `cargo fmt
  --all --check` pass after this continuation.

This is implementation progress, not vendor or production acceptance. Live
LDAP/RFC 4533, AD DS/DirSync, and UCS certification still require external
environments and credentials; measured one-million-user execution,
five-minute revocation timing, physical-device runs, approved protobuf
descriptor comparison, live Folio source/target/rollback/retention evidence,
and explicit production approval remain blockers. No directory activation,
command-owner switch, Folio write retirement, or cutover was performed.

## Coordinator final verification addendum — 2026-07-27

After the continuation above, the coordinator reran `cargo fmt --all --check`,
workspace Clippy with warnings denied, the non-ignored workspace tests, and
the complete ignored workspace suite using fresh disposable control, tenant,
commercial, scheduler, and support databases; all passed. Protobuf compilation,
G13/G14 acceptance, all Dioxus web/desktop/host-mobile/iOS/Android checks,
`bun run lint`, `bunx tsc --noEmit`, `git diff --check`, and the port-3004
protected-route probe also passed. The four Folio lint warnings remain the
pre-existing n8n-script warnings.

The external acceptance queue is unchanged: credentialed LDAP/AD/UCS vendor
runs, measured million-user and five-minute revocation evidence, physical
mobile execution, approved descriptor comparison, live Folio source/target/
rollback/retention rehearsal, post-cutover write rejection, and explicit
production approval are still unavailable or unauthorized. G0–G14 is not
claimed complete; no directory activation, owner switch, Folio write
retirement, or cutover occurred.

## Coordinator continuation — G12 Android Keystore-backed session persistence — 2026-07-28

The Android client session adapter now crosses a narrow target-owned JNI bridge to the live
Dioxus/NDK context, provisions an AES key in the Android Keystore provider, and stores only a
bounded AES-GCM ciphertext envelope in private SharedPreferences. The adapter uses synchronous
commit semantics, authenticates and revalidates every loaded envelope, removes protected material
on sign-out, and returns fail-closed errors for unavailable context, missing keys, malformed data,
or authentication failure. The console now restores supported-target sessions and persists login,
refresh rotation, and revoke cleanup through this adapter; web and unsupported desktop builds do
not acquire a local fallback.

Android UI and console checks passed, as did the existing iOS, web, desktop, and UI tests. No
Android SDK/NDK, ADB, emulator, or available iOS simulator exists in this environment, so real
device Keystore/Keychain execution and the remaining G12 native journeys remain queued. No
production directory activation, command-owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — durable Folio source bridge — 2026-07-27

The G13 bridge now has an authoritative read-only runtime seam. Control
migrations 0059 and 0012 persist a bounded Folio source snapshot, its source
contract hash, extractor version, entity/missing-table/row counts, and capture
time; the write transaction is idempotent, emits a migration outbox event, and
projects the summary into the read model. The `g13_folio_bridge` worker uses
the existing SQLx `FolioSourceExtractor`, requires a TLS-only source URL and a
verified MFA proof, uses stable run/snapshot/idempotency identities, and queues
bounded deterministic backfill work without claiming unexecuted migration
success or mutating Folio business data.

The live disposable G13 persistence acceptance and worker/infrastructure test
surfaces pass. This closes the former extractor-only implementation gap but
does not provide a credentialed Folio source/target migration, shadow replay,
rollback restore, retention expiry, post-cutover write rejection, or approval
to change command ownership. Those gates remain queued; Folio remains the
operational write owner.

## Coordinator continuation — executable G13 target boundary — 2026-07-27

Added tenant-write migration 0050 with RLS-scoped immutable G13 import rows,
rollback capture rows, and target outbox event support. The target importer
enforces the recorded source contract hash/fence and deterministic target ID,
is idempotent on replay, writes checkpoints through the control store, and
reports source-versus-target counts without hiding unmapped source tables.
The registered `g13_target_import` worker consumes backfill/reconcile work,
records reconciliation evidence, and requires an explicit rollback write fence
before it captures, restores, and replays rollback rows. It never advances a
cutover phase or changes command ownership.

The disposable PostgreSQL G13 target acceptance passed with real control,
tenant-target, and rollback databases: source snapshot load, two-row import,
idempotent replay, matched reconciliation, rollback capture/restore, and two
append-only rollback-replay events. This is local target-boundary evidence;
credentialed Folio source/target/rollback, shadow replay, final delta,
retention expiry, post-cutover Folio write rejection, vendor certification,
approved protobuf descriptor comparison, and explicit production approval
remain queued. Folio remains the operational write owner.

## Coordinator final parallel-completion addendum — 2026-07-27

The final parallel packets added concrete authoritative protections across the
remaining local ERPX lanes: Operations now rejects immutable outbox-envelope
drift and requires fresh healthy reconciliation before rollback completes;
Finance close and journal projection reject source/envelope drift; Legal human
writes require session provenance; Marketing delivery/CRM persist complete
versioned outbox provenance; Sales, SCM, POS, HR, and Asset Maintenance reject
forged or conflicting replay envelopes; SCM projection accepts bounded
continuation cursors; and the Dioxus product-completion surface renders typed
persistence, reconciliation, authorization, blocked, and unavailable states.
Each packet includes SQLx/outbox or projection behavior and focused live or
unit evidence. No mocks or schema-only completion evidence was used.

The live acceptance tests for Generic LDAP, AD DS/DirSync, and UCS now execute
the real directory worker path when their explicit environment references are
present; in this workspace they safely report queued blockers because the
credentialed endpoint/database/CA/server references are absent. Folio source,
target, rollback, and migration evidence is now locally executable through the
G13 bridge and target importer, but no customer source run or cutover was
performed.

Final local verification passed: workspace format, warnings-denied Clippy,
non-ignored workspace tests, the complete ignored SQLx suite on fresh
disposable databases (including G13 target/rollback), protobuf compilation,
Dioxus web/desktop/host-mobile/iOS/Android checks, Folio lint/typecheck, the
port-3004 protected-route probe, and `git diff --check`. Folio remains the
operational write owner. External directory certification, measured external
volume/revocation, physical mobile execution, approved descriptor comparison,
credentialed Folio migration/retention/post-cutover write rejection, and
explicit production approval remain open gates.

## Coordinator continuation — directory capacity and G13 read/shadow boundary — 2026-07-27

The directory feed contract now accounts for the exact one-million-user
capacity before accepting a page or checkpoint, rejects zero/oversized pages,
and schedules disable/delete revocation with an exact five-minute deadline.
The contract is vendor-neutral and covers the existing Generic LDAP, AD
DirSync, RFC 4533, and UCS Provisioning Service feed matrix; no vendor feed
was activated locally.

The G13 target boundary now has a separate tenant-read migration and
rebuildable payload-free read model, durable shadow-replay comparison,
complete immutable identity-mapping gating, explicit Folio write-fence and
final-delta validation, full outbox envelope metadata, rollback capture/
restore evidence, and retention expiry that preserves append-only evidence.
These are executable local target-boundary behaviors, not a claim of a
credentialed Folio migration or production cutover.

Coordinator verification is green: format, warnings-denied workspace Clippy,
non-ignored workspace tests, complete ignored SQLx tests with fresh disposable
databases, protobuf and G13 compatibility suites, Dioxus web/desktop/mobile/
iOS/Android checks, `bun run lint`, `bunx tsc --noEmit`, whitespace checks, and
the Folio port-3004 protected-route probe. The four Folio lint warnings remain
the pre-existing n8n-script warnings.

Credentialed LDAP/AD/UCS certification, measured external one-million-user
import and five-minute revocation, physical mobile execution, an approved
protobuf descriptor comparison, live Folio source/target/rollback/retention
rehearsal, post-cutover write rejection, and explicit production approval
 remain queued. No directory activation, command-owner switch, Folio write
 retirement, or cutover occurred.

## Coordinator continuation — final durable claim and retention fences — 2026-07-27

Support SLA delivery now carries an immutable worker claimant, persisted claim
token, original claim timestamp, and bounded lease through claim, provider
publish, and acknowledgement. Reclaimed or expired leases fail closed and
remain retryable; additive control migration `0060_support_sla_claim_identity.sql`
provides the durable fields. High-risk support reads require a non-nil MFA
evidence identifier verified within five minutes.

G13 retention expiry is scheduled by the registered target worker only after
the control run is `folio_read_only`, the retention horizon has passed, durable
work is settled, reconciliation is `matched`, and rollback evidence is
`succeeded`. The importer checks the immutable source fence and rollback
database before deleting target/read rows, preserves rollback rows, and
appends an idempotent retention outbox event. No Folio command ownership,
directory activation, write retirement, or production cutover was performed.

## Coordinator continuation — Finance proposal generation and support SLA producer — 2026-07-27

The Finance banking boundary now has an authoritative automatic proposal path:
it reloads the imported transaction and account from the tenant write database,
selects only immutable posted-journal rows with exact holding/company/date/
currency/amount facts, validates the serialized journal payload against its
match columns, derives a deterministic proposal identity, and persists the
proposal through the existing idempotent snapshot/outbox boundary. The Finance
bank-provider worker materializes one proposal per imported transaction and
records bounded proposal-count evidence. It never posts or mutates a journal;
ambiguous and unmatched results remain explicit decisions for an authorized
Finance command.

Support ticket insert/update/command paths now materialize deterministic warning
and breach notifications transactionally, and the durable worker claim path
excludes closed tickets while preserving claimant/lease/acknowledgement
identity. These are local G4/G8 implementation slices, not closure of their
broader gates; the coordinator must still complete serialized verification and
the remaining transport/UI/provider evidence.

## Coordinator continuation — bounded product and directory completion wave — 2026-07-27

The current parallel wave also delivered bounded local slices for fail-closed
backup/retention evidence, SQLx and worker Operations detection batches,
durable Marketing `CrmHandoff` journey execution, typed G12 product-row
evidence, and directory import/resume, role mapping, AD/UCS change, revocation,
replay, projection, and outbox acceptance. These slices preserve Folio as the
command/write owner and do not activate directories, switch production
ownership, retire Folio writes, or perform cutover.

Directory vendor certification remains queued because credentialed Generic
LDAP/RFC 4533, AD DS/DirSync, and UCS Provisioning Service endpoints, CA and
server references are absent. The local directory harness additionally reports
the migration-owned `erpx_identity.directory_reconciliation_runs` dependency
as queued. The broader G0–G14 gates remain open pending coordinator integration
and final serialized verification.

## Coordinator verification correction — current-tree final sweep — 2026-07-27

Current-tree coordinator verification completed successfully for `cargo fmt
--all --check`, workspace Clippy with warnings denied, workspace tests,
protobuf tests and compatibility suites, all five Dioxus target checks,
`bun run lint`, `bunx tsc --noEmit`, `git diff --check`, and the Folio
port-3004 protected-route probe. The fresh ignored SQLx sweep passed every
database target except the deliberately fail-closed backup external-
credentials test when aggregated with other database targets; that test
passes independently against fresh control databases and records a failed
run plus retryable blocker evidence. It is therefore not treated as a
product failure or as a complete-ignored-suite pass.

The G14 external preflight was run explicitly and remains blocked by absent
non-secret references for Generic LDAP/RFC 4533, on-premises AD DS/DirSync,
and UCS Provisioning Service endpoints, TLS CA/server references, bind or
subscription credentials, and disposable test database URLs. No vendor
acceptance is claimed. The broader G0–G14 gates remain open pending this
external certification, remaining live/production evidence, and explicit
approval for any activation, command-owner switch, Folio write retirement,
or cutover.

## Coordinator verification correction — current tree after checkpoint fix — 2026-07-27

The current-tree acceptance run found and fixed a real outbox projector defect:
checkpoint advancement now reaches the atomic conflict-update path for
aggregate versions greater than one while still refusing an out-of-order first
event. Fresh Customer 360 revocation and backup-blocker acceptance tests pass
after the fix.

Current evidence is green for formatting, warnings-denied workspace Clippy,
non-ignored workspace tests, protobuf tests, G13/G14 focused suites, all five
Dioxus checks, Folio lint/typecheck, whitespace checks, and the port-3004
protected-route probe. The fresh aggregate ignored SQLx run passes every local
database-backed target except the intentionally blocked G14 external
certification preflight and the backup external-credentials test when mixed
with other fixtures. The backup test passes independently against a fresh
control database; its aggregate result remains an integration-order gap.

The explicit G14 preflight still fails closed because Generic LDAP/RFC 4533,
on-premises AD DS/DirSync, and UCS Provisioning Service database, endpoint,
bind or subscription, CA fingerprint, server-name, and TLS-mode references
are absent. This is not a G0–G14 closure claim. Credentialed vendor
acceptance, measured external volume/revocation, physical mobile execution,
approved descriptor comparison, live Folio migration and post-cutover
write-rejection evidence, and explicit production approval remain queued. No
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator continuation — full-import checkpoint rollback boundary — 2026-07-30

The coordinator audit found and fixed a real native-directory consistency gap:
full-import staging previously advanced `directory_sync_checkpoints` before
canonical import finalization. A late immutable-link, role, or projection
failure could therefore leave incremental sync past data that had been rolled
back. The worker now keeps the ordinary connector checkpoint unchanged while
pages are staged, recovers the staged source cursor after a restart, and
publishes that cursor only after finalization succeeds. Empty terminal imports
remain without a fabricated native cursor. Focused worker and identity tests,
then the serialized workspace gate, passed.

The live LDAP/AD/UCS protocol audit found no additional local defect. AD
DirSync response `MoreResults` is the specified INTEGER form, and UCS message
confirmation uses the documented `/messages/{sequence}/status` endpoint. This
is local protocol evidence only; no vendor endpoint was activated.

## Coordinator final verification after rollback fix — 2026-07-30

PASS: current-tree formatting, warnings-denied workspace Clippy, complete
non-ignored workspace/doc tests, protobuf tests, G13 parallel/recovery (14/14
and 6/6), G14 local compatibility/directory (11/11 and 10/10), all required
Dioxus console web/desktop/iOS/Android checks, shared UI host/iOS/Android,
Android bridge, Folio lint/typecheck, diff hygiene, and port-3004 probes
passed. Folio lint had zero errors and four pre-existing n8n warnings. No
production Next.js build ran.

QUEUED: the exact ignored workspace sweep remains an 81-target fail-closed
queue for missing disposable SQLx, backup/MinIO, credentialed Generic LDAP/
RFC 4533, AD DS/DirSync, UCS Provisioning Service, approved protobuf
descriptor, physical PITR/device/scale, and production-approval inputs. No
directory activation, command-owner change, Folio write retirement, or
production cutover occurred; G0–G14 remain active and are not claimed 100%.

## Coordinator continuation — approved Marketing journey integration — 2026-07-30

The approved-integration action now has a complete local authoritative slice:

- `JourneyIntegrationInvocation` is a bounded profile-redacted handoff whose
  `integration_ref` is the existing verified provider-account reference.
- Tenant-write SQLx owns state, deterministic idempotency, snapshots, outbox
  events, leases, retries, and versioned results.
- Tenant-read SQLx owns a rebuildable event projector, bounded read page, and
  source/read reconciliation; the worker runs projection and reconciliation
  per active holding before claiming work.
- Journey execution verifies exact profile scope and active control-plane
  binding before commit. The multi-holding worker re-resolves that binding
  before HTTPS delivery, resolves the opaque credential through the shared
  secret boundary, rejects insecure routes, redacts the request, and reconciles
  ambiguous outcomes without duplicate submission.
- Existing authenticated provider onboarding RPCs and the read-only Marketing
  Channels console are the configuration API/UI boundary and explicitly state
  that journey invocation transports neither credentials nor profile details.
- Real PostgreSQL acceptance passed the durable lifecycle and cleaned up
  disposable databases; focused worker tests and warnings-denied Clippy passed.

Vendor-certified delivery remains queued until an approved HTTPS endpoint,
credential-manager binding, and external acceptance evidence are supplied. No
customer directory activation, command-owner change, Folio write retirement,
or production cutover is allowed without explicit product-owner approval.
G0–G14 remain active and are not claimed 100% complete.

## Coordinator continuation — Operations claim regression — 2026-07-30

PASS: added and ran a fresh-database SQLx regression for Operations SLA
notification claiming. It passed 1/1 and proves holding-scoped selection,
worker identity persistence, claim-token persistence, and independent claims
for a second holding. `cargo fmt --all --check` and warnings-denied focused
Clippy for the target also passed.

## Coordinator continuation — post-regression serialized verification — 2026-07-30

PASS: after the Operations regression, `cargo fmt --all --check`, workspace
Clippy with warnings denied, and the complete non-ignored workspace suite all
passed. The full Operations ignored SQLx target passed 10/10 on fresh
write/read databases. Folio lint/typecheck passed with four existing n8n
warnings, disposable databases were removed, and port 3004 remained healthy
(`/` 200, `/platform` 307, `/operations` 307). No production Next.js build or
cutover action was performed.

QUEUED: credentialed Generic LDAP, AD DS/DirSync, UCS Provisioning Service,
and live PostgreSQL/MinIO backup certification inputs remain absent.

## Coordinator continuation — post-regression serialized verification — 2026-07-30

PASS: after the Operations regression, `cargo fmt --all --check`, workspace
Clippy with warnings denied, and the complete non-ignored workspace suite all
passed. The full Operations ignored SQLx target passed 10/10 on fresh
write/read databases. Folio lint/typecheck passed with four existing n8n
warnings, disposable databases were removed, and port 3004 remained healthy
(`/` 200, `/platform` 307, `/operations` 307). No production Next.js build or
cutover action was performed.

QUEUED: credentialed Generic LDAP, AD DS/DirSync, UCS Provisioning Service,
and live PostgreSQL/MinIO backup certification inputs remain absent.

## Coordinator continuation — G11 HR employee command boundary — 2026-07-28

The HR console command selector now exposes the existing authoritative
`SaveHrEmployee` route alongside leave, expense, and payroll. Employee payloads
are bounded JSON, bound to the explicit holding/company/optional-branch scope,
carry optimistic versions when supplied, and have client-provided
`last_command_id` and `last_idempotency_key` replaced with the authenticated
command context. Typed HR receipts and authoritative employee-read refresh
remain server-mediated; the server owns HR domain validation, hr.write
authorization/entitlement, SQLx persistence, audit/outbox, worker projection,
idempotency, reconciliation, and rollback.

Evidence: the console package passes 99/99 tests, including employee command
identity/context binding and the existing leave/expense/payroll request
coverage. Broader G11 HR/payroll, privacy, device, reconciliation, live SQLx,
and production approval evidence remains open. No directory activation, owner
switch, Folio write retirement, or production cutover occurred.

## Coordinator verification — G11 HR employee current tree — 2026-07-28

Focused console formatting and tests passed after adding employee writes. The
serialized full verification remains queued for the current tree; the known
ignored workspace sweep remains an exact 66-target missing-resource/
certification blocker set. Those remain queued blockers, not completion
evidence. No temporary databases, activation, owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator verification — current tree after HR and Finance UI packets — 2026-07-28

The final local verification pass is green: serialized format, workspace
warnings-denied Clippy, and non-ignored workspace tests; protobuf checks;
Dioxus web/desktop/Android/iOS target checks; Folio lint and TypeScript; and
port-3004 health. The current aggregate ignored matrix exits 101 with 66
targets reporting absent authoritative runtime/vendor resources. No production
Next.js build, directory activation, owner switch, Folio write retirement, or
cutover was performed.

## Coordinator continuation — reconciliation migration and G1 conflict fence — 2026-07-27

- Added the forward identity migration `0002_directory_reconciliation_runs.sql`
  and wired `apply_identity_migration` to apply it after the baseline. The
  migration persists reconciliation runs and complete directory outbox
  envelope metadata without changing migration numbering ownership elsewhere.
- Directory import reconciliation now writes its authoritative report and
  reconciliation outbox event atomically, with deterministic event identity,
  connector scope, aggregate version, and idempotency metadata. Fresh local
  directory SQLx acceptance and the complete ignored local database sweep
  exercised this path successfully.
- The G1 projector checkpoint fence now rejects a different event ID at the
  current aggregate version, releases the claim, and records the conflict
  through fresh control-plane SQLx acceptance. Workspace Clippy, workspace
  tests, protobuf/package checks, all Dioxus target checks, Folio lint/typecheck,
  and the port-3004 probe remain green.

The G14 external certification preflight still fails closed because the
credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, and UCS
Provisioning Service environments are unavailable. The aggregate backup
blocker remains a fixture-order issue; its isolated fresh-database test passes.
Measured external one-million-user/five-minute-revocation runs, physical
mobile execution, approved descriptor comparison, live Folio migration and
post-cutover write-rejection evidence, and explicit production approval remain
queued. No directory activation, command-owner switch, Folio write retirement,
or production cutover occurred.

## Coordinator final verification addendum — current tree — 2026-07-27

The remaining parallel completion wave is integrated. Support diagnostics now
persist redacted, scoped, five-minute-freshness evidence atomically across the
aggregate, audit, outbox, and idempotency records; support expiry rejects
duplicate or out-of-order terminal batches; Marketing incremental projection
drains and validates every source page; and the server maps the new support
binding denial to a fail-closed policy status. These slices include focused
unit tests and live disposable SQLx acceptance where applicable.

Coordinator verification completed with `cargo fmt --all --check`, workspace
Clippy with `-D warnings`, non-ignored workspace tests, `cargo check -p
erpx-proto --offline`, G13 deployment/recovery and parallel acceptance, G14
directory acceptance, all five Dioxus target checks, `bun run lint`,
`bunx tsc --noEmit`, `git diff --check`, and the Folio `/operations` probe
(HTTP 307 on port 3004). The four lint warnings remain the pre-existing
`infra/scripts/start-n8n.js` warnings.

The ignored workspace suite was executed against fresh disposable local
PostgreSQL databases. Every local database-backed target passed except the
deliberate G14 external-readiness preflight. Stripe semantic and backup
external-credentials acceptance now create their own disposable databases and
pass in the aggregate run. The G14 preflight still fails closed with missing
Generic LDAP/RFC 4533, AD DS/DirSync, and UCS Provisioning Service endpoint,
TLS, credential, and disposable-database references. No vendor certification
is claimed.

G0–G14 closure remains open for credentialed vendor acceptance, measured
one-million-user and five-minute external runs, physical mobile execution,
the approved protobuf descriptor comparison, live Folio source/target/
rollback/retention/post-cutover evidence, and explicit production approval.
No directory activation, production command-owner switch, Folio write
retirement, or cutover occurred.

## Coordinator final verification addendum — descriptor and ignored-suite evidence — 2026-07-27

The protobuf build now emits the authoritative descriptor set used by the
coordinator compatibility test. When an approved descriptor reference is
provided, the test decodes both descriptor sets and rejects removed messages,
fields, enum values, RPCs, or incompatible field/RPC contracts while allowing
additive compatible changes. No approved descriptor reference is present in
this environment, so the comparison is implemented and queued rather than
claimed as passed.

The corrected fresh serial ignored workspace run passed every local
database-backed target, including provisioning, Finance candidate activation,
Operations projection, support persistence/attachments, Stripe semantic
isolation, and backup-blocker isolation. Its only failing target was the
intentional credentialless `erpx-migrator --test g14_external_readiness`
preflight. The initial harness fixture mistake (a prefixed `{database}`
template) was corrected to the authoritative `postgres://localhost/{database}`
contract; no product code was changed for that fixture correction. All unique
disposable databases were removed after the run.

G0–G14 closure remains open for credentialed Generic LDAP/RFC 4533, AD
DS/DirSync, and UCS Provisioning Service certification, measured one-million
user and five-minute revocation evidence, physical mobile execution, an
approved descriptor baseline, live Folio source/target/rollback/retention and
post-cutover write-rejection evidence, and explicit production approval. No
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator final verification addendum — G14 harness and ignored-suite isolation — 2026-07-27

The final G14 acceptance hardening is integrated. Generic LDAP/RFC 4533 now
requires explicit full-import capacity (`FULL_IMPORT=true` and
`MAX_IMPORT_USERS=1000000`) plus operator-supplied real disable/delete
revocation evidence. AD DS/DirSync and UCS Provisioning Service require the
same capacity and revocation evidence while using fixed native feed kinds
(`ad_dirsync` and `ucs_provisioning_service`), with no unused feed variables.
All three live harnesses poll the real feed and fail closed on missing,
malformed, stale, or late provider evidence; no provider mutation or synthetic
certification is performed.

The aggregate ignored-suite fixture contamination is resolved in the tests:
Stripe semantic and backup blocker acceptance each create and clean up their
own disposable PostgreSQL databases. The fresh serial workspace ignored run
passed every local database-backed target, including support persistence and
attachments; the only failing target was the intentional credentialless G14
external-readiness preflight. The corrected G14 directory acceptance and live
harness focused suites passed, and all disposable databases were removed.

G0–G14 closure remains open for credentialed Generic LDAP/RFC 4533, AD
DS/DirSync, and UCS vendor runs; measured million-user and five-minute
revocation evidence; physical mobile execution; the approved protobuf
descriptor comparison; live Folio source/target/rollback/retention/
post-cutover evidence; and explicit production approval. No directory
activation, production command-owner switch, Folio write retirement, or
cutover occurred.

## Coordinator verification — entitlement command current tree — 2026-07-30

PASS: after the entitlement packet, serialized `cargo fmt --all --check`,
workspace warnings-denied Clippy, and the complete non-ignored workspace/doc-
test suite passed. Focused protobuf, G13 parallel/recovery (14/14 and 6/6),
G14 local/directory (11/11 and 11/11), all console web/desktop/host-mobile/
iOS/Android, shared UI host/iOS/Android, Android bridge, Folio lint/typecheck,
`git diff --check`, and port-3004 probes passed. Folio lint had zero errors
and four pre-existing n8n warnings; no production Next.js build ran.

The exact ignored invocation `cargo test --workspace --offline --no-fail-fast
--quiet -- --ignored` exited 101 with exactly 81 explicit missing-resource or
certification guards. The new entitlement ignored SQLx acceptance had already
passed separately 1/1 against fresh disposable PostgreSQL databases; no
application assertion failed in the guarded sweep.

QUEUED: credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning
Service, approved protobuf provenance, physical PITR/device/scale evidence,
live Stripe/catalog reconciliation, and production approval. G0–G14 remain
active and are not claimed 100%; no directory activation, command-owner
switch, Folio write retirement, or production cutover occurred.

## Coordinator continuation — G1 multi-tenant readiness enumeration — 2026-07-28

The unauthenticated server readiness boundary now supports both an explicit
diagnostic tenant and the platform-wide active tenant registry. When no
diagnostic tenant is configured, it reads active registrations through the
bounded SQLx registry API, probes every registered tenant through the existing
live routing/projection/backup readiness adapter, and aggregates each
component fail-closed: any unavailable tenant makes the component unavailable,
any unprobed tenant prevents readiness, and only an all-ready registry can
report ready. Registry enumeration failure and an empty active registry cannot
claim health. The registry bound is 1,000 tenants and no database, credential,
or registry references cross the public endpoint.

Focused server coverage is 87 passed with 1 intentionally ignored database
test, including aggregation, state mapping, and
the existing readiness/security suite; formatting and package compilation
pass. Evidence is recorded in `audits/G1-READINESS-ENUMERATION-2026-07-28/README.md`.
Workspace re-verification and external Generic LDAP/AD DS/UCS certification
remain open. No directory activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator continuation — G11 POS sale command boundary — 2026-07-28

The POS route now exposes the existing authoritative SavePosSale command.
The console derives explicit company or branch command scope, replaces
client-forged command ID/idempotency facts, binds optional expected versions,
and renders typed aggregate/version/replay/correlation receipts. The server
remains authoritative for POS sale invariants, payment and line totals,
pos.write authorization, tenant SQLx persistence, audit/outbox publication,
worker/read projection, idempotency, and rollback.

Evidence: the console package passes 96/96 tests, including POS company/
branch scope binding, immutable command identity replacement, typed receipts,
and malformed/zero-version rejection. Device/payment execution, accounting
handoff, full POS workflow, live SQLx/provider evidence, and production
approval remain open. No directory activation, owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator continuation — G11 Legal command boundaries — 2026-07-28

The Legal route now exposes the existing authoritative SaveLegalContract,
SaveLegalDocument, and RecordLegalAccess commands. Contract metadata receives
the server-matching last-command identity; document metadata remains bounded
and scope-bound; access records receive the authenticated actor identity.
Typed UUID/version/idempotency/event/replay/correlation receipts and
projection refreshes are rendered by the console. Server-side Legal
validation, legal.write authorization, SQLx persistence, audit/outbox,
retention/projection, immutable access history, and rollback remain
authoritative.

Evidence: the console package passes 96/96 tests, including Legal branch
scope, command identity/actor binding, expected-version handling, typed
receipts, and malformed/zero-version rejection. Object-store/provider
certification, broader document/retention workflows, live SQLx evidence, and
production approval remain open. No directory activation, owner switch, Folio
write retirement, or production cutover occurred.

## Coordinator verification — approved integration current-tree sweep — 2026-07-30

Current-tree serialized formatting, warnings-denied workspace Clippy, and the
full non-ignored workspace/doc suite passed. The isolated PostgreSQL
Marketing-integration lifecycle acceptance passed and cleaned its disposable
write/read databases. Protobuf, Dioxus console web/desktop/iOS/Android,
shared UI native checks, Folio lint/typecheck, diff hygiene, and port-3004
protected-route probes passed.

The exact ignored sweep exits 101 with 81 explicit environment or approval
guards; the added Marketing integration target is correctly queued when its
disposable tenant databases are absent. External LDAP/AD/UCS, backup/MinIO/
PITR, approved descriptor, physical device/scale, and production approval
inputs remain unavailable. G0–G14 remain active; no directory activation,
command-owner change, Folio write retirement, or production cutover occurred.

## Coordinator correction — final local verification and dev-runtime handoff — 2026-07-30

VERIFIED: the final exact tree passed `cargo fmt --all --check`, workspace
Clippy with `-D warnings`, and the complete non-ignored workspace suite. The
exact ignored sweep exited 101 with 82 explicit fail-closed guards and no
unexpected assertion failure. Protobuf check/tests, G13 acceptance (15/15),
G14 local compatibility (11/11), G14 directory acceptance (12/12), console
Dioxus web/desktop/mobile, shared UI iOS/Android, and Android bridge target
checks passed. Folio `bun run lint` had zero errors and four pre-existing n8n
warnings; `bunx tsc --noEmit` and `git diff --check` passed.

RUNTIME: the required local Folio service remains on port 3004. Next's
Turbopack dev process repeatedly hit its CSS-worker timeout, so the process
was restarted with the equivalent webpack development server without changing
application code; `/` returns 200 and protected `/platform` and `/operations`
return 307.

QUEUED: the 82 guards remain real SQLx/resource/vendor prerequisites:
credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS
Provisioning Service, approved protobuf descriptor provenance, production
backup/PITR authorization, measured 1M-user/device/provider evidence, and
production approval. G0–G14 are not claimed at 100%; no directory activation,
command-owner change, Folio write retirement, or production cutover occurred.

## Coordinator final addendum — current-tree evidence — 2026-07-30

CURRENT TREE: integrated local ERPX work is verified on
`codex/erpx-parallel-completion`; the exact ignored sweep remains a recorded
exit-101 queue of 82 fail-closed external-resource guards. Local acceptance
and compile evidence does not substitute for the unavailable LDAP/AD/UCS,
vendor-scale, descriptor, backup/PITR, device/provider, or production
approval evidence. G0–G14 therefore remain open for certification only.

## Coordinator continuation — integrated parallel completion evidence — 2026-07-30

IMPLEMENTED: the parallel directory lanes completed Generic LDAP/RFC 4533,
on-premises AD DS/DirSync, UCS provisioning, directory-managed immutable
identity linking, role mappings, append-only revocation, native cursor
fencing, and fail-closed import/reconciliation behavior. The platform,
operations, finance, product-context, migration, and UI/QA lanes were
integrated without changing shared manifest, protobuf, registration,
authorization, migration numbering, or console ownership boundaries.

VERIFIED: fresh disposable PostgreSQL acceptance completed all eight G9
contexts and all ten G11 contexts, including the corrected Quality projection
query and Legal read-model freshness path. G13 parallel acceptance passed
15/15, G14 local compatibility passed 13/13, G14 directory acceptance passed
12/12, the isolated tamper/replay G13 target-import SQLx acceptance passed
1/1, and focused directory/worker/UI suites remained green. The full local
verification set passed formatting, workspace Clippy with warnings denied,
non-ignored workspace tests, protobuf tests/checks, Dioxus web/desktop/mobile
and iOS/Android target checks, Android bridge checks, Folio lint, Folio
TypeScript checking, and diff whitespace validation.

RUNTIME: Folio remains operational on port 3004; `/` returns 200 and the
protected `/platform` and `/operations` routes return 307 while unauthenticated.
No production command-owner switch, directory activation, Folio write
retirement, or final cutover was performed.

QUEUED: the exact ignored workspace sweep remains an exit-101 certification
queue containing 82 fail-closed external-resource guards. Credentialed LDAP,
AD DS, and UCS environments; measured 1M-user vendor-scale evidence; approved
protobuf descriptor provenance; production backup/PITR targets; device and
provider evidence; and explicit production approval are unavailable locally.
These are certification prerequisites, not mocked or schema-only successes,
so G0–G14 and vendor acceptance are not claimed at 100%.

## Coordinator continuation — parallel completion packets and final evidence — 2026-07-30

IMPLEMENTED: the final parallel packets strengthened Generic LDAP/RFC 4533
acceptance boundaries (including the exact 1,000,000-user limit, cursor,
reconciliation, rollback, and directory-managed RBAC evidence), AD/UCS bounded
native-envelope/TLS response handling, POS historical idempotency and outbox
timestamp fencing, legal source-watermark freshness/immutable approval and
transactional rebuild helpers, PITR/backup candidate fencing and cleanup, and
G12/G14 credential-redacting/MFA/break-glass UI and certification-readiness
assertions. No mock transport or production cutover path was introduced.

VERIFIED: after integration, `cargo fmt --all --check`, workspace Clippy with
warnings denied, and the complete non-ignored workspace suite passed. The
serialized ignored sweep (`cargo test --workspace --offline --no-fail-fast --
--ignored --nocapture --test-threads=1`) exited 101 with exactly 82 explicit
fail-closed database/vendor/resource blockers. Protobuf check/tests, G13
acceptance (15/15), G14 local compatibility (13/13), G14 directory acceptance
(12/12), and G14 readiness (10 passed, 1 intentionally ignored) passed.
Console Dioxus web/desktop/Android/iOS, shared UI host/Android/iOS, and the
Android bridge target checks passed. Folio lint had zero errors and four
pre-existing n8n warnings; `bunx tsc --noEmit`, `git diff --check`, and the
port-3004 probes passed.

QUEUED: the 82 blockers remain genuine certification prerequisites: disposable
SQLx control/tenant databases, credentialed Generic LDAP/RFC 4533, on-premises
AD DS/DirSync, UCS Provisioning Service, approved protobuf descriptor
provenance, production backup/PITR targets, measured live 1M-user/device/
provider evidence, native device execution, and explicit production approval.
G0–G14 remain unclaimed at 100%; no customer directory activation, command
owner switch, Folio write retirement, or production cutover occurred.

## Coordinator continuation — G11 HR complete read bundle — 2026-07-28

The authenticated HR/Payroll route now consumes all existing typed HR read
boundaries: employees, leave requests, expense claims, payroll runs with
bounded lines, and expired-role reassignments. The console builds one
scope-bound read bundle from the tenant-read RPCs, validates holding/company/
branch containment, opaque identifiers, lifecycle states, dates, cursors,
source metadata, currency/amount bounds, and payroll line/aggregate
reconciliation, then renders explicit read-only sections while keeping
credentials, bank/tax/account/payment/attachment material out of the client.
The server remains authoritative for hr.read authorization, customer-visible
audit, SQLx projection state, outbox history, freshness, and rebuild.

Evidence is recorded in audits/G11-UI-HR-CONTEXT-READ/README.md. Focused
console coverage is 100/100, workspace Clippy and non-ignored tests pass, and
all required Dioxus console/shared-UI target checks pass. This closes only the
local HR context read adapter; leave/expense/payroll/reassignment writes, live
database acceptance, native device execution, broader HR workflows,
migration/cutover, and G0-G14 closure remain open. No activation, owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator continuation — G10 Marketing profile erasure command boundary — 2026-07-28

The Marketing/CRM console now exposes the existing authoritative
`EraseMarketingProfile` route. It accepts a bounded profile UUID, privacy
reason, and optional RFC3339 timestamp under an explicit authenticated tenant
scope, sets the marketing audit module, and renders a typed profile-erasure
receipt with replay and correlation metadata. The server remains authoritative
for `marketing.profile.erase` entitlement, profile scope containment, source
redaction, atomic SQLx privacy persistence, outbox/read projection rebuild,
idempotency, reconciliation, and rollback.

Evidence: console coverage remains 99/99 and now asserts the marketing command
module, timestamp/reason bounds, immutable profile identity, typed replay
receipt, and malformed UUID rejection. Broader G10 Marketing content,
journey, delivery, provider, device, reconciliation, live SQLx, and
production approval evidence remains open. No directory activation, owner
switch, Folio write retirement, or production cutover occurred.

## Coordinator verification — G10 Marketing privacy current tree — 2026-07-28

Focused formatting and console tests passed after the erasure boundary was
added. Full serialized verification is queued for this current tree; the
known ignored sweep remains 66 explicit missing-resource/certification
targets. Those remain queued blockers, not completion evidence. No temporary
databases, activation, owner switch, Folio write retirement, or production
cutover occurred.

## Coordinator continuation — G7 source outbox feed adapter — 2026-07-28

`SqlxOperationsStore::read_source_outbox_event_page` now exposes a bounded,
holding-routed feed of non-Operations tenant outbox rows for the Operations
detection scheduler. It requires the authoritative shared envelope, maps only
the closed source-domain set, rejects unknown or incomplete legacy rows, and
returns immutable source facts without moving payload ownership into
Operations. Focused infrastructure tests, warnings-denied infrastructure
Clippy, and the worker consumer compile pass. Automatic policy evaluation,
notifications, manager-chain resolution, and live source/vendor evidence
remain queued; G7 and G0–G14 are not claimed complete.

## Coordinator final verification addendum — boundary hardening — 2026-07-27

The final local hardening wave is integrated. Object-storage access now fails
closed unless the configured endpoint is HTTPS and enforces namespace, key,
inventory, truncation, signed-access, and credential-redaction boundaries.
The G2 role-review worker requires the exact authoritative reconciliation
count and releases partially acknowledged work for retry. Finance subledger
replay now requires matching checkpoint event identity and payload, and fails
closed on same-version conflicts, checkpoint/read-row divergence, or orphaned
rows. Focused object-storage, role-review, and Finance SQLx acceptance tests
passed, including fresh disposable database runs for the live worker and
subledger paths.

The final workspace Clippy and non-ignored workspace test runs passed after
this wave. The serial ignored suite has only the deliberate external G14
readiness preflight failure; all local worker cases pass with disposable
database isolation. The external readiness blocker remains queued for
credentialed Generic LDAP/RFC 4533, AD DS/DirSync, and UCS Provisioning
Service environments, including measured million-user import and five-minute
revocation evidence.

G0–G14 closure remains open for those vendor runs, physical mobile execution,
the approved protobuf descriptor comparison, live Folio source/target/
rollback/retention/post-cutover evidence, and explicit production approval.
No directory activation, production command-owner switch, Folio write
retirement, or cutover occurred.

## Coordinator continuation — bounded G2/G7/G8/G12 hardening — 2026-07-27

The local completion wave added four executable hardening slices. G2 recovery
now rehydrates and validates an existing deterministic request-ID assignment on
retry, closing the crash window between the role command/outbox commit and the
separate recovery-review receipt transaction; the server binds the receipt's
start time to the committed assignment's authoritative write-side timestamp.
G7 Operations rejects nil hierarchy IDs and Platform/Subtree authorization
scopes for persisted aggregates and source events. G8 AP/AR allocations now
require exact party and currency identity in addition to existing scope,
payment, invoice, and amount checks. G12 shared navigation sanitizes every
target through the deep-link redaction boundary before it reaches a shell.

Focused evidence: application recovery tests 19/19; domain tests 227/227
plus Finance and POS integration tests; UI tests 79/79 plus all UI integration
tests; server tests 84/84 active; format check passes. These slices advance
local evidence only and do not close the G1/G2/G7/G8/G12 gates: universal
mutation adoption, provider delivery, full Operations/Finance workflows,
native secure storage/device execution, external vendor certification, and
production/cutover evidence remain open.

## Coordinator continuation — G9–G13 bounded hardening — 2026-07-27

The same local wave also hardened G9 SCM recosting (missing authoritative
average cost now fails closed without mutation), G10 consent evaluation
(invalid granted records cannot authorize delivery), G11 POS completion/refund
payment completeness and HR/payroll atomic rejection/overflow behavior, and
G13 migration evidence (locked run-version conflict fencing). Focused domain,
infrastructure, migration, UI, and server evidence is recorded in
`audits/COORDINATOR-CONTINUATION-2026-07-27/README.md`.

These are bounded local advances. They do not close the remaining transport,
worker scheduling, live reconciliation, vendor, mobile, Folio cutover, or
production approval obligations.

## Coordinator final verification addendum — completed local run — 2026-07-27

The required serialized local verification is complete. `cargo fmt --all
--check`, workspace Clippy with warnings denied, and the full non-ignored
workspace tests and doc tests all pass. The Dioxus web, desktop, host-mobile,
iOS, and Android checks pass. Protobuf/G13/G14 focused acceptance passes; the
descriptor comparator reports a queued result because no approved baseline
descriptor is configured.

The fresh serial ignored workspace run passed every local database-backed
target, including the Finance subledger handoff after its acceptance fixture
was corrected to use one shared authoritative party. The only failing target
was the intentional credentialless `g14_external_readiness` preflight, which
lists the missing Generic LDAP/RFC 4533, AD DS/DirSync, and UCS Provisioning
Service evidence references. All exact disposable databases were removed.

Folio `bun run lint` and `bunx tsc --noEmit` pass; lint reports only the four
pre-existing `infra/scripts/start-n8n.js` warnings. The read-only structural
audit passes, `git diff --check` passes, and Folio `/operations` responds with
HTTP 307 on port 3004. G0–G14 closure remains open for credentialed vendor
certification, measured one-million-user/five-minute external evidence,
physical mobile execution, an approved descriptor baseline, live Folio
migration/rollback/retention/post-cutover evidence, and explicit production
approval. No directory activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator continuation — directory and provider boundary hardening — 2026-07-27

The current tree adds executable local hardening for directory authentication,
native synchronization cursors, directory RBAC/revocation, and Marketing
provider delivery. Authentication ignores caller timestamps for MFA and
break-glass validity, redacts MFA references, and keeps local break-glass
explicit. LDAP, AD DirSync, RFC 4533, and UCS cursors reject empty or repeated
continuations. Role facts remain append-only and immutable-link scoped.
Marketing claim-result timestamps come from the database clock, and a live
claim is renewed before a bounded provider submission is recorded.

Combined identity tests and warnings-denied identity Clippy pass. Focused
worker/infrastructure Marketing tests and the fresh disposable live provider
flow pass. The external-readiness preflight remains intentionally queued for
credentialed Generic LDAP/AD/UCS environments, measured one-million-user and
five-minute evidence, physical mobile execution, an approved protobuf
descriptor baseline, live Folio migration/rollback/retention/post-cutover
proof, and explicit production approval. No activation, command-owner switch,
Folio write retirement, or cutover occurred.

## Coordinator continuation — G9 SCM procurement lifecycle transition — 2026-07-28

SCM now exposes a typed `TransitionScmAggregate` command for the safe
Procurement lifecycle actions on Purchase Requests, Purchase Orders, and
Goods Receipt cancellation. It requires explicit tenant/holding routing,
`scm.write`, a positive optimistic source version, and a bounded allow-list;
the SQLx writer loads authoritative source state, invokes the domain method,
and reuses the atomic source/snapshot/shared-outbox/idempotency writer. The
console exposes the action only for Procurement/Inventory contexts and
refreshes the typed read model after a successful receipt. Receipt posting and
stock mutations remain on their relationship-aware domain methods and are not
flattened into a status-only command.

While exercising this packet, the domain invariant was corrected so Purchase
Request/Order/Goods Receipt transitions rotate `command_id` and deterministic
idempotency identity before persistence. The fresh ignored SQLx acceptance
passed submit/approve, projection, reconciliation, clear-and-replay rebuild,
and source/read separation on disposable PostgreSQL databases. Full
Procure-to-Pay breadth, stock/accounting/Operations reconciliation,
worker/vendor/device evidence, and production approval remain open. No
activation, owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G8 authenticated Finance journal read — 2026-07-27

The Dioxus `/finance` product route now consumes the existing Finance
`ListPostedJournals` and `GetFinanceCloseReview` RPCs through an explicit
tenant, holding route reference, UUID holding scope, and (for close review)
company/period UUIDs. The client sends authenticated/audited holding query
contexts and relies on server-side `finance.read` authorization, tenant
read routing, bounded pagination, immutable posted-journal persistence, and
the existing Finance event/outbox/projector boundary.

The display adapter retains only bounded journal/company/period identifiers,
source version, and posted timestamp. Raw `payload_json` is discarded before
the read model crosses into the UI. Missing correlation IDs, invalid UUIDs,
unsafe scalars, oversized pages, and cursor inconsistencies fail closed. The
route exposes explicit scope/loading/empty/denied/offline/error states and a
bounded 25-row read-only table; posting, close, banking, AP/AR, and other
Finance commands remain server-owned. Close review additionally validates
read-only metadata, freshness, status, scope, statement/control totals, fact
counts, and bounded account totals before rendering.

Evidence: `cargo check -p erpx-console --offline` passed and the focused
console suite passed 68/68, including Finance scope, cursor, close-review,
redaction, and
safe-error tests. See `audits/G8-UI-FINANCE-JOURNAL-READ/README.md`.

This closes the local G8 journal-read UI seam only. Finance provider/close/
AP/AR/banking workflows, broader product journeys, external certification,
measured directory evidence, live Folio migration/cutover evidence, and
production approval remain queued. No activation, owner switch, Folio write
retirement, or cutover occurred.

## Coordinator continuation — G11 authenticated HR employee read — 2026-07-28

Completed the local Dioxus HR & Payroll employee lifecycle adapter over the
existing authenticated HrService::ListHrEmployees SQLx projection. The route
requires explicit tenant, holding, company, and optional branch UUID scope;
server authorization, customer containment, tenant-read routing, pagination,
audit, source/outbox projection, and reconciliation remain authoritative.

The console retains only bounded employee reference/display metadata,
company/branch, lifecycle status, dates, source version, projection timestamp,
and correlation. It rejects scope escapes, malformed UUIDs/cursors, unknown
statuses, invalid dates, inconsistent termination state, unsafe values, and
zero source versions. Payroll values, credentials, identity proof, and
mutation controls remain server-owned.

Focused verification passed erpx-console web check and 78/78 console tests.
Evidence: audits/G11-UI-HR-EMPLOYEE-READ/README.md. Full HR/payroll
workflows, leave/expense/reassignment, device journeys, independent database
acceptance, migration/cutover, and production approval remain queued. No
activation, owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G11 authenticated Legal contract read — 2026-07-28

Completed the local Dioxus Legal contract metadata adapter over the existing
authenticated LegalService::ListLegalContracts SQLx projection. The route
requires explicit tenant, holding, company, and optional branch UUID scope;
server authorization, customer containment, tenant-read routing, cursor
pagination, retention projection, audit, source/outbox projection, and
reconciliation remain authoritative.

The console retains only bounded contract reference/title, scoped
company/branch, lifecycle status, retention state/timestamp, party count,
approval presence, source version, projection timestamp, and correlation. It
rejects scope escapes, malformed cursors, unknown lifecycle/retention states,
invalid party metadata, unsafe values, and overlarge party sets. Object bytes,
object references, hashes, party identities, and mutations remain outside the
client contract.

Focused verification passed erpx-console web check and 79/79 console tests.
Evidence: audits/G11-UI-LEGAL-CONTRACT-READ/README.md. Full Legal document
versions/access history, retention workers, device journeys, independent
database acceptance, migration/cutover, and production approval remain
queued. No activation, owner switch, Folio write retirement, or cutover
occurred.

## Coordinator continuation — G11 current product-context acceptance — 2026-07-28

Re-ran the current-tree G11 product-context verifier after the Sales,
SCM/Asset, HR, Legal, and POS read-surface waves. All seven typed
API/server-authorization/console handoffs are present, and all seven product
projection registrations are present in the worker composition root:
Sales, SCM, Asset/Maintenance, HR, Legal, POS, and Marketing. The verifier
passes 7/7 and does not promote source markers into live acceptance.

The remaining G11 evidence is correctly queued: no disposable tenant
write/read pair was supplied for a fresh routed write/outbox/projector/
reconciliation/rollback run; Sales, SCM, POS, payroll, document-storage, and
Marketing provider endpoints are unavailable; and Folio bridge ownership
acceptance remains read-only by design. No activation, command-owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator continuation — G11 authenticated POS sales read — 2026-07-28

Completed the local Dioxus POS sales metadata adapter over the existing
authenticated PosService::ListPosSales SQLx projection. The route requires
explicit tenant, holding, company, and optional branch UUID scope; server
authorization, customer containment, tenant-read routing, cursor pagination,
audit, source/outbox projection, and reconciliation remain authoritative.

The console retains only sale reference, currency, bounded monetary totals,
lifecycle status, line/payment counts with truncation markers, creation time,
source version, projection timestamp, and correlation. It rejects scope
escapes, malformed cursors, invalid currency/status values, unsafe values,
overlarge line/payment sets, and zero source versions; customer/register and
sale payload material never enter the display model.

Focused verification passed erpx-console web check and 80/80 console tests.
Evidence: audits/G11-UI-POS-SALES-READ/README.md. Full POS workflow,
device/payment execution, accounting reconciliation, independent database
acceptance, migration/cutover, and production approval remain queued. No
activation, owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G9 authenticated Sales aggregate read — 2026-07-27

The Dioxus `/sales` product route now consumes the existing
`SalesService::GetSalesAggregate` read boundary with explicit tenant, holding,
holding UUID, aggregate type, and aggregate UUID scope. It sends an
authenticated/audited holding query context, validates response identity,
presence/version/correlation, and discards aggregate JSON before display.
Only bounded aggregate metadata is rendered; Sales save, approval,
fulfillment, and payment commands remain server-owned. Focused console tests
pass 68/68. See `audits/G9-UI-SALES-READ/README.md`.

This closes the local G9 Sales read adapter only. Full Lead-to-Cash,
Procure-to-Pay, SCM breadth, workflow UI, reconciliation/rollback, external
certification, and production approval remain queued. No activation, owner
switch, Folio write retirement, or cutover occurred.

## Coordinator follow-up — G3–G6, G9–G11, and evidence quality — 2026-07-27

G3–G6 platform control now derives holding-scoped authorization from a stable
holding key, preventing per-holding connectors from collapsing to platform
scope. G9–G11 payroll validation rejects malformed or duplicate employee
lines, and SCM transfer validation requires exact scoped product/warehouse
references while checking both balances before mutation. Focused tests and
warnings-denied package Clippy pass.

Directory SQLx acceptance is now explicit: PostgreSQL-dependent tests are
ignored unless `ERPX_G14_LOCAL_DIRECTORY_DATABASE_URL` is supplied, while the
non-database check records the exact queued blocker. The serialized ignored
run must supply a disposable URL for genuine persistence evidence. Current
worker scheduler composition and Marketing callback ingress are present;
historical QA-062 absence claims are superseded, while ignored live SQLx and
external certification evidence remain open. Folio TypeScript excludes
generated `erpx/target` output, and the required typecheck passes.

No activation, command-owner switch, Folio write retirement, or cutover
occurred. Credentialed Generic LDAP/AD/UCS certification, measured one-million
user/five-minute evidence, physical mobile execution, approved protobuf
baseline, live Folio migration/rollback/retention/post-cutover proof, and
explicit production approval remain queued.

## Coordinator final verification — current tree — 2026-07-27

The final clean serial run passes `cargo fmt --all --check`, workspace Clippy
with warnings denied, and `cargo test --workspace --offline --no-fail-fast`,
including identity, domain, worker, and doc tests. The fresh ignored suite
executed real disposable PostgreSQL acceptance for directory SQLx import,
RBAC/revocation, all local G3–G13/G14 database paths, and Marketing delivery
callback/provider-flow persistence; every local target passed and every
disposable database was removed. Its sole nonzero target was the intentionally
ignored credentialless G14 external-certification preflight, which records the
exact Generic LDAP, AD DS/DirSync, and UCS blocker references without claiming
vendor acceptance.

G13/protobuf surfaces and G13/G14 focused acceptance pass; the approved
descriptor comparison is queued because no approved baseline is configured.
Dioxus web/desktop/host-mobile/iOS/Android checks pass. Folio lint and
`bunx tsc --noEmit` pass, with four pre-existing n8n warnings and zero lint
errors; the structural audit passes and `/operations` remains HTTP 307 on
port 3004. Historical QA-062 scheduler/callback absence statements are
superseded by current source, while live external/vendor, mobile, descriptor,
Folio cutover, and production approval evidence remains queued. No activation,
command-owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G9–G11 authenticated SCM and Asset/Maintenance reads — 2026-07-27

The Dioxus `/procurement`, `/inventory`, and `/maintenance` routes now consume
the existing `ScmService::GetScmAggregate` and
`AssetMaintenanceService::GetAssetMaintenanceAggregate` read boundaries. Each
route requires explicit tenant, holding route reference, holding UUID,
aggregate type, and aggregate UUID. Authenticated/audited holding query
contexts, exact response identity checks, bounded correlation/version/presence
validation, and a one-megabyte transport bound are enforced; aggregate JSON is
discarded before display.

The UI renders only allow-listed aggregate metadata with explicit
loading/empty/denied/offline/error states. Procurement, Inventory, and
Maintenance commands remain server-owned and audited. Focused console tests
pass 69/69. Evidence: `audits/G9-G11-UI-PRODUCT-AGGREGATE-READ/README.md`.

This closes only the local SCM/Asset/Maintenance read adapters. Full G9–G11
workflow, vendor/device, browser/device, live reconciliation/rollback,
external directory certification, measured one-million-user/five-minute
evidence, approved protobuf baseline, mobile execution, live Folio
migration/cutover evidence, MinIO round-trip, and production approval remain
open. No activation, command-owner switch, Folio write retirement, or cutover
occurred. 

## Coordinator verification — current G3/G4/G6 tree — 2026-07-28

The current tree passes `cargo fmt --all --check`, workspace Clippy with
warnings denied, the serialized non-ignored workspace/doc test suite, and all
five Dioxus web/desktop/host-mobile/iOS/Android target checks. Current focused
counts are UI 81/81, console 75/75, server 84/84 non-ignored tests, and
infrastructure 219/219 unit tests. The required Folio checks pass: `bun run
lint` has zero errors with four pre-existing n8n warnings, `bunx tsc --noEmit`
passes, the read-only structural probe passes, and the protected Folio
`/operations` route remains HTTP 307 on port 3004.

The fresh current-tree eight-database ignored PostgreSQL matrix passed all
local database targets. Its only nonzero target was the intentionally
credentialless G14 external-readiness preflight, which recorded the exact
missing Generic LDAP/RFC4533, AD DS/DirSync, and UCS Provisioning Service
references. Disposable databases were removed. Approved descriptor baseline,
credentialed vendor certification, MinIO/PITR, physical mobile execution,
live Folio migration/cutover, and production approval remain queued; G0–G14
remain open. No activation, owner switch, Folio write retirement, or cutover
occurred.

## Coordinator continuation — G0 current-state re-audit — 2026-07-27

Added `audits/G0-CURRENT-RE-AUDIT/README.md` to re-audit the current branch
against the G0 workspace, contract, fixture, protobuf, Dioxus, Folio, and
verification requirements. The current tree now has reproducible final
warnings-denied Cargo evidence, the ten-database ignored local acceptance
matrix, focused G13/G14 checks, all Dioxus target checks, Folio lint/typecheck,
and the read-only/port-3004 probes recorded in one current packet.

G0 remains open only for the independent approved-descriptor compatibility
comparison because no approved descriptor baseline is configured. This packet
does not close G0–G14 or authorize directory activation, command-owner change,
Folio write retirement, or production cutover.

## Coordinator continuation — G8 authenticated Finance subledger read — 2026-07-27

Extended the Dioxus `/finance` route over the existing
`FinanceService::ListFinanceSubledgerReadRows` authoritative tenant-read
projection. The client now accepts an explicit tenant/holding scope plus
optional aggregate-type and company filters, sends the authenticated audited
holding query context, and renders bounded AP/AR metadata only.

The adapter validates filter containment, aggregate/company/branch identities,
positive source versions, correlation, page/cursor consistency, and the
one-megabyte payload bound before discarding aggregate JSON. The UI exposes
loading/empty/denied/offline/error states and keeps posting, invoicing,
payments, allocations, and reconciliation commands server-owned. Focused
console tests pass 70/70. Evidence is recorded in
`audits/G8-UI-FINANCE-JOURNAL-READ/README.md`.

This closes only the local Finance subledger read seam. Full G8 posting,
AP/AR command journeys, banking/provider, tax/consolidation, live
two-database reconciliation, rollback, and production acceptance remain open.

## Coordinator continuation — G8 authenticated Finance banking read — 2026-07-27

Extended `/finance` over the existing
`FinanceService::ListFinanceBankingReadRows` tenant-read projection. The
console now accepts explicit tenant/holding scope plus optional aggregate-type
and company filters, sends the authenticated audited holding query context,
and renders only bounded bank-account/import/reconciliation metadata.

The adapter validates holding identity, filter containment, aggregate/company
UUIDs, positive source versions, correlation, cursor consistency, and the
one-megabyte payload bound before discarding aggregate JSON. Bank imports,
match proposals/decisions, reconciliation, and other Finance mutations remain
server-owned and audited. Focused console tests pass 71/71.

This closes only the local Finance banking read seam. Provider execution,
banking commands, broader G8 workflows, live reconciliation/rollback, and
production acceptance remain open.

## Coordinator correction — AD DS DirSync response BER — 2026-07-27

The live AD DS adapter now decodes the response-control `moreData` field as
BOOLEAN, followed by the reserved INTEGER and opaque cookie. Its prior local
INTEGER expectation would fail closed on real AD DirSync responses. Focused
worker tests cover boolean true/false and malformed response rejection. This
closes a local protocol defect but does not close credentialed AD DS vendor
certification or measured one-million-user/five-minute evidence.

## Coordinator continuation — parallel local hardening — 2026-07-27

The following locally unblocked gaps were closed after the AD DS BER repair:

- Local break-glass authentication requires and revalidates loopback peer-origin
  evidence; non-loopback peers fail closed.
- Native directory full imports preserve disabled/deleted states and reject
  immutable-envelope mismatches; live LDAP/RFC4533/AD transports require their
  native continuation control exactly once.
- Operations reconciliation rejects orphan outbox rows without authoritative
  Operations events. Finance journal posting locks and validates the persisted
  accounting period before any journal, usage, outbox, or idempotency write.
- Console directory and G13 evidence surfaces render invalid or unknown states
  as unavailable/read-only. G13 SQLx acceptance proves lease reclaim,
  stale-claim rejection, retry exhaustion, and DLQ read-model accounting.
- G13/G14 runtime reference, endpoint/server-name, evidence-path, and unsafe
  LDAP/AD query validation is fail closed.

These are local implementation and disposable-PostgreSQL evidence closures;
credentialed LDAP/AD/UCS acceptance, approved protobuf baseline, physical
mobile evidence, live Folio migration/cutover/retention proof, and explicit
production approval remain open. No activation or cutover occurred.

The same continuation wave also closed connector-scope reload/locking for
directory RBAC, source-fenced Finance banking rebuilds, exact HR outbox
continuity, holding-bound platform read-model projection, role-review
notification-state validation, and Operations orphan-outbox detection. G6
MinIO round-trip certification remains queued because the local MinIO endpoint
and credentials are unavailable.

## Coordinator final verification record — 2026-07-27

The current tree passed formatting, warnings-denied workspace Clippy, and the
non-ignored workspace tests/doc-tests. A fresh serial ignored SQLx run passed
all local database targets across directory identity, G3–G13/G14, product
contexts, Finance, HR, Legal, SCM, POS, Marketing, Operations, provisioning,
support, and G13. Its only nonzero target was the intentionally ignored G14
external-certification preflight, which failed closed for missing credentialed
Generic LDAP/RFC4533, AD DS/DirSync, and UCS Provisioning Service references.
Eighteen disposable databases made for the run were explicitly removed.

G13 deployment 4/4, G13 parallel 9/9, G14 directory 8/8, G14 readiness 7/7
plus one ignored preflight, protobuf compilation, all five Dioxus checks,
Folio lint/typecheck, the structural probe, and the port-3004 HTTP 307 probe
also passed. The approved descriptor baseline, credentialed vendor acceptance,
measured million-user/five-minute evidence, physical mobile run, live Folio
migration/cutover/rollback/retention evidence, MinIO round-trip, and explicit
production approval remain open. No production activation, command-owner
switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G2 role-review runtime completion — 2026-07-27

The local G2 runtime gap is closed through an authoritative worker path:

- `role_review_snapshot` reads role assignments from the control write database,
  evaluates the domain review queue for an explicit platform/holding/company/
  branch scope, and persists the bounded snapshot and expiry-notification rows
  through `RoleReviewPersistence`.
- `role_review_notifications` is now registered by the worker composition root
  when explicitly selected. Its production publisher appends one idempotent
  `role_expiry_notification.requested` event to the control-plane outbox and
  only then reconciles the claimed notification. Replays cannot duplicate the
  event; provider delivery is not claimed by this outbox append.
- Focused worker role-review tests pass (7/7). The ignored PostgreSQL role-review
  acceptance passes against a fresh disposable database, including snapshot
  persistence, lease retry, outbox idempotency, and recovery receipt replay.

This closes local G2 snapshot/worker/outbox behavior. External notification
provider delivery, remaining universal mutation adoption, credentialed
directory/vendor certification, measured million-user/five-minute evidence,
physical mobile execution, approved protobuf baseline, live Folio migration /
rollback / retention proof, and explicit production approval remain queued. No
activation, command-owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — directory shared authorization boundary — 2026-07-28

The directory full-import and native-feed projections now participate in the
server's shared effective-role boundary. Before local role assignments are
consulted, the identity write store checks the current connector projection,
active mapping, group membership, connector enablement, immutable projection
state, platform/holding containment, and the five-minute freshness window.
Directory-managed principals fail closed when their mapped permission is
missing, disabled, revoked, or stale; only principals with no directory
projection use local roles. High-risk directory mappings require a recent
write-side TOTP verification. Focused compilation, strict Clippy, and fresh
disposable PostgreSQL active/disabled/stale authorization assertions passed.
This closes the local G2 integration gap but does not close vendor, scale,
physical MFA, migration, or production-approval gates.

## Coordinator verification — directory authorization and current tree — 2026-07-28

The coordinator completed the serialized verification required for this
continuation. `cargo fmt --all --check`, workspace Clippy with warnings denied,
and `cargo test --workspace --offline --no-fail-fast` passed. The focused
current-tree directory SQLx acceptance passed on a fresh disposable database,
covering active mapped access, disabled revocation, stale-feed denial,
high-risk MFA, full-import role facts/projection hydration, native change
persistence, and scope-escape rejection; that database was dropped after the
run.

The aggregate `cargo test --workspace --offline -- --ignored --nocapture`
attempt fails closed at the first environment-gated RBAC test because
`ERPX_TEST_DIRECTORY_RBAC_DATABASE_URL` is not configured. The prior corrected
disposable local matrix remains the evidence for the other database-backed
targets; no missing external reference was replaced with a mock. The current
PostgreSQL inventory also contains pre-existing `erpx_*` databases outside
this slice; they were not removed in order to preserve unrelated user state.

Protobuf package check/tests, G13 parallel acceptance 9/9, G13 deployment /
recovery 4/4, all five Dioxus web/desktop/host-mobile/iOS/Android checks, the
read-only structural audit, authorization route inventory, and `git diff
--check` passed. The descriptor comparison remains queued because no approved
baseline exists. Folio lint/typecheck passed with four pre-existing n8n
warnings, and the protected `/operations` probe returned 307 while the dev
server remained on port 3004.

This is not a G0–G14 closure claim. Credentialed LDAP/RFC 4533, AD DS/DirSync,
and UCS certification; measured million-user/five-minute external evidence;
physical mobile execution; approved descriptor comparison; live Folio
migration/rollback/retention/reconciliation; MinIO/PITR; and explicit
production approval remain queued. No directory activation, command-owner
switch, Folio write retirement, or production cutover occurred.

## Coordinator continuation — G7 authenticated Operations read surface — 2026-07-27

The existing authoritative Operations summary and active-exception RPCs are
now connected to the Dioxus `/operations` console route. The route requires an
explicit tenant reference and UUID holding scope; it never infers a holding
from platform metadata or displays platform-scope data as tenant data.

The console sends authenticated, audited tenant-scoped query contexts to
`GetOperationsSummary` and `ListOperationsExceptions`, maps only safe
categorical RPC failures, enforces a bounded 25-row exception page, and
validates scope identity, the `operations_read_model` projection, correlation
IDs, scalar bounds, and pagination consistency before rendering. The UI now
shows the authoritative summary counters, source-domain counts, active
exceptions, explicit loading/empty/denied/offline/error states, and
authority/reconciliation copy. It also provides the existing
`TransitionOperationsException` command behind an explicit holding-scoped
`CommandContext`, allow-listed action fields, positive expected version, fresh
idempotency key, and safe receipt/error handling. No client cache, local
fallback, credential, outbox payload, worker error, optimistic mutation, or
client-side authorization is exposed.

Evidence: `cargo check -p erpx-console --offline` passed and the focused
console suite passed 67/67, including explicit-scope validation, malformed
projection rejection, page/cursor bounds, command-context scope, and redaction
checks. Existing
Operations SQLx persistence/projector/reconciliation and server authorization
remain the authoritative backend evidence. See
`audits/G7-UI-OPERATIONS-READ/README.md`.

This closes the local G7 authenticated read/exception-command UI seam only;
broader Operations command journeys, source-producer breadth, Folio live reconciliation, credentialed
directory/vendor certification, measured million-user/five-minute evidence,
and production approval remain queued. No activation, command-owner switch,
Folio write retirement, or cutover occurred.

## Coordinator verification — replay-complete current tree — 2026-07-27

After the typed command-argument correction, `cargo fmt --all --check`,
warnings-denied workspace Clippy, and serialized workspace tests pass. The
corrected ten-database ignored matrix passes all local targets, including
identity SQLx, worker context, and the three replay control-plane tests; only
the intentional G14 external preflight remains nonzero because credentialed
Generic LDAP/RFC4533, AD DS/DirSync, and UCS references are unavailable. G13
compatibility passes 9/9, deployment recovery passes 4/4, G14 directory
acceptance passes 8/8, and all Dioxus web/desktop/mobile/iOS/Android checks
pass. Folio lint/typecheck pass and `/operations` remains HTTP 307 on port
3004. All disposable databases were removed. The remaining external,
production, descriptor, mobile execution, MinIO, and Folio migration/cutover
items remain queued; no activation or cutover occurred.

## Coordinator correction — UCS provisioning sequence continuity — 2026-07-27

The UCS native feed now rejects a sequence gap after the initial message. It
still treats an exact duplicate as already committed for the durable
confirmation path, while accepting the first observed sequence and the exact
next sequence only. Focused directory-live tests pass (12/12). Credentialed
UCS acceptance and measured revocation evidence remain queued.

## Coordinator final verification — corrected current tree — 2026-07-27

The required verification is green on the current branch: `cargo fmt
--all --check`, warnings-denied workspace Clippy, and the serialized
`cargo test --workspace --offline --no-fail-fast` suite all pass. The fresh
serial ignored SQLx run passed every local database target across directory
identity/RBAC, G3–G13/G14, Finance, HR, Legal, SCM, POS, Marketing,
Operations, provisioning, support, scheduler, and role review. The local
directory SQLx acceptance was rerun with its explicit disposable URL and
passed 2/2. The only remaining nonzero ignored target is the intentionally
ignored credentialless G14 external-certification preflight; it fails closed
with the exact Generic LDAP/RFC4533, AD DS/DirSync, and UCS references and
makes no vendor claim. All 18 databases created for this verification were
removed.

Protobuf compilation, G13 parallel 9/9, G13 recovery 4/4, G14 directory 8/8,
G14 readiness 7/7 plus one ignored preflight, and Dioxus web/desktop/mobile,
iOS, and Android checks pass. Folio `bun run lint` passes with zero errors
and four pre-existing `infra/scripts/start-n8n.js` warnings; `bunx tsc
--noEmit`, the read-only structural audit, `git diff --check`, and the
`/operations` HTTP 307 probe on port 3004 pass. No production Next.js build
was run.

G0–G14 are not represented as 100% closed. Credentialed vendor environments,
measured one-million-user imports and five-minute external revocation,
approved protobuf descriptor baseline, physical mobile execution, live Folio
migration/rollback/retention/post-cutover evidence, MinIO round-trip, and
explicit production approval remain queued. No directory activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Current coordinator audit — 2026-07-30

PASS: final formatting, warnings-denied workspace Clippy, the complete
non-ignored workspace suite, protobuf/docs, G13 recovery/parallel (6/6 and
14/14), G14 local/directory (11/11 and 11/11), all requested Dioxus
console/shared-UI targets, Android bridge, Folio lint/typecheck, and port 3004
probes passed. The exact ignored sweep returned exactly 81 explicit
missing-resource or certification guards and no unexpected assertion failure.

PASS: isolated local SQLx evidence includes identity 6/6, RBAC 2/2, directory
3/3, Finance consolidation 2/2, Marketing journey 6/6, delivery 5/5,
scheduler 1/1, HR 5/5, Sales 4/4, SCM 10/10, POS 9/9, Quality 2/2 plus scope
1/1, Finance controls 10/10, and Operations slices. Local PostgreSQL was
non-TLS-only and is not production or vendor certification evidence.

QUEUED: external LDAP/AD/UCS environments and credentials, remaining SQLx
database URLs, approved protobuf provenance, TLS MinIO/physical PITR,
device/scale evidence, live provider credentials, G13 runtime references, and
production approval. G0–G14 remain active; no activation, command-owner switch,
Folio write retirement, or production cutover occurred.

## Final coordinator verification — 2026-07-30

IMPLEMENTED: directory TLS evidence now enforces bounded freshness and the
configured trust-anchor fingerprint; native-feed handling covers contiguous
UCS sequences and RFC 4533 terminal-cookie semantics; directory RBAC fails
closed for break-glass, cross-connector, stale-revision, and incomplete
revocation states; platform completion commands validate tenant/holding
bindings and lifecycle states. Focused identity, feed, volume, platform,
Operations, Finance, product, UI, and G13/G14 acceptance lanes were rerun.

VERIFIED: `cargo fmt --all --check`, workspace Clippy with `-D warnings`, the
complete non-ignored workspace suite, the exact ignored sweep, protobuf and
G13/G14 compatibility tests, all requested Dioxus web/desktop/Android/iOS
checks, Android bridge tests, `bun run lint`, `bunx tsc --noEmit`,
`git diff --check`, and the port-3004 health probe. The ignored sweep remains
81 explicitly queued resource/certification guards, not completion evidence.

QUEUED: credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS
Provisioning Service, remaining SQLx URLs, approved descriptor provenance,
TLS MinIO/physical PITR, device-backed secure storage, measured 1M-scale
evidence, live provider credentials, G13 runtime references, and production
approval. G0–G14 remain active; no directory activation, command-owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator continuation — parallel packet completion and final matrix — 2026-07-30

PASS: parallel completion packets closed additional authoritative gaps in
directory authentication and synchronization, G3–G6 platform/PITR controls,
G7 Operations, G8 Finance, G9 POS/SCM/Quality, G10 Support/Customer 360 and
Marketing replay, G11 Legal/HR, G12 UI safety/accessibility, and G13
execution-plan dependency validation. No activation, command-owner switch,
Folio write retirement, or production cutover occurred.

PASS: isolated disposable local PostgreSQL acceptance covered identity 6/6,
directory RBAC 2/2, local directory 3/3, Finance consolidation 2/2, Marketing
journey 6/6, delivery 5/5, scheduler 1/1, HR 5/5, Sales 4/4, SCM 10/10,
POS 9/9, Quality 2/2 plus scope integrity 1/1, Finance controls 10/10, and
Operations authoritative/SQLx slices. These local runs used `sslmode=disable`;
they are not production TLS certification. The G10 replay defect was fixed so
an existing owner-notification payload retains its authoritative occurrence
time and stable delivery payload across replay.

PASS: final formatting and workspace warnings-denied Clippy passed; the
complete non-ignored workspace suite passed. The exact ignored sweep returned
exactly 81 fail-closed resource/certification targets with no unexpected
application assertion. Protobuf/docs, G13 recovery/parallel (6/6 and 14/14),
G14 local/directory (11/11 and 11/11), all requested Dioxus console/shared-UI
target checks, Android bridge, Folio lint/typecheck, and port-3004 probes
passed. No production Next.js build ran.

QUEUED: credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS
Provisioning Service, their live TLS/full-import/revocation evidence, control
and tenant database URLs for the remaining ignored SQLx matrix, approved
protobuf descriptor provenance, TLS MinIO/physical PITR, device-backed mobile
storage, measured-scale, live Stripe/provider credentials, G13 runtime
references, and production approval. The local PostgreSQL instance is
non-TLS-only. G0–G14 remain active and are not claimed 100%.

## Coordinator continuation — durable worker failure context — 2026-07-27

Closed the next unblocked G1 boundary with control migration `0061`: worker
retry/dead-letter rows now persist bounded structured failure context carrying
the exact event, aggregate, tenant, actor, correlation, causation, idempotency,
worker, and retry metadata beside the legacy safe display error. The worker
validates this context against the claimed event before persistence, and
secret-bearing actor/scope metadata is omitted.

Focused worker/outbox tests pass, and a fresh disposable control write/read
SQLx acceptance passes 1/1 for durable context and redaction. This is local
diagnostic persistence evidence. Authenticated/audited dead-letter replay,
credentialed directory/vendor acceptance, measured million-user/five-minute
evidence, descriptor approval, live Folio migration/cutover evidence, and
production approval remain queued. No activation, owner switch, Folio write
retirement, or cutover occurred.

## Coordinator continuation — current-tree G1 context verification — 2026-07-27

The current-tree serialized eight-database ignored workspace matrix passed
every local database target, including the new durable worker-context SQLx
acceptance. Its only nonzero target was the intentionally ignored,
credentialless G14 vendor preflight, which recorded the exact Generic LDAP,
AD DS/DirSync, and UCS blocker references without claiming certification.
The two dedicated identity SQLx targets also passed on fresh disposable
databases: directory RBAC scope authority 1/1 and local directory persistence
2/2. All disposable databases were removed.

No new external authority was introduced: credentialed vendor environments,
measured one-million-user imports/five-minute revocation, approved protobuf
descriptor baseline, physical mobile execution, live Folio migration and
cutover evidence, MinIO round-trip, and explicit production approval remain
queued. Authenticated/audited dead-letter replay remains a local G1 follow-up.

## Coordinator continuation — authenticated dead-letter replay — 2026-07-27

Closed the local G1 dead-letter replay boundary. The versioned Platform
Operations contract now exposes a platform-scoped replay command. The server
requires an authenticated session, actor/context equality, the directory-
managed `platform.outbox.replay` permission, platform scope, and a fresh
one-time TOTP step-up. The MFA code is transport-only and is never persisted,
returned, or included in diagnostics.

Replay persistence is one SQLx transaction: it reserves and completes the
command idempotency key, requeues only a dead-lettered event with remaining
attempt budget, clears stale claim/error context, and appends one safe audit
event containing event identity and command metadata but no outbox payload or
worker failure details. Duplicate commands return the stored receipt without a
second replay or audit row. The Dioxus Platform Operations Settings surface
now presents the guarded command and clears the MFA input after submission.

Focused infrastructure SQLx acceptance passes 1/1 on a fresh disposable
control write/read pair, including atomic requeue, duplicate replay, audit
cardinality, and payload/MFA redaction assertions. Server and console
protobuf consumers compile. External directory/vendor certification,
measured one-million-user/five-minute evidence, approved descriptor baseline,
physical mobile execution, live Folio migration/rollback/retention proof,
MinIO round-trip, and explicit production approval remain queued. No
activation, command-owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G4 authenticated platform incident read — 2026-07-27

The Dioxus console now consumes the existing authenticated,
platform-authorized `SupportService.ListIncidents` read boundary for
`/platform/incidents`. The adapter sends actor/session metadata and a
platform-scoped audit context, caps the page at 100 records, validates the
correlation reference, and converts only the shared allow-listed incident
fields into the redaction-safe incident list model. Invalid rows fail closed;
internal timeline, broadcast, mitigation, root-cause, corrective-action,
evidence, capability, credential, and command fields are not rendered.

Loading, empty, denied, offline, error, and read-only states are explicit, and
incident commands remain server-owned. Focused console verification passes
72/72 after `cargo fmt --all`. Evidence is recorded in
`audits/G4-UI-INCIDENT-READ/README.md`.

This closes only the local G4 incident-list read adapter. Full Support and
Incident workflows, approval containment, notifications, tenant support,
reconciliation, device execution, external certification, measured
one-million-user/five-minute evidence, approved descriptor baseline, live
Folio migration/rollback/retention proof, MinIO round-trip, and production
approval remain queued. No activation, command-owner switch, Folio write
retirement, or cutover occurred.

## Coordinator continuation — G4 authenticated platform support-ticket read — 2026-07-27

Extended the same G4 read lane over the existing `SupportService.ListTickets`
RPC for `/platform/support`. The request uses the authenticated platform query
context and support-specific audit purpose, caps the first page at 100 tickets,
validates correlation and cursor consistency, and fails closed on invalid rows
or inconsistent pagination.

The shared UI now renders a typed ticket-list state with only opaque ticket and
customer references, lifecycle status, and bounded severity. Ticket
descriptions, customer replies, internal notes, capability material, and
commands remain outside the display model. Loading, empty, denied, offline,
error, and read-only states are explicit. The combined incident/support packet
is `audits/G4-UI-INCIDENT-READ/README.md`; focused suites pass 80/80 for
`erpx-ui` and 73/73 for `erpx-console`.

This closes only the local G4 incident/ticket metadata read adapters. Full
Support/Incident workflows, approval containment, notifications, tenant
support, reconciliation, device execution, external certification, measured
one-million-user/five-minute evidence, approved descriptor baseline, live
Folio migration/rollback/retention proof, MinIO round-trip, and production
approval remain queued. No activation, command-owner switch, Folio write
retirement, or cutover occurred.

## Coordinator verification — current G4 incident/ticket tree — 2026-07-27

The final current-tree serialized verification passes: formatting, protobuf
package check, all five Dioxus target checks, workspace Clippy with warnings
denied, and `cargo test --workspace --offline --no-fail-fast`. Focused suites
pass 80/80 for `erpx-ui` and 73/73 for `erpx-console`. Folio lint and
TypeScript checks pass with zero errors and four pre-existing n8n warnings;
the structural probe, diff check, branch check, and port-3004 `/operations`
307 probe also pass. The prior ignored PostgreSQL matrix remains valid for
this UI-only wave; all local targets passed and only the intentional
credentialless G14 external-readiness preflight was nonzero. This is not a
G0–G14 closure claim.

## Coordinator continuation — G3 Customer 360 notes transport and bounded read — 2026-07-28

Carried the existing authoritative SQLx Customer 360 notes page through the
versioned platform protobuf and authenticated Dioxus console. The server
continues to validate platform/customer scope, purpose, page bound, and the
deterministic `(created_at DESC, id DESC)` cursor before reading the read
database. Domain redaction remains authoritative; the additive transport
contains only note UUID, author reference, timestamp, body, and continuation
cursor.

The console and shared UI reject pages over 100, incomplete or non-UUID
continuation cursors, control characters, oversized values, and
secret-bearing/invalid note rows. The page renders read-only note metadata and
body, and requests subsequent pages through the server cursor. Cursor state is
bound to the customer UUID, so a customer change returns to the first page
without making local pagination authoritative. Evidence is recorded in
`audits/G3-CUSTOMER360-NOTES-READ/README.md`.

Focused verification passed: `erpx-ui` 81/81 and `erpx-console` 74/74;
protobuf, UI, console web, and server checks passed after formatting. This is
not a G3–G14 closure claim: live multi-holding routing, backfill/reconciliation,
binding-revocation replay, native/mobile deep-link acceptance, external vendor
certification, measured one-million-user/five-minute evidence, approved
descriptor baseline, live Folio cutover, MinIO round-trip, and production
approval remain queued. No activation, owner switch, Folio write retirement,
or cutover occurred.

## Coordinator continuation — G6 scheduler registry lifecycle fence — 2026-07-28

Closed the local scheduler lifecycle race. Schedule, claim, lease renewal,
completion, failure, stale-rebuild terminalization, and stale reclaim now lock
and revalidate the exact active tenant/holding registry row inside the same
SQLx transaction as their scheduler side effects. Tenant lifecycle and
replacement-read activation use the same row lock, so registry deactivation or
route changes cannot interleave after the scheduler preflight check.

Existing permission and audit preflight remains mandatory; the lock is a
transaction fence, not an authorization substitute. Schedule and claim
idempotency remain durable and no public payload/credential material was
expanded. Evidence: `audits/G6-SCHEDULER-REGISTRY-FENCE/README.md`.

Focused infrastructure verification passed: package check and 219 unit tests
with no failures. Full disposable PostgreSQL, MinIO/PITR, external vendor,
mobile, descriptor, Folio migration/cutover, and production-approval evidence
remain queued. No activation, owner switch, Folio write retirement, or
cutover occurred.

## Coordinator continuation — G4 authenticated platform diagnostics read — 2026-07-28

Added a typed, additive `SupportDiagnosticField` response to the existing
server-authorized `GetSupportDiagnostics` boundary. The server remains the
diagnostics snapshot and redaction authority; only bounded operational
summaries, schema version, capture timestamp, and correlation ID cross into the
console adapter. The raw snapshot bytes remain outside the client model.

The console now sends authenticated platform query context and renders the
diagnostics route through an explicit read-only state boundary. It rejects
oversized responses, malformed metadata, unknown/secret-bearing keys, and
invalid values. The route has loading, denied, offline, error, and safe-read
states; no customer capability material, credential, cache authority, or
command is introduced. Evidence: `audits/G4-UI-DIAGNOSTICS-READ/README.md`.

Focused verification passed: server 84/84 non-ignored tests, UI 81/81,
console 75/75, server/console web checks, and formatting. Full support
intersection, notification/device execution, external directory/vendor
certification, measured million-user/five-minute evidence, MinIO/PITR,
approved descriptor, Folio migration/cutover, and production approval remain
queued. No activation, owner switch, Folio write retirement, or cutover
occurred.

## Coordinator continuation — G3 Customer 360 access-audit cursor pagination — 2026-07-28

Completed the local console adapter over the existing authoritative
`GetCustomerAccessAudit` cursor. The adapter keeps the server's SQLx/global-read
audit authority, validates the bounded safe display fields and paired
RFC3339/UUID continuation cursor, and exposes a read-only next-page action.
Cursor state is bound to the customer UUID, so customer changes return to the
first page without local pagination becoming authoritative. Evidence:
`audits/G3-CUSTOMER360-AUDIT-PAGING/README.md`.

Focused console verification passed 76/76 after formatting. Broader G3
multi-holding semantics, notes/audit backfill, binding-revocation
reconciliation, and native/mobile deep-link acceptance remain queued.

## Coordinator continuation — G8 authenticated Finance chart read — 2026-07-28

Completed the local Dioxus Finance chart-of-accounts read adapter over the
existing authenticated FinanceService::ListFinanceCharts SQLx projection.
The route requires explicit tenant, holding, and company UUID scope; server
authorization, tenant-read routing, keyset pagination, source/outbox
projection, reconciliation, and audit remain authoritative.

The console retains only bounded chart/account identifiers, labels, category,
normal balance, status, source version, and projection timestamp. It rejects
scope escapes, malformed UUIDs/cursors, unknown account classifications,
inconsistent parent flags, unsafe values, and overlarge account sets. Chart
payloads and source metadata never enter the display model; chart writes,
posting, close, and provider actions remain server-owned.

Focused verification passed erpx-console web check and 77/77 console tests.
Evidence: audits/G8-UI-FINANCE-CHART-READ/README.md. Full G8
consolidation/tax/provider/live reconciliation, AP/AR workflows, independent
database acceptance, external vendor certification, migration/cutover, and
production approval remain queued. No activation, owner switch, Folio write
retirement, or cutover occurred.

## Coordinator verification — current G8/G11 completion wave — 2026-07-28

The coordinator-owned serialized verification is green for the current tree:
`cargo fmt --all --check`, workspace Clippy with warnings denied, and
`cargo test --workspace --offline --no-fail-fast` all pass. The ignored
database matrix was rerun against disposable, physically distinct local
PostgreSQL databases; every configured local target passed, and its only
nonzero target was the deliberate credentialless G14 external-readiness
preflight. Those exact missing Generic LDAP, AD DS/DirSync, and UCS
references remain queued for credentialed vendor certification; no external
pass is claimed.

The protobuf package check/test, the repository's G13 descriptor comparator
acceptance, all five Dioxus web/desktop/host-mobile/iOS/Android checks,
G13/G14 structural acceptance, `git diff --check`, and branch validation pass.
The descriptor comparator executed and reported the approved-baseline
comparison as queued because no approved descriptor reference is configured;
generation/compilation is not being counted as compatibility proof. Folio
`bun run lint` and `bunx tsc --noEmit` pass with zero errors and four
pre-existing n8n warnings, and the protected `/operations` probe returns 307
while the development server remains on port 3004. No production Next.js
build was run.

This evidence does not close G0–G14. Full vendor certification, measured
one-million-user/five-minute runs, approved protobuf baseline comparison,
physical mobile execution, live Folio migration/rollback/retention and
reconciliation evidence, MinIO/PITR evidence, and explicit production
approval remain open. No directory activation, command-owner switch, Folio
write retirement, or production cutover occurred.

## Coordinator continuation — G13 authenticated execution-work command — 2026-07-28

The coordinator completed the next unblocked G13 control-plane slice. The
additive `MigrationService.QueueMigrationWork` RPC is now a real authenticated
command boundary: it requires an authenticated platform-scoped session,
directory-aware `migration.execute` authorization, transport-only fresh MFA,
bounded migration work kind/scope/fence fields, and an idempotency key. It
queues bounded work only; it does not change command ownership, activate a
directory, retire Folio writes, or perform production cutover.

The SQLx implementation persists the request hash, deterministic work item,
and `migration.work_enqueued` outbox event atomically. Same-key replays return
the original work identity without duplicate queueing; a changed payload is a
durable idempotency conflict. The existing registered worker claim/complete
path consumes the durable item, and the console now exposes a bounded MFA-bound
queue form without secrets or cutover controls.

Evidence: disposable PostgreSQL queue/idempotency/claim/completion acceptance;
disposable source extraction, target import, replay, reconciliation, rollback,
and outbox acceptance; server/infrastructure/proto checks; and console web plus
81-test verification. Full serialized verification is rerun after integration.
Credentialed Generic LDAP/AD DS/UCS certification, approved descriptor
comparison, live Folio migration/rollback/retention, and production approval
remain queued and G13–G14 are not claimed complete.

## Coordinator continuation — current local acceptance packet — 2026-07-28

The current-tree focused acceptance packet passes: G14 directory acceptance
8/8, G13 deployment/recovery acceptance 4/4, G9 product transport 3/3, G11
Sales transport 3/3, G11 Asset/Maintenance acceptance 4/4, G11 product-context
acceptance 7/7, and the G2 authorization route inventory. These tests report
the exact missing disposable database, vendor, descriptor, and migration
references without fabricating live evidence. No activation, owner switch,
Folio write retirement, or cutover occurred.

## Coordinator continuation — directory full-import RBAC and projection — 2026-07-28

Closed a local authoritative gap in the durable directory import path. The
worker now loads connector-scoped directory role mappings before a full import
and passes them through `FullImportRunner::run_with_role_mappings`. Terminal
SQLx finalization validates mapping scope against the persisted connector,
upserts only newer mapping revisions, appends hash-chained grant/revoke facts
for staged users, and appends compensating revoke facts for identities missing
from the snapshot. Existing role-fact tails are respected, so the operation is
append-only and replay-safe.

The same transaction now hydrates immutable `directory_identity_links` and the
rebuildable `directory_identity_projection` for active, disabled, deleted, and
missing users before committing canonical state and the import-completed
outbox event. The ignored PostgreSQL directory acceptance fixture now includes
a grouped active user, a grouped disabled user, an ungrouped user, and asserts
three role facts plus three projected identities. Both ignored directory SQLx
tests passed against a fresh disposable PostgreSQL database on 2026-07-28,
which was dropped after the run. Vendor certification is still queued for
missing Generic LDAP/RFC 4533, AD DS/DirSync, and UCS environments. No
activation, command-owner switch, Folio write retirement, or cutover occurred.

## Coordinator verification — G13 queue command integration — 2026-07-28

- PASS: `cargo fmt --all --check`, workspace Clippy with warnings denied, and
  the complete non-ignored workspace test suite after the G13 structural
  verifier was updated for the authenticated queue RPC.
- PASS: protobuf package check/tests, G13 parallel acceptance 9/9, G13
  deployment/recovery acceptance 4/4, G14 directory acceptance 8/8, and all
  five Dioxus web/desktop/host-mobile/iOS/Android target checks (plus macOS
  desktop target check).
- PASS: Folio `bun run lint` with zero errors and four pre-existing n8n
  warnings, `bunx tsc --noEmit`, branch validation, and HTTP 307 from the
  protected `/operations` route on the development server at port 3004. No
  production Next.js build was run.
- QUEUED/BLOCKED EXTERNAL: the required aggregate ignored-database run was
  executed and failed closed at 64 targets whose explicit disposable database
  or vendor references are absent. The disposable G13 source/target tests
  still passed; missing Generic LDAP/RFC 4533, AD DS/DirSync, UCS, approved
  descriptor, live Folio migration, physical-device, scale/revocation, and
  production-approval evidence remain recorded blockers. No activation,
  ownership switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G13 lifecycle command integration — 2026-07-28

The G13 platform boundary now includes additive begin-run and ordered
phase-advance commands. `BeginMigrationRun` and `AdvanceMigrationPhase` use
the existing SQLx migration aggregate, append-only transition events, and
idempotency records. They require a platform-scoped authenticated session,
directory-aware `migration.execute`, fresh MFA, actor attribution matching,
and bounded run/phase fields. Optimistic version and expected-phase fencing
prevents stale operators from advancing a run incorrectly.

The server rejects expected phases at or beyond `final_delta_imported`, so
command-owner approval remains an explicit external control. The console
exposes only the pre-approval ordered phase list and never accepts credentials,
source rows, approval references, Folio retirement, or cutover commands.
Focused server/console/proto checks and warnings-denied Clippy pass. Full
serialized verification is pending. The aggregate ignored suite, credentialed
LDAP/RFC 4533/AD DS/DirSync/UCS certification, one-million-user and
five-minute revocation evidence, approved descriptor, live Folio migration /
rollback / retention, physical-device checks, and production approval remain
queued; G0–G14 are not claimed 100% complete and no production state changed.

## Coordinator verification — G13 lifecycle command integration — 2026-07-28

- PASS: `cargo fmt --all --check`, workspace Clippy with warnings denied, and
  the complete non-ignored workspace tests. G13 structural acceptance passed
  9/9 after recognizing the lifecycle safety markers.
- PASS: protobuf package tests, G13 deployment/recovery 4/4, G14 directory
  acceptance 8/8, and Dioxus web/desktop/mobile/iOS/Android/macOS target
  checks.
- PASS: Folio `bun run lint` with zero errors and four pre-existing n8n
  warnings, `bunx tsc --noEmit`, branch validation, `git diff --check`, and
  HTTP 307 on `/operations` while the development server listens on port
  3004. No production Next.js build was run.
- QUEUED/BLOCKED EXTERNAL: the required aggregate ignored workspace run
  executed and failed closed at 64 targets because explicit disposable
  PostgreSQL, Generic LDAP/RFC 4533, AD DS/DirSync, UCS, and related runtime
  references are absent. No missing-resource result was treated as vendor or
  production evidence.
- QUEUED: approved descriptor comparison, live two-worker restart/lease
  evidence, credentialed one-million-user/vendor directory acceptance,
  physical mobile secure-storage checks, live Folio migration/shadow/final
  delta/rollback/retention, post-cutover rejection, and explicit production
  approval. G0–G14 remain open and no activation, ownership switch, Folio
  write retirement, or production cutover occurred.

## Coordinator continuation — G12 iOS secure-session adapter — 2026-07-28

The shared UI session boundary now includes a bounded internal envelope that
revalidates every field after load. The iOS target adapter uses the system
Keychain generic-password APIs for save/load/clear; it has no file, browser,
URL, or plaintext fallback. The iOS UI and console targets cross-compile with
the real adapter, and the UI focused suite plus warnings-denied package Clippy
pass.

Android remains explicitly fail-closed because this workspace has no Android
Keystore bridge or SDK/device runtime. Physical iOS/Android execution,
notifications, offline/conflict, accessibility, and mobile approval evidence
remain queued; G12 and G0–G14 are not claimed complete. No directory
activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Coordinator continuation — authoritative Operations SQLx revalidation — 2026-07-28

Fresh disposable tenant write/read PostgreSQL pairs passed the four ignored
Operations persistence tests and two ignored authoritative completion tests.
The persistence evidence covered versioned/idempotent write/read boundaries,
source-referenced aggregate replay, exception lifecycle, and scheduled
detection with source event/outbox/read-model state. The worker evidence
covered authoritative source reconciliation with scope isolation and stable
authority-bound scheduler requests. Temporary databases were removed.

This advances local G7 Operations evidence only; full source-domain breadth,
runtime restart/lease telemetry, Folio reconciliation, credentialed
directory/vendor certification, measured external scale/revocation, and
production approval remain queued. No activation, owner switch, Folio write
retirement, or cutover occurred.

## Coordinator continuation — Finance bank import and tenant SQLx revalidation — 2026-07-28

Fresh disposable PostgreSQL evidence passed the Finance bank-provider worker
job target 1/1 and tenant banking persistence 2/2. The worker path covered
claim fencing, retry/reclaim, reconciled completion, rollback evidence, and
status-only projection. Tenant persistence covered idempotent scoped bank
writes, source/outbox reconciliation, typed matching, and read-projection
rebuild/replay. The ignored tenant fixtures now use unique holding references
per test, preventing shared disposable databases from contaminating source
page counts without changing production behavior. Temporary databases were
dropped. Provider SDK/credential composition, live vendor acceptance, and
broader Finance workflows remain queued.

## Coordinator verification — required ignored suite — 2026-07-28

The required aggregate ignored workspace command was executed and failed closed
at 64 targets because the explicit disposable PostgreSQL, Generic LDAP/RFC
4533, AD DS/DirSync, UCS, backup, and related environment references are not
configured. The emitted failures name the missing references; no mock,
fabricated vendor result, or missing-resource result was promoted to evidence.
Configured finance candidate acceptance passed. All non-ignored Rust gates,
Folio lint/typecheck, branch/diff checks, and the protected port-3004 server
probe remain green. G0–G14 stay open pending external evidence.

## Coordinator continuation — G12 Apple secure session and G7 source feed fence — 2026-07-28

The target-owned secure session adapter now uses the system Keychain on both
iOS and macOS. The bounded internal session envelope is revalidated on load;
web, Android, and Windows/Linux desktop remain explicit unsupported states with
no local or plaintext fallback. Host macOS UI/console compilation, UI tests,
and warnings-denied Clippy pass. Device execution and Android Keystore
certification remain queued.

Operations completion reconciliation now scans the source-context tenant
outbox through the bounded SQLx cursor adapter on every authenticated holding
pass. Each envelope is validated for its shared scope/actor/version/correlation
contract and out-of-scope source facts make reconciliation unhealthy. This is
an authoritative feed-integrity fence only: detection policies, assignee
windows, and manager-chain proofs remain explicit scheduler inputs and are not
invented by the worker. Worker focused check, Clippy, tests, and formatting
pass; G7 source-producer scheduling and live route/provider evidence remain
open.

## Coordinator local SQLx acceptance re-audit — 2026-07-28

The serialized ignored workspace suite was rerun with fresh disposable local
PostgreSQL databases for every configured control-plane, tenant, product,
directory, and G13 migration lane. All configured local SQLx acceptance
targets passed, including the Operations source-feed scope test; cleanup
removed the temporary databases. The only nonzero target was the deliberate
G14 external-certification preflight, which failed closed and listed the
missing Generic LDAP/RFC 4533, on-prem AD DS/DirSync, and UCS Provisioning
Service endpoint, credential, CA/server, import, revocation, and evidence
references. No missing-resource result was promoted to vendor evidence.

The local re-audit does not close G0–G14. Credentialed vendor certification,
measured one-million-user imports and five-minute revocation, approved
protobuf descriptor comparison, physical mobile execution, live Folio
migration/rollback/retention/cutover, and explicit production approval remain
queued. Directory activation, command-owner switching, Folio write retirement,
and production cutover were not performed.

## Coordinator final verification — local acceptance re-audit — 2026-07-28

- PASS: serialized `cargo fmt --all --check`, workspace Clippy with
  warnings denied, and `cargo test --workspace --offline --no-fail-fast`.
- PASS: protobuf package check/tests, G13 compatibility and deployment/
  recovery suites, Dioxus web/desktop/host-mobile/macOS/iOS/Android checks,
  `bun run lint`, `bunx tsc --noEmit`, `git diff --check`, and the read-only
  G14 structural probe.
- PASS: Folio development server remains on port 3004 and the protected
  `/operations` probe returned HTTP 307. No production Next.js build ran.
- BLOCKED/QUEUED: the fresh-database ignored suite passed all configured local
  SQLx targets; only the credentialless G14 external-certification preflight
  failed closed with the exact Generic LDAP/RFC 4533, AD DS/DirSync, and UCS
  references still required. No vendor or production certification was
  claimed.

G0–G14 remain open pending external directory, scale/revocation, approved
descriptor, device, live migration/rollback/retention, and production approval
evidence. No directory activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator continuation — G7 source-outbox detection scheduling — 2026-07-28

The registered Operations completion worker now builds bounded detection
requests from the source-owned tenant outbox when the coordinator supplies
explicit policy rules and current authorization, assignee, and manager-chain
proof. Matching is limited to named source domains and source types; scope
escape, invalid rules, stale authority, duplicate rules, page exhaustion, and
queue overflow fail closed. Exception IDs, correlation IDs, and idempotency
keys are deterministic across scans, while the existing SQLx aggregate/event/
outbox/projector/read-model path remains authoritative.

The source/application identity boundary now keeps source outbox IDs as
immutable provenance and causation and gives the Operations application event
a deterministic derived ID. This prevents a real source outbox envelope from
colliding with its derived Operations outbox event during scheduled detection.

Focused warnings-denied Clippy, worker/infrastructure tests, and a fresh local
PostgreSQL scheduler acceptance passed. The acceptance read one source
envelope, produced one policy-matched request, reproduced its identities on a
second scan, and persisted the assigned exception; temporary databases were
removed. Authenticated route/UI integration, policy publication, manager
authority evaluation, SLA notifications, Folio parity, and vendor evidence
remain queued. No production state changed.

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
notifications, Folio parity, and live vendor evidence remain queued.

## Coordinator post-reassignment verification — 2026-07-28

- PASS: after the reassignment scheduler change, serialized formatting,
  workspace warnings-denied Clippy, and the complete non-ignored workspace
  test suite passed.
- PASS: the fresh-database ignored workspace matrix passed every configured
  local SQLx target, including both Operations scheduler tests. The sole
  nonzero target remained the deliberate G14 external-certification preflight,
  which failed closed with the exact unavailable Generic LDAP/RFC 4533,
  AD DS/DirSync, and UCS references; no external result was claimed.
- QUEUED: the approved protobuf baseline, credentialed vendor endpoints and
  measured scale/revocation evidence, physical mobile execution, live Folio
  migration/rollback/retention/cutover, and explicit production approval.

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

Operations escalation now creates a forced-RLS, tenant-scoped SLA notification
inbox row in the same SQLx transaction as its lifecycle event, aggregate
snapshot, and outbox envelope. The inbox has deterministic idempotency,
bounded expiry, claim leases, exact-worker acknowledgement, retry release,
and an allow-listed metadata payload. The registered notification worker
emits a deterministic `operations.sla_notification_delivered.v1` outbox handoff
before acknowledging the claim; downstream provider delivery remains an
explicit injected boundary and cannot be reported successful when unavailable.

The fresh disposable PostgreSQL Operations acceptance passed transactionally
through detection, reassignment, escalation, notification claim, one outbox
handoff, acknowledgement, and replay with no duplicate delivery. This closes
the local durable notification handoff, not external provider/device
certification or the broader G7/G0–G14 gates.

## Coordinator final delta — deterministic notification expiry and serialized gates — 2026-07-28

- PASS: Operations notification claim expiry now uses the explicit sweep
  timestamp consistently for lease recovery, terminal expiry, selection, and
  acknowledgement fencing. The two affected fresh disposable PostgreSQL
  acceptance cases passed, including durable notification handoff and replay.
- PASS: after that change, `cargo fmt --all --check`, workspace Clippy with
  warnings denied, and `cargo test --workspace --offline --no-fail-fast`
  passed. The million-user bounded directory test also passed.
- QUEUED: the complete fresh-database ignored matrix remains locally green as
  recorded above; the deliberate external-readiness preflight still queues
  live Generic LDAP/RFC 4533, AD DS/DirSync, and UCS Provisioning Service
  endpoints, credentials, certificate/server proofs, scale/revocation
  observations, and evidence references. Approved protobuf baseline,
  physical mobile execution, credentialed Folio migration/rollback/retention,
  and explicit production approval remain required.

## Coordinator continuation — G6 credentialed PostgreSQL/MinIO round trip — 2026-07-28

The unblocked G6 backup slice now has a real credential-gated acceptance in
`crates/infrastructure/tests/backup_live.rs`. Against the host PostgreSQL 18
and MinIO services, it creates disposable tenant write/read databases,
migrates both sides, executes compatible PostgreSQL `pg_dump`/`pg_restore`
processes through the production argv-only adapter, and transfers the
tenant-bound archive through MinIO over an HTTPS endpoint trusted by a
disposable internal CA. The candidate read database is verified for the
expected projection relation and forced RLS, the complete object inventory is
reconciled, the exact artifact is deleted, and every disposable database is
dropped. The ignored acceptance passed.

The adapter no longer supplies `--file=-` while capturing `pg_dump` stdout;
this fixes the real PostgreSQL behavior that otherwise creates a literal `-`
file and returns an empty captured artifact. Explicit dump/restore tool paths
are used where the host PATH has a PostgreSQL-version mismatch. The new
internal-CA constructor accepts trust material only; the operational signed
request remains HTTPS-only and no credential enters the test output.

This closes only the local read-side logical backup/restore/object-cleanup
proof. True PITR/WAL recovery, write-side production backup certification,
native provider retention, external directory/mobile/descriptor evidence,
live Folio migration/rollback/cutover, and explicit production approval
remain queued. No customer directory was activated, command ownership was
switched, Folio writes were retired, or production cutover was performed.

## Coordinator final verification delta — G6 live round trip — 2026-07-28

- PASS: serialized `cargo fmt --all --check`, workspace Clippy with warnings
  denied, and `cargo test --workspace --offline --no-fail-fast` after the
  backup command regression assertion.
- PASS: protobuf package check/tests, G13 compatibility and deployment/
  recovery suites, G14 structural/directory suites, all five Dioxus web,
  desktop, host-mobile, Android, and iOS checks, `bun run lint` (four
  pre-existing n8n warnings, zero errors), and `bunx tsc --noEmit`.
- PASS: ignored G6 SQLx job/retention/worker checks against fresh disposable
  control databases; the credential-gated live PostgreSQL/MinIO round trip
  also passed and left no `tenant-backup-live` objects or disposable
  `erpx_bk_*`/`erpx_g6_*` databases behind.
- PASS: Folio port guard remains healthy with `/operations` HTTP 307 and root
  HTTP 200 on port 3004. No production Next.js build ran.
- QUEUED: full local ignored matrix evidence remains as recorded in the prior
  current-tree verification; its sole expected nonzero is the intentional
  credentialless G14 external-readiness preflight. G0–G14 are not closed:
  credentialed LDAP/AD/UCS certification and measured scale/revocation,
  approved protobuf baseline, physical mobile execution, true PITR/WAL,
  live Folio migration/rollback/retention/post-cutover proof, and explicit
  production approval remain required. No activation, command-owner switch,
  Folio write retirement, or production cutover occurred.

## Coordinator continuation — G13 disposable migration rehearsal — 2026-07-28

The unblocked G13 disposable acceptance passed against fresh local PostgreSQL
databases. The real Folio source extractor captured the fixture; the control
write/read pair persisted and projected the run; the target importer applied
immutable source mappings and outbox events; the separate tenant read model
was rebuilt; shadow replay and source-versus-target reconciliation matched;
rollback restored the captured rows with replayed outbox evidence; and the
Folio-write fence rejected an unapproved final delta. All disposable
databases were removed.

This is authoritative local rehearsal evidence for the bounded fixture, not
production migration evidence. Credentialed Folio source/target execution,
two-worker kill/restart capture, approved protobuf descriptor comparison,
retention/cutover approval, and Folio write retirement remain queued. No
command-owner switch, directory activation, Folio write retirement, or
production cutover occurred.

## Coordinator continuation — G13 worker lease-reclaim rehearsal — 2026-07-28

The focused ignored `g13_migration_persistence` acceptance passed against two
fresh control-plane PostgreSQL databases. It enqueued idempotent migration
work, claimed it with a first worker, expired that lease to model a crashed
worker, and reclaimed it with a restarted worker. The stale claim was fenced;
the restarted claim advanced to attempt two, exhausted its bounded retry
budget into durable dead-letter state, and projected the pending/failed counts
to the read model. Temporary databases were removed.

This provides local lease/restart/idempotency evidence only. Live process
kill/restart telemetry, external Folio migration, approved protobuf baseline,
retention/cutover approval, and Folio write retirement remain queued.

## Coordinator continuation — G9–G11 product shell registry — 2026-07-28

The target-neutral Dioxus UI now exposes an authenticated, read-only product
context registry for Sales, Procurement, Inventory, Assets, Maintenance,
HR/Payroll, Legal, POS, and Marketing/CRM. Each route is an allow-listed
canonical path, emits only redacted navigation targets, and cannot imply
mutation authority. The G11 acceptance verifier requires and passes these
markers; the ERPX UI suite passes 84 unit tests plus its target-specific
integration suites.

This closes the shared shell metadata gap only. Backend authorization, live
two-database workflow/reconciliation, vendor/provider execution, and complete
authenticated role journeys remain authoritative acceptance work and are not
claimed complete by this route registry.

## Coordinator verification — G13/G14 and target-neutral shell delta — 2026-07-28

- PASS: serialized format, workspace warnings-denied Clippy, and the full
  non-ignored workspace test suite after the UI change, including the 1M-user
  directory capacity test.
- PASS: G13 parallel 9/9, G13 deployment recovery 4/4, G14 directory 8/8,
  G14 external-readiness 7/7 plus its intentional ignored certification
  preflight, protobuf package tests/check, all Dioxus web/desktop/host-mobile/
  Android/iOS/macOS checks, Folio lint/typecheck, and the port-3004 probe.
- QUEUED: the certification preflight still lacks credentialed Generic LDAP,
  AD DS/DirSync, and UCS environments; approved protobuf descriptor input,
  physical mobile execution/Android Keystore, true PITR/WAL, live Folio
  migration/cutover/retention, and explicit production approval remain open.

## Coordinator continuation — G6 durable PITR recovery-point admission — 2026-07-28

Added the typed `PitrRecoveryPoint` contract (timestamp-or-LSN, WAL archive
reference, physical restore-target reference), API/protobuf admission,
control migration `0062`, control-read vocabulary migration `0013`, request
hash binding, authoritative job/run persistence, claim binding, and
redaction. A fresh disposable PostgreSQL control acceptance passed with
1,001 full jobs plus one PITR job projected and persisted; temporary databases
were removed.

Physical WAL archive capture and recovery into a verified physical target are
still fail-closed pending the credentialed executor/deployment contract. This
packet does not authorize directory activation, command-owner switching,
Folio write retirement, or production cutover.

## Coordinator continuation — physical PITR capture and final verification delta — 2026-07-28

The bounded `pg_basebackup`/WAL capture adapter is now registered in the
infrastructure API, with private staging, symlink/path/size guards, tenant
chain-scoped object keys, read-back SHA-256 verification, and a durable
recovery manifest. Its physical restore executor remains unconfigured and
fail-closed until WAL replay, target lifecycle, and independent recovery
verification are implemented against a credentialed deployment.

Final local gates pass after the change: serialized format, workspace
warnings-denied Clippy, the complete non-ignored workspace tests, focused
PITR/backup SQLx acceptance, protobuf/G13 registration, all Dioxus checks,
Folio lint/typecheck, and the port-3004 guard. Credentialed LDAP/AD/UCS,
approved descriptor, physical mobile, physical PITR restore, live Folio
migration/retention/cutover, and production approval remain queued. No
activation, owner switch, Folio write retirement, or production cutover
occurred.

## Coordinator continuation — physical PITR replay executor seam — 2026-07-28

The physical PITR path now includes a concrete PostgreSQL replay executor
behind an explicit coordinator-owned target resolver. It read-back verifies
the manifest and all artifacts, restores the base tar and WAL archive beneath
a contained root, writes recovery configuration for the admitted timestamp or
LSN, starts/stops the target with argv-only `pg_ctl`, and independently checks
`pg_is_in_recovery()` with `psql`. The default resolver remains fail-closed and
the executor is not worker-registered pending credentialed deployment
rehearsal; no production target, activation, owner switch, Folio write
retirement, or cutover occurred.

## Coordinator continuation — G8 durable bank-provider job source — 2026-07-28

The Finance bank-provider worker packet now includes durable SQLx scheduling
and read status. Control migration 0063 stores validated opaque import
envelopes, MFA-derived authorization facts, immutable source fences, bounded
claim leases, retry/dead-letter transitions, and append-only reconciliation or
rollback evidence. Control-read migration 0014 projects status only; request,
authorization, credential-key, and provider response payloads do not enter the
read model. Enqueue is idempotent, claim and lifecycle transitions are
worker/lease fenced, and completion requires exact authoritative tenant
transaction and import-outbox reconciliation evidence.

The explicit worker registration method requires injected authorization,
credential, provider-adapter, router, and batch/lease boundaries. The default
registry remains unregistered until those production dependencies are supplied;
no provider SDK, credential, directory activation, command-owner change, Folio
write retirement, or cutover was performed. A real disposable two-database
PostgreSQL acceptance passed, along with focused worker/infrastructure tests
and warnings-denied Clippy. Evidence: `audits/G8-FIN-024/README.md`.

Provider SDK/credential-manager composition, live vendor acceptance, and the
broader Finance AP/AR, posting, close, consolidation, Thai tax, statements,
and migration/reconciliation gates remain open.

## Coordinator continuation — authenticated Finance import job status — 2026-07-28

The Finance service now exposes the durable bank-provider import lifecycle as
an authenticated, explicit-scope query. `GetFinanceBankProviderImportJobStatus`
requires `finance.read`, matches the returned opaque tenant and holding route,
audits the read, and returns only bounded status metadata: job/import identity,
state, source fence, attempts, lease/failure fields, and timestamps. Request,
authorization, credential, and provider response material remain absent from
the wire response. Server mapping tests, strict Clippy, and the server test
suite pass. A visible console status panel, provider SDK/credential manager,
live vendor acceptance, and the wider G8 gate remain open; no activation,
owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G9–G11 disposable SQLx revalidation — 2026-07-28

The coordinator ran disjoint disposable tenant write/read PostgreSQL
acceptances for Sales (2/2), SCM/Procurement–Inventory (5/5), HR/payroll
(4/4), Legal (4/4), POS (5/5), and Marketing CRM/delivery (5/5). These runs
exercised the available authoritative source/outbox, scoped read projection,
idempotency, checkpoint/rebuild, mismatch, rollback, and retention assertions;
each temporary database pair was removed after its target completed.

This closes the current local database-execution gap for those targets only.
G9–G11 remain open for broader end-to-end workflows, vendor/device acceptance,
runtime worker restart/reconciliation, browser/mobile role journeys, and
production approval. No directory activation, command-owner change, Folio
write retirement, or cutover occurred.

## Coordinator verification — current tree after G8/G9–G11 packets — 2026-07-28

Serialized `cargo fmt --all --check`, workspace warnings-denied Clippy, and
the complete non-ignored workspace tests pass. Protobuf package checks, G13
compatibility/registration, G14 directory acceptance, Dioxus web/desktop/
Android/iOS target checks, Folio `bun run lint`, `bunx tsc --noEmit`, and
`git diff --check` also pass. Folio remains operational on port 3004 with `/`
returning 200 and protected `/operations` returning 307; no production
Next.js build was run.

The new local G8 and G9–G11 disposable PostgreSQL acceptances passed and were
cleaned up. Credentialed LDAP/AD DS/UCS, approved descriptor baseline,
measured scale/revocation, physical mobile, MinIO/PITR restore, live Folio
migration/cutover, and production approval remain queued. No activation,
command-owner change, Folio write retirement, or cutover occurred.

## Coordinator continuation — G9 Sales lifecycle transition — 2026-07-28

Sales now exposes a typed `TransitionSalesAggregate` command for the existing
domain lifecycle. The server requires an explicit tenant/holding route,
`sales.write`, a positive optimistic source version, and an allow-listed
transition. The SQLx writer loads the authoritative write aggregate, applies
the typed domain transition, and commits the updated source snapshot, shared
outbox event, and idempotency receipt through the existing atomic writer;
concurrent stale versions fail closed. The console now provides an explicit
transition control and refreshes the authoritative read projection after a
successful receipt; raw aggregate JSON remains discarded at the client.

The real ignored Sales acceptance passed against fresh disposable write/read
PostgreSQL databases with the lifecycle transition, projection, reconciliation,
clear-and-replay rebuild, and scope rejection assertions. This is a bounded
Lead-to-Cash advance, not full G9 closure: broader handoff/payment workflows,
Operations/accounting reconciliation, worker restart evidence, vendor/device
acceptance, and production approval remain open. No activation, owner switch,
Folio write retirement, or cutover occurred.

## Coordinator verification — aggregate ignored matrix — 2026-07-28

The required aggregate ignored command was rerun as
`cargo test --workspace --offline --no-fail-fast --quiet -- --ignored`.
It exited non-zero because 66 targets require unavailable disposable control,
tenant, directory, or external-service URLs/credentials. The failures are
explicit fail-closed `NotPresent` or queued-blocker messages; no target
claimed database evidence without its authoritative environment. The local
Sales and SCM lifecycle targets remain covered by the fresh disposable
PostgreSQL acceptances recorded above. These missing environments are queued
certification/runtime blockers, not production activation or cutover actions.

## Coordinator continuation — G11 HR employee lifecycle transition — 2026-07-28

HR now exposes an authenticated `TransitionHrEmployee` command for the
existing EmployeeRecord domain lifecycle. The server requires explicit
holding/company/branch scope, `hr.write`, a positive expected source version,
bounded termination-date input, audit attribution, and tenant-write routing.
The SQLx adapter reconstructs the authoritative employee row, applies the
typed domain transition, and reuses the atomic source/outbox/idempotency
writer; stale versions and cross-scope commands fail closed. The console
provides bounded transition inputs and refreshes only from the authorized HR
read model after a receipt.

A fresh disposable PostgreSQL acceptance passed the employee transition,
source version/status assertion, two-event outbox assertion, and stale-version
rejection. This is a bounded G11 advance; full HR/payroll workflows, role and
identity journeys, restart/reconciliation evidence, physical device/vendor
acceptance, and production approval remain open. The temporary database was
removed; no directory activation or cutover occurred.

The current-tree aggregate ignored rerun still exits 101 with the same 66
targets because the required external/runtime database and vendor resources
remain absent. The HR transition target is included in that fail-closed
matrix and reports its missing tenant-write URL; the separately supplied
disposable local HR database acceptance above passed.

## Coordinator continuation — G8 bank-provider import status UI — 2026-07-28

The Finance console now exposes the authenticated bank-provider import status
query against the durable control-read RPC. An operator enters an opaque job
UUID after loading an explicit Finance scope; the client verifies exact tenant
and holding identity, bounds lifecycle fields, and renders state, attempts,
source fence, lease, failure class, and timestamps only. No request,
credential, or provider response payload is displayed or cached.

Console boundary tests and the server/infrastructure checks pass. Provider
SDK/credential composition, live vendor acceptance, and the wider Finance
workflow remain open; no activation, owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator continuation — local G14 directory SQLx/RBAC evidence — 2026-07-28

Fresh disposable PostgreSQL evidence passed for the identity-owned directory
slice. The local directory acceptance covered Generic LDAP full import/resume
and immutable linking, AD DS DirSync disable/revocation handling, UCS
Provisioning Service delete handling, Generic LDAP RFC 4533 cursor replay and
role grant/revoke, projection, and transactional outbox persistence. The
scope-containment acceptance rejected a platform mapping for a holding
connector with zero mapping/outbox side effects, then accepted the contained
holding mapping. The separate directory-RBAC authority acceptance passed the
same persisted connector-scope rejection and valid mapping/outbox assertions.
Temporary databases were dropped after each run.

This advances local authoritative G14 evidence only. Credentialed LDAP,
on-premises AD DS, and UCS vendor certification; one-million-user measured
scale; five-minute measured revocation; physical mobile execution; approved
descriptor comparison; and live Folio source/target rollback and cutover
remain queued. No directory activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator verification — platform/holding directory packet — 2026-07-28

The platform-plus-holding directory packet is green on the current tree. A
fresh PostgreSQL SQLx run persisted and round-tripped a platform connector,
persisted a platform-owned holding role mapping, and retained the existing
holding connector scope-containment and outbox assertions. The deterministic
directory volume suite passed 5/5, including one-million-user bounded
checkpoint/restart replay, native LDAP/AD/UCS cursor continuity, exact
five-minute revocation deadlines, append-only role projection, and local-only
break-glass/high-risk MFA.

The serialized verification wave also passed `cargo fmt --all --check`,
workspace Clippy with warnings denied, the complete non-ignored workspace
tests, protobuf package and G13 compatibility checks, Dioxus web/desktop/
mobile/Android/iOS checks, Folio `bun run lint`, `bunx tsc --noEmit`,
`git diff --check`, and port-3004 root/protected-route health (`200`/`307`).
The required aggregate ignored run exits 101 at 66 explicit missing-resource
targets, including disposable database URLs, MinIO/PITR credentials,
credentialed directory/vendor environments, and the approved descriptor
baseline; these remain queued blockers, not test passes. No directory
activation, command-owner switch, Folio write retirement, or production
cutover occurred.

The local SQLx fixture was then rerun with an actual platform connector and a
platform-owned holding role mapping; both the connector scope round-trip and
contained mapping row passed. The deterministic directory volume suite passed
5/5, including one-million-user bounded import/checkpoint restart replay,
native LDAP/AD/UCS cursor continuity, exact five-minute revocation deadlines,
append-only role projection, and local-only break-glass/high-risk MFA. These
are local authoritative/contract evidence only; live multi-holding vendor
scale, physical revocation timing, device execution, and production approval
remain queued.

## Coordinator verification — final current tree after Finance isolation fix — 2026-07-28

After the Finance tenant-fixture isolation fix, the serialized current-tree
verification remained green: `cargo fmt --all --check`, workspace Clippy with
warnings denied, complete non-ignored workspace tests, protobuf tests, G13
compatibility (9/9), G14 structural acceptance (8/8), Dioxus web/desktop/
mobile/Android/iOS checks, Folio `bun run lint`, `bunx tsc --noEmit`,
`git diff --check`, and port-3004 health (`200` root, `307` protected
Operations). The aggregate ignored command still exits 101 at 66 explicit
missing-resource targets; it records unavailable disposable URLs, MinIO/PITR,
credentialed directory/vendor environments, and the approved descriptor
baseline as queued blockers. No directory activation, command-owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator continuation — G10 CRM scope-provider amendment — 2026-07-28

The existing G10 CRM console read boundary now has an explicit tenant-scope
provider. It collects tenant reference, holding route reference, holding UUID,
and optional company/branch UUIDs; canonicalizes and validates the selected
scope; rejects control characters and
credential-marker text; and activates the existing authenticated
`ListCrmReadRows` request only after submission. With no selected scope, the
console makes no CRM business-data request. Customer containment, role and
entitlement authorization, audit context, tenant routing, and read-projection
authority remain server-side. Server-provided bounded cursors are now carried
through the page control and reset on route/scope changes. This closes the
client scope-selection and first-page-only gaps from G10-MKT-027 for holding
reads.

Evidence: `cargo fmt --all --check`; focused
`cargo test -p erpx-console --offline crm_holding_scope -- --nocapture`
(PASS); and desktop console check (PASS). Projection freshness, remaining CRM
adapters, Marketing journeys and
delivery, Finance attribution reconciliation, vendor certification, and
broader G10/G12/G14 acceptance remain open. No activation, owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator verification — CRM scope/cursor current tree — 2026-07-28

The G10 CRM client amendment is green on the current tree. The console now
accepts explicit tenant/holding/company/branch scope, rejects malformed or
sensitive references before transport, requests only server-authorized CRM
rows, and carries bounded server cursors through page continuation. The
focused CRM scope and cursor tests pass, and the complete console package
passes 85/85 tests.

Serialized verification also passed `cargo fmt --all --check`, workspace
warnings-denied Clippy, the complete non-ignored workspace suite, protobuf
tests, G13 compatibility (9/9), G14 structural acceptance (8/8), Dioxus
web/desktop/host-mobile/iOS/Android checks, `bun run lint` (zero errors, four
pre-existing n8n warnings), `bunx tsc --noEmit`, `git diff --check`, branch
validation, and port-3004 health (`200` root, `307` protected Operations).
No temporary databases remain. The aggregate ignored database suite is still
queued at the known missing-resource/certification targets; these results do
not close G0–G14. No directory activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator continuation — G8 Finance command boundary — 2026-07-28

The Finance console now exposes the existing authoritative `SaveFinancePeriod`
and `PostFinanceJournal` command routes. Period fields are bounded and encoded
into the domain JSON contract; journal posting requires an authoritative chart
payload, binds period/company/holding identifiers, generates two balanced
immutable lines, and embeds a domain command context that exactly matches the
protobuf transport context. Both commands carry authenticated metadata and
typed receipt/correlation handling; server-side finance.write authorization,
tenant write routing, SQLx persistence, audit/outbox behavior, projection
refresh, and idempotency remain authoritative. No raw credentials or source
payloads are rendered.

Evidence: the console package passes 87/87 tests, including period bounds and
scope binding plus journal context equality and debit/credit balancing. This
closes the Finance UI/API command seam for G8-FIN-047; provider SDK/credential
retrieval, automatic bank-to-GL, close/AP/AR/SoD/consolidation/tax/statements,
broader Finance reconciliation, external certification, and production
approval remain open. No directory activation, owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator continuation — G8 Finance chart command boundary — 2026-07-28

The Finance console now also exposes the existing authoritative
`SaveFinanceChart` route. It accepts a bounded authoritative chart document,
replaces client-supplied command facts with a freshly generated transport
command envelope, binds expected-version semantics, and displays only typed
receipt/event/correlation metadata. The server continues to validate chart
identity, company containment, account lifecycle rules, finance.write
authorization, audit, SQLx persistence, outbox publication, and idempotency.

Evidence: the console package passes 88/88 tests, including chart command-fact
replacement and explicit holding binding. This closes the chart UI/API seam;
chart lifecycle/usage workflows, provider SDKs, close/AP/AR/SoD/consolidation/
tax/statements, broader reconciliation, external certification, and production
approval remain open. No directory activation, owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator verification — G8 Finance command current tree — 2026-07-28

Current-tree verification passed `cargo fmt --all --check`, warnings-denied
workspace Clippy, the complete non-ignored workspace suite, `erpx-proto`
tests, G13 compatibility/registration (9/9), G14 directory acceptance (8/8),
all Dioxus web/desktop/host-mobile/iOS/Android checks, Folio `bun run lint`
(zero errors, four pre-existing n8n warnings), `bunx tsc --noEmit`, and
port-3004 health (`200` root, `307` protected Operations). The required
aggregate ignored workspace run was rerun and exits 101 at 66 explicit
missing-resource/certification targets; no temporary databases were left.
These queued results do not close G0–G14. No directory activation, command
owner switch, Folio write retirement, or production cutover occurred.

## Coordinator continuation — G8 Finance close-review command boundary — 2026-07-28

The Finance console now exposes the existing authoritative
`SaveFinanceCloseReview` route. It accepts bounded period/review documents,
requires an explicit lifecycle status, replaces the selected embedded
opened/reviewed/closed domain context with a freshly generated matching
command context, binds optimistic expected versions, and renders only typed
receipt/status/correlation metadata. Existing server validation remains the
authority for period locks, reviewer/closer separation, controls, scope,
finance.write authorization, SQLx persistence, audit/outbox publication, and
idempotency; the close projection refreshes after a receipt.

Evidence: the console package passes 89/89 tests, including lifecycle-context
replacement and expected-version binding. Close fact authoring, full close
workflow acceptance, live database evidence, broader Finance reconciliation,
provider certification, and production approval remain open. No directory
activation, owner switch, Folio write retirement, or production cutover
occurred.

## Coordinator verification — chart-inclusive current tree — 2026-07-28

The final chart-inclusive tree passed `cargo fmt --all --check`, warnings-denied
workspace Clippy, the complete non-ignored workspace tests (including 88
console tests), `erpx-proto` tests, G13 compatibility/registration (9/9), G14
directory acceptance (8/8), Dioxus web/desktop/host-mobile/iOS/Android checks,
Folio `bun run lint` (zero errors, four pre-existing n8n warnings),
`bunx tsc --noEmit`, `git diff --check`, and port-3004 health (`200` root,
`307` protected Operations). The exact aggregate ignored rerun exits 101 at
66 explicit missing-resource/certification targets; this remains queued
evidence, not completion. No temporary databases, activation, owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator continuation — G11 HR command boundary — 2026-07-28

The HR employee context now exposes the existing authoritative
`SaveHrLeave`, `SaveHrExpense`, and `SaveHrPayroll` command routes. The
console accepts bounded JSON documents, requires a positive optional expected
version, derives an immutable company or branch tenant scope from the selected
HR scope, and attaches authenticated command metadata. Leave payloads receive
the generated matching `last_command_id`; expense and payroll payloads remain
server-validated against their authoritative scope contracts. Typed aggregate
receipts, replay state, versions, and correlation IDs are rendered after the
RPC, and the authoritative employee read is refreshed on success. Existing
server HR validation, SQLx write-side persistence, outbox/audit publication,
worker projection, RBAC, idempotency, and reconciliation remain authoritative.

Evidence: the console package passes 90/90 tests, including branch-scope
context binding, expected-version handling, immutable leave command facts,
and unsupported-kind rejection. This closes the HR leave/expense/payroll
console command seam; broader HR/payroll/legal acceptance, live SQLx evidence,
provider/device integrations, reconciliation, and production approval remain
open. No directory activation, owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator verification — G11 HR current tree — 2026-07-28

Current-tree verification passed `cargo fmt --all --check`, warnings-denied
workspace Clippy, the complete non-ignored workspace tests, `erpx-proto`
tests, G13 compatibility/registration (9/9), G14 directory acceptance (8/8),
console and shared UI Dioxus web/desktop/host-mobile/iOS/Android checks,
Folio `bun run lint` (zero errors, four pre-existing n8n warnings),
`bunx tsc --noEmit`, and port-3004 health (`200` root, `307` protected
Operations). The exact aggregate ignored workspace run exits 101 at 66
explicit missing-resource/certification targets: disposable database URLs,
MinIO/PITR credentials, credentialed directory/vendor environments, and the
approved protobuf descriptor baseline. These remain queued blockers, not
completion evidence. No temporary databases, directory activation, owner
switch, Folio write retirement, or production cutover occurred.

## Coordinator continuation — G8 Finance subledger command boundary — 2026-07-28

The Finance console now exposes the existing authoritative `SaveSubledger`
route for AP/AR and other supported subledger aggregates. It bounds the
aggregate type and JSON payload, validates JSON before transport, derives an
immutable holding command context from the selected Finance scope, attaches
authenticated metadata, and renders a typed aggregate/version/replay/
correlation receipt. Existing server-side aggregate validation, exact
holding/company/branch containment, finance.write authorization, tenant SQLx
persistence, audit, outbox, worker/read projection, idempotency, and rollback
behavior remain authoritative.

Evidence: the console package passes 91/91 tests, including explicit holding
scope binding, typed receipt validation, invalid-payload rejection, and
command-fact generation. This closes the Finance subledger console command
seam; allocation batches, bank reconciliation/matching, broader AP/AR/close/
tax/consolidation/provider workflows, live SQLx/provider evidence, and
production approval remain open. No directory activation, owner switch, Folio
write retirement, or production cutover occurred.

## Coordinator verification — G8 Finance subledger current tree — 2026-07-28

Current-tree verification passed `cargo fmt --all --check`, warnings-denied
workspace Clippy, the complete non-ignored workspace tests (including 91
console tests), all five console Dioxus target checks, protobuf tests, G13
compatibility/registration (9/9), G14 directory acceptance (8/8), and the
shared UI native iOS/Android checks. The aggregate ignored workspace command
was rerun and exits 101 at exactly 66 explicit missing-resource/certification
targets for disposable PostgreSQL URLs, MinIO/PITR credentials, credentialed
directory/vendor environments, and the approved protobuf descriptor baseline.
Folio lint/typecheck and port-3004 health remain green (`200` root, `307`
protected Operations); no temporary databases, activation, owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator continuation — G9 Sales aggregate command boundary — 2026-07-28

The Sales context now exposes the existing authoritative
`SaveSalesAggregate` route alongside its lifecycle transition. The console
accepts bounded aggregate JSON, validates an optional positive expected
version, binds the command context to the explicit holding scope, and renders
typed aggregate/version/replay/correlation receipts. The server remains the
authority for Sales domain validation, sales.write authorization, tenant
write/read routing, SQLx persistence, audit/outbox publication, worker/read
projection, optimistic concurrency, idempotency, and rollback behavior.

Evidence: the console package passes 92/92 tests, including version/context
equality, explicit holding binding, typed receipt validation, and malformed
payload rejection. Procurement/SCM save, broader G9 order-to-cash/logistics/
accounting/device/reconciliation workflows, live SQLx/provider evidence, and
production approval remain open. No directory activation, owner switch, Folio
write retirement, or production cutover occurred.

## Coordinator verification — G9 Sales current tree — 2026-07-28

The latest serialized workspace wave passed `cargo fmt --all --check`,
warnings-denied workspace Clippy, and all non-ignored workspace tests,
including 92 console tests. The console web/desktop/host-mobile/iOS/Android
and shared UI native iOS/Android checks passed. The aggregate ignored command
remains at exactly 66 queued external-resource/certification targets for
disposable PostgreSQL URLs, MinIO/PITR credentials, credentialed
LDAP/AD DS/UCS/provider environments, and the approved protobuf descriptor
baseline. Protobuf/G13/G14 structural checks, Folio lint/typecheck, and
port-3004 health remain green; no temporary databases, activation, owner
switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G9 SCM aggregate command boundary — 2026-07-28

The Procurement and Inventory contexts now expose the existing authoritative
`SaveScmAggregate` route alongside their lifecycle transitions. The console
accepts bounded aggregate JSON, validates an optional positive expected
version, binds the command context to the explicit holding scope, and renders
typed aggregate/version/replay/correlation receipts. Maintenance remains
read-only. The server remains authoritative for SCM domain validation,
scm.write authorization, tenant write/read routing, SQLx persistence,
audit/outbox publication, worker/read projection, optimistic concurrency,
idempotency, and rollback behavior.

Evidence: the console package passes 93/93 tests, including SCM
version/context equality, explicit holding binding, typed receipt validation,
and malformed payload rejection. Broader G9 procurement-to-pay/logistics/
inventory/accounting/device/reconciliation workflows, live SQLx/provider
evidence, and production approval remain open. No directory activation, owner
switch, Folio write retirement, or production cutover occurred.

## Coordinator verification — G9 SCM current tree — 2026-07-28

The latest serialized workspace wave passed `cargo fmt --all --check`,
warnings-denied workspace Clippy, and all non-ignored workspace tests,
including 93 console tests. Console web/desktop/host-mobile/iOS/Android and
shared UI native iOS/Android checks passed. The ignored aggregate remains a
queued external-resource inventory at 66 targets for disposable PostgreSQL
URLs, MinIO/PITR credentials, credentialed LDAP/AD DS/UCS/provider environments,
and the approved protobuf descriptor baseline. Folio lint/typecheck,
protobuf/G13/G14 structural checks, and port-3004 health remain green; no
temporary databases, activation, owner switch, Folio write retirement, or
cutover occurred.

## Coordinator continuation — G8 Finance banking command boundary — 2026-07-28

The Finance banking surface now exposes the existing authoritative
RegisterBankAccount, ImportBankStatement, SaveBankReconciliation,
SaveBankMatchProposal, and SaveBankMatchDecision routes. Each command
accepts bounded JSON, binds an authenticated domain context to the explicit
holding scope, supports optimistic reconciliation versions, and renders
typed identifiers, versions/counts, idempotency, replay, and correlation
receipts. The server remains authoritative for banking domain validation,
finance.write authorization, tenant SQLx persistence, audit/outbox
publication, workers/read projection, deterministic matching, and rollback.

Evidence: the console package passes 94/94 tests, including context injection
for import/reconciliation/match payloads, holding/version binding, typed
receipts, and malformed/zero-version rejection. Broader provider, bank-to-GL,
AP/AR, close, tax, reconciliation, live certification, and production
approval evidence remains open. No directory activation, owner switch, Folio
write retirement, or production cutover occurred.

## Coordinator verification — Finance banking current tree — 2026-07-28

Focused console coverage is 94/94. The serialized workspace wave passed
format, warnings-denied Clippy, and all non-ignored workspace tests; protobuf
tests, G13 compatibility/registration (9/9), G14 directory acceptance (8/8),
all Dioxus target checks, Folio lint/typecheck, diff checks, and port-3004
health (200 root, 307 protected Operations) also passed. The required ignored
workspace run exits 101 at exactly 66 queued missing-resource/certification
targets. Bun lint has zero errors and four pre-existing n8n warnings. No
temporary databases, activation, owner switch, Folio write retirement, or
cutover occurred.

## Coordinator verification — POS and Legal current tree — 2026-07-28

Focused console coverage is 96/96. Current serialized Cargo verification
passed format, warnings-denied workspace Clippy, all non-ignored workspace
tests, and all Dioxus web/desktop/host-mobile/iOS/Android plus shared UI
native checks. Protobuf tests, G13 compatibility/registration (9/9), G14
directory acceptance (8/8), Folio lint/typecheck, diff checks, and port-3004
health also passed. The required ignored aggregate exits 101 at exactly 66
queued missing-resource/certification targets, including live SQLx, provider,
MinIO/PITR, directory, and protobuf-descriptor references. No temporary
databases, activation, owner switch, Folio write retirement, or cutover
occurred.

## Coordinator continuation — G9 Asset/Maintenance command boundary — 2026-07-28

The Maintenance context now exposes the existing authoritative
`SaveAssetMaintenanceAggregate` route. The console accepts bounded
Asset/Maintenance aggregate JSON, binds an authenticated holding-scoped
command context, derives an immutable idempotency key, supports optimistic
expected versions, renders a typed aggregate/version/replay/correlation
receipt, and refreshes the authoritative Asset/Maintenance read model after a
successful write. The server remains authoritative for typed aggregate
validation, asset_maintenance.write authorization, SQLx write persistence,
audit/outbox publication, worker projection, read models, idempotency, and
rollback behavior.

Evidence: the console package passes 97/97 tests, including bounded payload
handling, holding scope/context binding, idempotency equality, optimistic
version binding, typed receipt validation, malformed-payload rejection, and
zero-version rejection. Broader G9 maintenance lifecycle, reliability,
accounting, device, reconciliation, live SQLx, and production approval
evidence remains open. No directory activation, owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator verification — G9 Asset/Maintenance current tree — 2026-07-28

Current-tree verification passed `cargo fmt --all --check`, `git diff
--check`, warnings-denied workspace Clippy, the complete non-ignored
workspace tests, `cargo test -p erpx-proto --offline`, G13 compatibility/
registration (9/9), G14 directory acceptance (8/8), console web/desktop/
mobile plus iOS/Android target checks, shared UI iOS/Android checks, Folio
`bun run lint` (zero errors, four pre-existing n8n warnings),
`bunx tsc --noEmit`, and port-3004 health (`200` root, `307` protected
Operations). The required `cargo test --workspace --offline
--no-fail-fast --quiet -- --ignored` sweep exits 101 at exactly 66 explicit
missing-resource/certification targets for disposable PostgreSQL URLs,
MinIO/PITR credentials, credentialed directory/vendor environments, and the
approved protobuf descriptor baseline. Those remain queued blockers, not
completion evidence. No temporary databases, activation, owner switch, Folio
write retirement, or production cutover occurred.

## Coordinator verification — G10 Marketing erasure and G11 HR employee current tree — 2026-07-28

The Marketing/CRM console now exposes the authoritative profile-erasure
command boundary: bounded profile UUID, reason, and optional RFC3339 time are
bound to the authenticated holding scope and privacy audit context; client
identity facts are replaced server-side; the typed replay/correlation receipt
is rendered; and the authoritative read path is refreshed after success. The
HR console JSON boundary likewise now supports the existing authoritative
employee lifecycle command with bounded, scope-bound, versioned, and
authenticated context. The server remains authoritative for erasure and
employee validation, SQLx persistence, append-only audit/outbox publication,
worker projection, authorization, reconciliation, and rollback.

Current-tree verification passed `cargo fmt --all --check`, warnings-denied
workspace Clippy, the complete non-ignored workspace tests including 99
console tests, `cargo test -p erpx-proto --offline`, G13 compatibility and
deployment suites (9/9 and 4/4), console Dioxus web/desktop/host-mobile/iOS/
Android checks, shared UI iOS/Android checks, Folio `bun run lint` (zero
errors, four pre-existing n8n warnings), `bunx tsc --noEmit`, and port-3004
health (`200` root, `307` protected Operations; the existing dev server
remains on PID 24923). The required ignored workspace sweep exits 101 at
exactly 66 explicit missing-resource/certification targets. The approved
protobuf descriptor comparison is still queued because no approved baseline
is present; the remaining targets also require disposable PostgreSQL URLs,
MinIO/PITR credentials, local directory SQLx references, and credentialed
Generic LDAP/AD DS/UCS/provider environments. These are queued certification
blockers, not completion evidence. No temporary databases, directory
activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Coordinator continuation — G8 Finance allocation batch command boundary — 2026-07-28

The Finance console now exposes the existing authoritative
`SaveAllocationBatch` route. The surface accepts a bounded payment/invoice/
allocation batch, requires a positive optimistic payment version, binds the
explicit holding scope and authenticated command context, replaces nested
client command IDs, idempotency keys, and contexts, and renders typed payment,
invoice, and allocation receipts with replay and correlation metadata. The
server remains authoritative for balanced allocation domain rules,
finance.write authorization/entitlement, atomic tenant SQLx persistence,
audit/outbox publication, worker projection, idempotency, reconciliation, and
rollback.

Evidence: the console package passes 99/99 tests, including nested identity
replacement, holding scope, optimistic version binding, typed multi-receipts,
bounded JSON, and missing-version rejection. Broader G8 banking-provider,
AP/AR, journal handoff, close, tax, statements, reconciliation, live SQLx,
and production approval evidence remains open. No directory activation, owner
switch, Folio write retirement, or production cutover occurred.

## Coordinator verification — G8 Finance allocation current tree — 2026-07-28

Focused console verification passed after the allocation boundary was added;
the serialized full verification is queued after this packet. The existing
ignored workspace sweep remains an exact 66-target missing-resource/
certification blocker set for disposable PostgreSQL URLs, MinIO/PITR
credentials, credentialed directory/vendor environments, and the approved
protobuf descriptor baseline. Those remain queued blockers, not completion
evidence. No temporary databases, activation, owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator continuation — G10 CRM command boundary — 2026-07-28

The Marketing & CRM console now exposes the existing authoritative
`SaveCrmHandoff`, `SaveCrmOpportunity`, and `RecordCrmAttribution` routes. The
surface accepts explicit holding/company/branch scope, bounded JSON for lead
handoffs and opportunities, positive optimistic versions, allow-listed
attribution models, and authenticated command metadata. Client-supplied
aggregate command IDs are replaced with the immutable command context; typed
receipts, replay state, versions, references, and correlation IDs are shown,
and the authoritative CRM read projection is refreshed after success. The
server remains authoritative for CRM domain validation, crm.write
authorization/entitlement, tenant SQLx persistence, audit/outbox publication,
worker projection, idempotency, attribution, and reconciliation.

Evidence: the console package passes 98/98 tests, including branch scope and
command identity binding, optimistic version handling, attribution model and
UUID bounds, typed receipts, malformed-payload rejection, and zero-version
rejection. Broader G10 Marketing journeys, consent/delivery/provider/device
workflows, live SQLx/provider evidence, Finance attribution reconciliation,
and production approval remain open. No directory activation, owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator verification — G10 CRM current tree — 2026-07-28

Current-tree verification passed `cargo fmt --all --check`, `git diff
--check`, warnings-denied workspace Clippy, the complete non-ignored
workspace tests (including 98 console tests), `cargo test -p erpx-proto
--offline`, G13 compatibility/registration (9/9), G14 directory acceptance
(8/8), console web/desktop/mobile plus iOS/Android target checks, shared UI
iOS/Android checks, Folio `bun run lint` (zero errors, four pre-existing n8n
warnings), `bunx tsc --noEmit`, and port-3004 health (`200` root, `307`
protected Operations). The required ignored workspace sweep exits 101 at
exactly 66 explicit missing-resource/certification targets for disposable
PostgreSQL URLs, MinIO/PITR credentials, credentialed directory/vendor
environments, and the approved protobuf descriptor baseline. Those remain
queued blockers, not completion evidence. No temporary databases, activation,
owner switch, Folio write retirement, or production cutover occurred.

## Coordinator verification — G8/G11 current tree after allocation and employee packets — 2026-07-28

Serialized current-tree verification passed `cargo fmt --all --check`,
warnings-denied workspace Clippy, the complete non-ignored workspace tests
(including 99 console tests), protobuf tests, and all required Dioxus
console/shared-UI web/desktop/mobile/iOS/Android target checks. Folio
`bun run lint` passed with zero errors and four pre-existing n8n warnings;
`bunx tsc --noEmit` passed; port 3004 remains healthy (`200` root, `307`
protected Operations). The required ignored workspace sweep exits 101 at
exactly 66 explicit missing-resource/certification targets, including absent
disposable PostgreSQL URLs, MinIO/PITR credentials, local directory SQLx
references, external Generic LDAP/AD DS/UCS certification references, and the
protobuf descriptor baseline. These are queued certification blockers, not
completion evidence. No temporary databases, activation, owner switch, Folio
write retirement, or production cutover occurred.

## Coordinator verification — G11 HR complete read bundle current tree — 2026-07-28

The authenticated HR/Payroll route now consumes all existing typed HR read
boundaries: employees, leave requests, expense claims, payroll runs with
bounded lines, and expired-role reassignments. The console builds one
scope-bound read bundle from the tenant-read RPCs, validates holding/company/
branch containment, opaque identifiers, lifecycle states, dates, cursors,
source metadata, currency/amount bounds, and payroll line/aggregate
reconciliation, then renders explicit read-only sections while keeping
credentials, bank/tax/account/payment/attachment material out of the client.
The server remains authoritative for hr.read authorization, customer-visible
audit, SQLx projection state, outbox history, freshness, and rebuild.

Evidence is recorded in audits/G11-UI-HR-CONTEXT-READ/README.md. Focused
console coverage is 100/100, workspace Clippy and non-ignored tests pass, and
all required Dioxus console/shared-UI target checks pass. The latest ignored
workspace sweep remains exactly 66 queued missing-resource/certification
targets. This closes only the local HR context read adapter; live database
acceptance, native device execution, broader HR workflows, migration/
cutover, and G0-G14 closure remain open. No activation, owner switch, Folio
write retirement, or production cutover occurred.

## Coordinator verification — local acceptance wave — 2026-07-28

Real isolated PostgreSQL acceptance now covers the G6 HTTPS MinIO logical
backup/restore round trip with PostgreSQL 18 clients, G13 disposable
migration/import/rollback paths, G14 local identity/directory SQLx and RBAC
paths, G8 Finance journal/banking/budget/chart/close/subledger/backfill/import
and candidate paths, and G9–G11 product and worker/control-plane paths. Each
focused test used fresh disposable databases; the temporary databases,
temporary TLS material, MinIO process, and MinIO data were removed afterward.

The external G14 certification preflight still fails closed without
credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, or UCS
Provisioning Service environments. The approved protobuf descriptor baseline,
measured one-million-user/five-minute evidence, true PITR/WAL, physical mobile
execution, and explicit production Folio migration/cutover approval remain
open gates. No customer directory was activated, no command owner changed,
Folio writes were not retired, and no production cutover occurred.

## Coordinator final verification — 2026-07-28

The serialized verification wave passed formatting, workspace warnings-denied
Clippy, the complete non-ignored workspace suite, protobuf tests, G13
compatibility/deployment checks, G14 structural checks, all available Dioxus
web/desktop/host-mobile/iOS/Android checks, shared UI/mobile bridge checks,
Folio `bun run lint`, and `bunx tsc --noEmit`. The mandated ignored workspace
sweep executed with one test thread and exited 101 at exactly 66 explicit
missing-resource/certification targets; those targets are backed by focused
real PostgreSQL/HTTPS-MinIO acceptance where local resources were available.

Folio remains healthy on port 3004 (`root=200`, `/operations=307`). The
external G14 vendor environments, approved descriptor baseline, measured
million-user/five-minute certification, true PITR/WAL, physical mobile
execution, and explicit production migration/cutover approval remain open.
No customer directory was activated, ownership changed, Folio writes were
retired, or production cutover occurred.

## Coordinator verification — G6 owner bootstrap and Platform Operations copy — 2026-07-28

The ignored server owner-bootstrap acceptance passed against fresh disposable
PostgreSQL control write/read databases. It verified the atomic recovery
identity and single recovery-code issuance, the finite holding-owner role and
approver binding, idempotent replay without a second recovery code, and
write-only owner-seed state isolation from the read database. Exact-name
cleanup removed both disposable databases.

Platform Operations route descriptions now match the existing authenticated
server adapters for projections, backups, integrations, audit, and settings.
The console still excludes credentials, payloads, mutable settings, and
operational commands; server authorization, audit, and command ownership are
unchanged. External G14 certification, scale/revocation measurements,
descriptor compatibility, true PITR/WAL, physical mobile execution, and
explicit production migration/cutover approval remain open gates.

The local one-million-user directory-volume suite also passed all five
bounded import, checkpoint/replay, and revocation cases in 51.74 seconds.
This validates the implementation's bounded scale/replay behavior, while
credentialed provider-side transport, measured production throughput, and
the remaining external acceptance gates stay queued.

## Coordinator final serialized verification — owner and scale evidence — 2026-07-28

After the owner-bootstrap and Platform Operations metadata corrections, the
serialized formatter, warnings-denied workspace Clippy, non-ignored workspace
tests, Dioxus web/desktop/mobile/iOS/Android checks, Folio lint, and TypeScript
checks passed. The non-ignored suite includes 100 console tests and all five
one-million-user directory-volume cases.

The exact mandated ignored sweep still exits with 66 queued missing-resource
or certification targets. Those are explicit fail-closed blockers for absent
disposable SQLx URLs, MinIO/PITR credentials, credentialed Generic LDAP,
on-premises AD DS/DirSync, UCS Provisioning Service, and related evidence;
they are not completion evidence. Folio remains healthy on port 3004. No
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator final serialized verification — G4 Support command current tree — 2026-07-28

After the Support/Incident console command adapter and boundary tests,
`cargo fmt --all --check`, warnings-denied workspace Clippy, the complete
non-ignored workspace suite, protobuf tests, G13 compatibility/deployment
checks, G14 directory/readiness checks, and the Dioxus console/shared UI
target matrix passed. The current console suite is 102/102.

The mandated ignored sweep ran with one test thread and reported exactly 66
queued missing-resource/certification targets. Those targets fail closed on
absent disposable SQLx URLs, MinIO/PITR credentials, external Generic LDAP/
AD DS/UCS references, or explicit protobuf/cutover evidence; they are not
application assertion failures. Folio lint completed with zero errors and
four pre-existing n8n warnings, TypeScript passed, and port 3004 remains
healthy (`root=200`, `/operations=307`, listener PID 24923).

No production Next.js build, directory activation, command-owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator continuation — G4 Support and Incident command surface — 2026-07-28

The console Support and Incident routes now provide bounded authenticated
ticket/incident create and lifecycle-transition controls over the existing
`SupportService` RPCs. Inputs are UUID-, control-character-, size-,
cardinality-, and allow-list validated. Incident commander identity is bound
to the authenticated principal. Transition requests carry positive expected
aggregate versions, and all requests carry support audit purpose, correlation,
idempotency, platform scope, and session-bound actor context.

The server remains authoritative for Support authorization, MFA/capability
policy, scope, optimistic concurrency, SQLx persistence, snapshots,
idempotency, append-only outbox/events, worker projection/checkpoints,
reconciliation, and rollback. The console renders only safe receipt metadata
and refreshes read projections; command bodies, replies, notes, secrets, and
incident timelines are not exposed. Focused console coverage is 102/102,
strict console Clippy passes, and all console/shared UI target checks pass.
See `audits/G4-UI-SUPPORT-COMMANDS/README.md`.

This is a local G4 command-adapter completion, not a claim of full G0–G14
acceptance. External vendor/directory certification, live disposable-resource
evidence, measured million-user/five-minute certification, true PITR/WAL,
physical mobile execution, and explicit production migration/cutover approval
remain open. No activation, owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator continuation — G3 Customer 360 raw-payload redaction — 2026-07-28

The Customer 360 detail console boundary now discards the complete
`GetCustomer360DetailResponse.payload_json` blob instead of rendering it.
Only the closed ten-section state vocabulary is retained after bounded JSON
validation; malformed, oversized, incomplete, and unknown sections fail
closed. Customer identity, freshness, read-only state, unsupported reason,
and the existing cursor-bound notes model remain bounded and display-safe.

The server remains authoritative for customer scope, global-read audit,
control-plane SQLx reads, projection freshness, notes pagination, and
reconciliation. Focused console coverage is 103/103, strict console Clippy
passes, and evidence is recorded in
`audits/G3-UI-CUSTOMER-360-REDACTION/README.md`. No database, server, worker,
protobuf, Folio write, activation, owner switch, or production cutover state
was changed.

The transport is now typed as well: the server derives
`Customer360SectionState` values from the authoritative detail model and
returns an empty legacy `payload_json` field, so the aggregate JSON is not
serialized into the response.

## Coordinator continuation — G10 Marketing preview and domain commands — 2026-07-28

The shared Marketing/CRM console now exposes authenticated commands over the
existing authoritative Marketing content service for issuing and revoking
expiring previews, issuing and verifying custom-domain challenges, and
binding verified domains to exact published template revisions. Inputs are
bounded, UUID-validated, explicit-scope bound, and rebuilt into authenticated
command context; only typed receipt metadata is rendered. Preview tokens,
domain challenges, raw form values, persistence payloads, and provider
credentials remain out of the client.

The server remains authoritative for publication and verification state,
entitlement/RBAC, tenant SQLx routing, hash-only secret persistence,
idempotency, audit/outbox, expiry worker, reconciliation, and rollback.
Evidence is recorded in `audits/G10-MKT-075/README.md`. Focused console
coverage is 104/104, strict console Clippy and all available Dioxus target
checks pass. The broader G10 content read adapters, provider certification,
live SQLx evidence, external vendor evidence, migration/cutover, and Folio
write retirement remain open. No provider account was activated, no domain
was bound, and no production state changed.

## Coordinator continuation — G10 Marketing provider account controls — 2026-07-28

The authenticated Marketing Channels console now exposes the existing
provider-account onboarding commands for an explicit holding scope:
configuration with bounded enabled channels and credential-free HTTPS route
metadata, external verification acknowledgement with optional server-gated
activation, and optimistic suspension/reactivation. Secret-manager references
are opaque and only safe provider state, onboarding state, version, replay,
and correlation metadata are rendered.

The server remains authoritative for provider scope, entitlement/RBAC,
channel/route validation, verification gating, control-plane SQLx persistence,
audit/outbox, worker account routing, idempotency, reconciliation, and
rollback. Evidence is recorded in `audits/G10-MKT-076/README.md`. Focused
console coverage is 105/105, strict console Clippy passes, and all available
Dioxus target checks pass. External provider verification, live SQLx/provider
evidence, the remaining Marketing read adapters, migration/cutover, and Folio
write retirement remain open. No provider account was activated and no
external provider was contacted.

## Coordinator verification — current tree after G10-MKT-076 — 2026-07-28

PASS: the current ERPX tree passed `cargo fmt --all --check`, workspace
warnings-denied Clippy, the complete non-ignored workspace test suite,
protobuf tests, G13 compatibility/deployment checks, G14 directory/readiness
checks, and all available Dioxus web, desktop, host-mobile, iOS, Android,
and Android-bridge checks. The focused console suite is 105/105.

PASS: Folio `bun run lint` completed with zero errors and four pre-existing
n8n warnings; `bunx tsc --noEmit` passed. Folio remains operational on port
3004 (`root=200`, `/operations=307`, listener PID 24923). No production
Next.js build ran.

QUEUED: the mandated serialized ignored workspace sweep exited at exactly 66
explicit missing-resource/certification targets. These fail closed on absent
disposable SQLx URLs, MinIO/PITR resources, credentialed Generic LDAP/AD DS/
UCS environments, the approved protobuf descriptor baseline, and other
external evidence. They are not application assertion failures. Directory
activation, command-owner switching, Folio write retirement, and production
cutover remain unperformed pending explicit approval and the required
resources.

## Coordinator local acceptance wave — 2026-07-28

Fresh, exact-name disposable PostgreSQL databases were used for the available
ignored acceptance targets; no customer or Folio database was used. The
following local source/read-model/outbox/worker/reconciliation/rollback lanes
passed: identity SQLx (6/6), local directory SQLx and directory RBAC (3/3),
Support persistence (8/8), worker backup retention, Customer 360, access
audit, role-review acknowledgement, Finance import/candidate/close/journal/
banking/chart/budget/subledger/worker paths, HR persistence/projection,
Legal persistence/projection/retention, Marketing content/CRM/read projection/
consent/delivery/provider callback/journey/erasure/provider-account/abuse
paths, Operations persistence/completion/projection, POS persistence/read,
SCM persistence, organization, tenant provisioning/replacement/readiness,
G13 migration and target import, control-plane SQLx, backup jobs/retention,
tenant registry/projection scheduler/status, and support attachment expiry.

The real PostgreSQL + TLS MinIO backup/restore test also passed using the
PostgreSQL 18 dump/restore tools. The local one-million-user directory-volume
suite remains a bounded algorithmic scale proof; it is not vendor throughput
certification. The earlier aggregate ignored sweep using reused databases is
not completion evidence and is superseded by these isolated target runs.

The only unresolved acceptance resources are the credentialed Generic
LDAP/RFC 4533, on-premises AD DS/DirSync, and UCS Provisioning Service
environments and their disposable TLS evidence databases, the approved
protobuf descriptor baseline, true production PITR/WAL evidence, physical
mobile execution, and explicit migration/cutover approval. No directory was
activated, command ownership changed, Folio writes were retired, or production
cutover occurred.

## Coordinator verification delta — G1 readiness enumeration — 2026-07-28

PASS: after the readiness packet, `cargo fmt --all --check`, workspace
warnings-denied Clippy, the complete non-ignored workspace suite, protobuf
package tests, G13 compatibility/deployment checks, G14 directory checks,
and all available Dioxus web/desktop/host-mobile/iOS/Android/bridge checks
passed. The local directory volume proof passed all five tests in 51.55s.

PASS: Folio `bun run lint` completed with zero errors and four pre-existing
`infra/scripts/start-n8n.js` warnings; `bunx tsc --noEmit` passed. Folio is
still operational on port 3004 (`root=200`, `/operations=307`, listener PID
24923). No production Next.js build ran.

QUEUED: the required ignored workspace sweep exited 101 at exactly 66
resource-gated targets. The failures are missing disposable SQLx/MinIO/PITR
references, missing credentialed Generic LDAP/AD DS/UCS environments, and
the intentionally fail-closed external certification preflight; no new
application assertion failure was observed. Approved protobuf descriptor
comparison, true PITR/WAL, physical mobile execution, and explicit G13
command-owner/cutover approval remain unavailable. G0–G14 are not claimed
complete.

## Coordinator continuation — G8 Thai statutory tax fact/report packet — 2026-07-28

Finance now owns explicit Thai output-VAT, input-VAT, and withholding-payable
facts. Each fact is validated against an immutable posted journal line before
SQLx persistence; its append-only source row, idempotency record, aggregate
snapshot, and outbox event commit atomically. A tenant-read projection stores
only typed report rows and checkpoints, and a durable multi-tenant worker
handles incremental replay and rebuild. `RecordThaiTaxFact` is authenticated,
entitled, and holding/company-bound. `GetThaiTaxReport` requires `finance.read`
plus fresh MFA and returns only typed totals/facts. The Finance Dioxus surface
exposes the company/period/currency/MFA report query. Evidence:
`audits/G8-THAI-STATUTORY-TAX-2026-07-28/README.md`.

PASS: domain tax aggregation tests, worker projection-bound tests, protobuf
tests, targeted warnings-denied Clippy, formatting, server/worker/console
compilation, and desktop Dioxus checks.

QUEUED: the live SQLx tax projection acceptance is correctly ignored and fails
closed without disposable tenant write/read database URLs. Thai filing/export
certification, consolidation, broader Finance acceptance, external directory
certification, approved descriptor baseline, true PITR/WAL, physical mobile
execution, and explicit migration/cutover approval remain open. G0–G14 are not
claimed complete; no production state changed.

## Coordinator verification delta — current tree after G8 tax packet — 2026-07-28

PASS: serialized current-tree verification completed with `cargo fmt --all --check`,
workspace warnings-denied Clippy, the complete non-ignored workspace suite,
protobuf package tests, G13 compatibility/deployment checks, G14 directory
checks, and all available Dioxus web/desktop/host-mobile/iOS/Android/bridge
checks. The new Thai tax domain, worker, proto, infrastructure, server,
console, and focused projection target also passed their local checks.

PASS: Folio `bun run lint` completed with zero errors and four pre-existing
`infra/scripts/start-n8n.js` warnings; `bunx tsc --noEmit` passed. Folio remains
operational on port 3004 (`root=200`, `/operations=307`, listener PID 24923).
No production Next.js build ran.

QUEUED: the mandated ignored workspace sweep exited 101 at exactly 67
resource-gated targets, including the new tax projection acceptance. Missing
disposable SQLx/MinIO/PITR inputs, credentialed Generic LDAP/AD DS/UCS
environments, the approved protobuf descriptor baseline, true PITR/WAL,
physical mobile execution, and explicit migration/cutover approval remain
open. No application assertion failure was observed in the gated targets. G0–
G14 are not claimed complete and no production state changed.

## Coordinator verification delta — current tree after G8 consolidation — 2026-07-28

PASS: serialized workspace warnings-denied Clippy and the complete non-ignored
workspace test suite passed. Formatting, protobuf tests, the consolidated
Finance domain tests, server/infrastructure/console checks, and all available
Dioxus web/desktop/host-mobile/iOS/Android/bridge checks passed.

PASS: Folio `bun run lint` completed with zero errors and four pre-existing
`infra/scripts/start-n8n.js` warnings; `bunx tsc --noEmit` passed. Folio remains
operational on port 3004 (`root=200`, `/operations=307`, listener PID 24923).
No production Next.js build ran.

QUEUED: the mandated ignored workspace sweep exited 101 at exactly 68
resource-gated targets, including the Thai-tax and consolidated-trial-balance
acceptances. Missing disposable SQLx/MinIO/PITR inputs, credentialed Generic
LDAP/AD DS/UCS environments, the approved protobuf descriptor baseline, true
PITR/WAL, physical mobile execution, and explicit migration/cutover approval
remain open. No new application assertion failure was observed. G0–G14 are
not claimed complete and no production state changed.

## Coordinator continuation — G10 delivery projection and RFC 4533 import — 2026-07-28

The Marketing delivery worker now consumes authoritative delivery outbox pages
through a durable tenant-read cursor, validates event/aggregate identity,
replays safely after a cursor-commit crash, supports disposable read-model
reset, and exposes source/read reconciliation counts. Existing authenticated
provider callback ingress, inbox idempotency, transition events, and
multi-tenant provider routing remain in the same runtime path. Evidence:
`audits/G10-MKT-077/README.md`.

The directory full-import continuation guard now includes RFC 4533 alongside
LDAP polling, AD DirSync, and UCS Provisioning Service, preserving the bounded
one-million-user limit. Focused worker/infrastructure checks and warnings-denied
Clippy passed. The ignored SQLx delivery projection target compiles but fails
closed without disposable tenant write/read URLs. External directory/provider
certification, approved descriptor comparison, true PITR/WAL, physical mobile
execution, and explicit migration/cutover approval remain open; G0–G14 are not
claimed complete and no production state changed.

## Coordinator continuation — G8 consolidated Finance trial balance — 2026-07-28

Finance now exposes a holding-level consolidated trial balance over explicit
company chart snapshots. The domain validates common holding/currency,
company-chart coverage, account metadata consistency, immutable posted-journal
scope, overflow, and balanced totals, aggregating by account code. Intercompany
elimination is never inferred and must be an explicit posted adjustment. The
SQLx adapter consumes the existing projected Finance journal source, carries
source version, and reuses the existing outbox/projector/worker path. The
authenticated server verifies Finance chart read projections and holding scope;
the Dioxus surface displays server totals and lines only. Evidence:
`audits/G8-FIN-CONSOLIDATED-2026-07-28/README.md`.

PASS: two consolidated domain tests, formatting, protobuf/server compilation,
and the focused ignored SQLx target compilation.

QUEUED: explicit SQLx execution requires disposable tenant write/read database
URLs and fails closed without them. FX translation, intercompany elimination,
statutory consolidation packs, broader Finance reconciliation, external
directory/vendor certification, mobile execution, and production approval
remain open. G0–G14 are not claimed complete; no production state changed.

## Coordinator verification delta — current tree after G10-MKT-077 — 2026-07-28

PASS: serialized `cargo fmt --all --check`, workspace warnings-denied Clippy,
and the complete non-ignored workspace test suite passed. The G13 parallel
acceptance suite passed 9/9; protobuf package tests passed; all console and
shared UI Dioxus web/desktop/mobile/iOS/Android/bridge checks passed. The
directory volume proof passed all five tests, including the one-million-user
bounded replay test.

PASS: the focused Marketing delivery projection target compiles, and the
directory worker continuation tests cover all four native feed kinds. The
required ignored workspace sweep exited 101 at exactly 68 environment-gated
targets, including the new delivery projection acceptance; failures are the
recorded missing disposable SQLx/MinIO/PITR resources, credentialed external
directory/provider environments, or other certification inputs. No application
assertion failure was observed.

PASS: Folio `bun run lint` completed with zero errors and four pre-existing
`infra/scripts/start-n8n.js` warnings; `bunx tsc --noEmit` passed. Folio remains
operational on port 3004 (`root=200`, `/operations=307`, listener PID 24923).
No production Next.js build ran.

QUEUED: approved protobuf descriptor comparison, credentialed Generic LDAP/
RFC 4533, AD DS/DirSync and UCS Provisioning Service certification, true
PITR/WAL, physical mobile execution, and explicit migration/cutover approval.
No directory/provider activation, command-owner switch, Folio write retirement,
or production cutover occurred; G0–G14 are not claimed complete.

## Coordinator continuation — G10 delivery read transport and console surface — 2026-07-28

The authenticated Marketing content service now exposes a typed
`GetMarketingDeliveryRead` read boundary. It reuses the tenant/holding route,
checks `marketing.read` and the caller's holding scope, and returns only the
delivery state, attempt count, retry/expiry timestamps, immutable version, and
read-only/presence flags. The Marketing Channels console surface consumes that
contract and explicitly excludes recipient payloads, provider credentials, and
outbound content. Console tests cover scope binding, UUID/state validation,
missing rows, and payload-free decoding. No provider activation or production
cutover occurred.

## Coordinator final current-tree verification — 2026-07-28

PASS: `cargo fmt --all --check`; serialized workspace warnings-denied Clippy;
the complete non-ignored workspace test suite; `erpx-proto` package tests;
G13 compatibility/registration 9/9; and all console/shared-UI Dioxus web,
desktop, host-mobile, native iOS, native Android, and Android-bridge checks.
The console suite is 106/106 and the one-million-user directory volume proof
passes.

PASS: Folio `bun run lint` has zero errors and four pre-existing warnings in
`infra/scripts/start-n8n.js`; `bunx tsc --noEmit` passes. The development
server remains on port 3004 (`root=200`, `/operations=307`, listener PID
24923). No production Next.js build ran.

QUEUED: the mandated ignored workspace sweep exits 101 at exactly 68
environment-gated targets. The failures are the recorded missing disposable
SQLx/MinIO/PITR resources, credentialed Generic LDAP/AD DS/UCS/provider
environments, approved protobuf descriptor baseline, or other certification
inputs; no application assertion failure was observed. Physical mobile
execution, true PITR/WAL, and explicit migration/cutover approval remain
queued. G0–G14 are not claimed complete, and no activation, owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator local acceptance refresh — 2026-07-28

Fresh exact-name local PostgreSQL acceptance now passes the previously
resource-gated identity, control-plane, tenant, Finance, Marketing, Sales,
SCM, Asset/Maintenance, provisioning, support, role-review, worker, server,
and G13 targets. The G8 consolidated-trial-balance acceptance persists both
company chart snapshots before posting journals and passes balanced two-company
aggregation. The G6 backup round-trip passes through a temporary HTTPS MinIO
instance with PostgreSQL 18 dump/restore clients; the resident HTTP-only
MinIO service was correctly rejected by the TLS-only boundary.

The exact disposable databases and temporary MinIO resources used for this
refresh were removed. The only remaining failed acceptance is the deliberate
G14 external-readiness preflight, because credentialed Generic LDAP/RFC 4533,
on-premises AD DS/DirSync, and UCS Provisioning Service environments plus
operator revocation evidence are unavailable. Approved protobuf descriptor
baseline, true PITR/WAL, and explicit production migration/cutover approval
remain queued; no activation or ownership change occurred.

## Coordinator serialized verification refresh — 2026-07-28

PASS: the final current-tree sequence completed `cargo fmt --all --check`,
workspace warnings-denied Clippy, the serialized non-ignored workspace test
suite, `erpx-proto`, G13 parallel/deployment-recovery suites, and all required
Dioxus web, desktop, host-mobile, native iOS, native Android, shared-UI, and
Android-bridge checks. The non-ignored workspace suite passed; the console
suite remained 106/106, and the directory volume proof passed all five tests,
including its bounded one-million-user replay.

PASS: Folio `bun run lint` completed with zero errors and four pre-existing
warnings in `infra/scripts/start-n8n.js`; `bunx tsc --noEmit` passed. Port
3004 remained healthy (`root=200`, `/operations=307`). No production Next.js
build ran.

QUEUED: the required exact command
`cargo test --workspace --offline --no-fail-fast -- --ignored
--test-threads=1` exited 101 with exactly 68 environment-gated targets.
Failures were missing disposable SQLx/MinIO/PITR variables, the missing
approved protobuf descriptor reference, or missing credentialed external
directory/provider/cutover evidence; the focused fresh resource-backed local
targets passed separately. G0–G14 remain open for those external and approval
gates. No directory/provider activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator current-tree completion record — 2026-07-28

PASS: the directory API-started full-import handoff is now authoritative and
durable: a transactional `directory_imports` row and
`directory.import_started` outbox event are created under a connector advisory
lock, and the native worker claims the persisted import identity, page size,
and one-million-user bound. The fresh local SQLx handoff acceptance passed,
with no legacy queue row. Directory authentication, immutable linking, scoped
directory RBAC, AD DirSync/RFC 4533/UCS feed contracts, five-minute
revocation, TLS-only communication, MFA high-risk gates, and local-only
break-glass behavior remain fail-closed and covered by focused/domain
evidence.

PASS: G7 Operations source-cursor, notification handoff/retry, and bounded
reprojection packets; G8 Thai tax/consolidation persistence; G9 Sales/SCM
worker reconciliation; G10 delivery provider/read-model reconciliation and
payload-free authenticated read transport; G11 Quality rollback-safe domain
validation; G12 platform adapter UI coverage; and G13 target import/read/
rollback readiness are integrated and verified in the current tree. The final
serialized workspace test suite passed, as did warnings-denied workspace
Clippy, protobuf tests, all Dioxus targets, Folio lint/type checks, and port
3004 health.

QUEUED: the exact required ignored workspace sweep still exits 101 at 68
environment-gated targets. The remaining blockers are missing credentialed
Generic LDAP/RFC 4533, on-prem AD DS/DirSync, and UCS Provisioning Service
certification environments; missing disposable SQLx/MinIO/PITR resources;
the approved protobuf descriptor baseline; physical mobile execution; true
PITR/WAL evidence; and explicit production migration/cutover approval. G0–G14
remain open and no directory/provider activation, command-owner switch, Folio
write retirement, or production cutover occurred.

## Coordinator final evidence refresh — 2026-07-28

PASS: the completed parallel lanes hardened directory authentication, native
feed synchronization, directory-managed RBAC/revocation, G6/G7 operations and
backup boundaries, G8 Finance allocation/tax/projection rollback, G10
Marketing reconciliation/read transport, G11 domain rollback invariants, and
the Folio read-only bridge. Shared authorization now honors directory-managed
approver/security mappings with fail-closed scope and denial behavior. The
Folio bridge remains explicitly read-only while Folio writes and command
ownership remain unchanged.

PASS: the current tree passed `cargo fmt --all --check`, workspace Clippy with
warnings denied, the complete non-ignored workspace test suite, protobuf tests,
G14 local descriptor/mobile/PITR boundary checks, all Dioxus web/desktop/mobile
and native target checks, `bun run lint`, and `bunx tsc --noEmit`. The console
suite is 108/108. Folio port 3004 returned `root=200` and
`/operations=307`; lint retained only four pre-existing n8n warnings.

QUEUED: the exact ignored workspace sweep still exits 101 with 68
environment-gated targets. Credentialed Generic LDAP/RFC 4533, on-premises AD
DS/DirSync, UCS Provisioning Service, disposable SQLx/MinIO/PITR resources,
approved protobuf descriptor comparison, physical mobile execution, true
PITR/WAL evidence, and explicit production migration/cutover approval remain
unavailable. G0–G14 are not claimed complete; no directory/provider
activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Coordinator current-turn verification and reconciliation refresh — 2026-07-28

PASS: fresh resource-backed local acceptance passed identity directory SQLx
handoff 3/3, directory RBAC 1/1, Operations completion 2/2, Operations
projection 1/1, provisioning 4/4, control-plane SQLx 3/3, and real PostgreSQL
18 plus CA-signed HTTPS MinIO backup/restore 1/1. The Operations orphan check
now scopes mismatch detection to Operations-owned event types, while upstream
source outbox rows remain validated by authoritative source-feed
reconciliation. The scheduler fixture uses a past detection time while the
database lease remains fenced to the database clock.

PASS: serialized `cargo fmt --all --check`, workspace Clippy with warnings
denied, and the complete non-ignored workspace test suite passed. Protobuf
tests, G13 compatibility 9/9 and recovery 4/4, G14 local compatibility 5/5,
shared UI tests (90), all console web/desktop/mobile/iOS/Android checks, and
Android bridge checks passed. Folio `bun run lint`, `bunx tsc --noEmit`, and
bridge tests (5/5) passed; lint retains only four pre-existing n8n warnings.
Folio remains healthy on port 3004 (`root=200`, `/operations=307`).

QUEUED: the full ignored workspace invocation with local resources surfaced
five targets in its shared-database aggregate. The four local targets
(backup, provisioning, control-plane, and Operations projection) passed when
rerun with their exact fresh database/template inputs; the only remaining
failed target is the deliberate G14 external-readiness preflight. Credentialed
Generic LDAP/RFC 4533, on-premises AD DS/DirSync, and UCS Provisioning Service
environments are unavailable, so vendor certification and operator revocation
evidence remain queued. The approved protobuf descriptor baseline, true
PITR/WAL, physical mobile execution, and explicit production migration/
cutover approval also remain queued. Disposable databases and temporary
MinIO resources were removed; no activation, owner switch, Folio write
retirement, or production cutover occurred. G0–G14 are not claimed complete.

## Coordinator final current-turn record — 2026-07-28

PASS: the current tree now includes private-CA-bundle loading for live LDAP
TLS, preserving hostname and verified-peer fingerprint checks while rejecting
empty, malformed, oversized, or control-character paths/bundles. Directory
authentication, immutable identity linking, directory-managed RBAC, AD
DirSync/RFC 4533/UCS feed contracts, bounded one-million-user imports,
five-minute revocation, MFA high-risk gates, TLS-only communication, and
local-only break-glass remain fail-closed.

PASS: fresh real PostgreSQL acceptance passed identity directory SQLx 3/3 and
RBAC 1/1; G3 Customer 360 1/1; G5 commercial persistence 8/8; G7 Operations
persistence 5/5; G8 Thai tax projection 2/2; and G13 migration persistence
1/1. Protobuf tests passed, G13 compatibility passed 9/9, deployment recovery
passed 4/4, and G14 local compatibility passed 5/5. The G4 support workflow
packet is integrated with atomic persistence, outbox/projector/reconciliation
and redacted workflows; its credentialed SQLx worker acceptance remains
resource-gated.

PASS: serialized `cargo fmt --all --check`, workspace Clippy with warnings
denied, and the complete non-ignored workspace test suite passed. UI tests,
console web/desktop/mobile plus native iOS/Android targets, Android bridge,
Folio lint/typecheck/bridge tests, and port-3004 probes passed. Folio lint
retains four pre-existing warnings in `infra/scripts/start-n8n.js`; no
production Next.js build ran. The ten exact coordinator-created disposable
PostgreSQL databases were removed after acceptance.

QUEUED: the exact ignored workspace sweep exits 101 with 70 environment-gated
targets because disposable SQLx/MinIO/PITR resources and credentialed
Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS Provisioning Service,
provider, operator-revocation, approved-descriptor, and production approval
inputs are unavailable. Physical mobile execution and true PITR/WAL evidence
also remain queued. G0–G14 are not claimed 100% complete. No directory or
provider activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator continuation — G7 work-queue policy publication — 2026-07-28

The Operations control plane now publishes a holding-scoped WorkQueuePolicy
through the existing authoritative aggregate path. The server requires an
authenticated actor, contained holding scope, and `operations.manage`; the
SQLx adapter enforces bounded policy fields, optimistic versioning, stable
idempotency replay, immutable `operations.work_queue_policy.published` events,
tenant outbox parity, snapshots, and the rebuildable `operations_read_model`.
The Operations service exposes publish and bounded list RPCs, and the console
reads the policy projection and provides the server-owned publish control.

Fresh disposable PostgreSQL write/read acceptance passed create, projection,
idempotent replay, optimistic update, bounded JSON read, and event-count
evidence. Focused console wire-boundary tests and server compilation passed.
The packet does not claim live provider/device delivery, Folio cutover, vendor
certification, or G0–G14 completion; no production state changed.

## Coordinator continuation — G7 manager-chain enforcement — 2026-07-28

Operations exception assign, reassign, and escalate commands now resolve the
assignee's complete effective role-assignment chain at command time. The
requested manager must be the chain root, and the root must hold
`operations.exception.manage` or `operations.manage`; the domain proof also
checks active windows, scope containment, delegation links, authority levels,
and permission preservation. Independent active-reader checks are no longer
sufficient. The server remains authenticated, tenant-routed, and fail-closed;
the chain proof is validated before the existing SQLx aggregate/event/outbox
mutation. Domain coverage proves an authoritative manager passes while a
read-only root is rejected. No production state changed and G0–G14 remain
open.

## Coordinator continuation — G7 lifecycle MFA step-up — 2026-07-28

The authenticated Operations transition contract now carries an ephemeral
`mfa_code`. Server-side assign, reassign, and escalate commands require a
fresh write-side TOTP verification before tenant routing or aggregate
mutation; acknowledge and resolve retain their lower-risk command path. The
console collects the code as a password/one-time-code field and never stores
it in the command context, event, receipt, or read model. Invalid or missing
step-up material fails closed. No production state changed and G0–G14 remain
open.

## Coordinator current-tree verification — 2026-07-28

PASS: serialized `cargo fmt --all --check`, workspace warnings-denied Clippy,
and the complete non-ignored workspace test suite. Focused protobuf
compatibility tests passed G13 9/9 and G14 local 5/5. Dioxus console web,
desktop, host-mobile, iOS, and Android checks all passed after the MFA
protobuf/UI change. Folio `bun run lint` and `bunx tsc --noEmit` passed;
lint retains only the four pre-existing n8n warnings. Folio remains healthy
on port 3004 (`/` 200 and `/operations` 307), with its development server
still running. No production Next.js build ran.

QUEUED: the mandated ignored workspace sweep exited 101 with exactly 70
environment-gated targets. Missing disposable SQLx/MinIO/PITR resources,
credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS
Provisioning Service, operator-revocation evidence, the approved protobuf
descriptor baseline, physical mobile execution, and explicit production
migration/cutover approval remain blockers. G0–G14 are not claimed complete;
no activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Coordinator continuation — G7 source-event producer boundary — 2026-07-28

The Operations service now exposes `CreateOperationsExceptionFromSource` for
authenticated source-context producers. The server requires an exact actor,
contained holding scope, `operations.exception.manage`, bounded source/event
identity and versions, RFC3339 occurrence time, and bounded valid JSON. It
derives the holding reference from the active tenant registry, never accepts
client roles or permissions, and routes through the existing SQLx source
provenance/aggregate/event/outbox/read-projection path. Operations stores only
the source identity and orchestration state; upstream contexts remain owners
of source transactions and payload authority. This is an internal producer
boundary, so no client UI was added. No production state changed and G0–G14
remain open.

## Coordinator final verification — G7 source producer boundary — 2026-07-28

PASS: after the additive source-producer protobuf RPC, serialized format,
warnings-denied workspace Clippy, and the complete non-ignored workspace test
suite passed again. G13 protobuf compatibility passed 9/9 and G14 local
compatibility passed 5/5. All five Dioxus console checks (web, desktop,
host-mobile, iOS, Android) passed.

QUEUED: the final mandated ignored sweep again exited 101 with exactly 70
environment-gated targets. Its blockers are unchanged: absent disposable
SQLx/MinIO/PITR resources, credentialed Generic LDAP/RFC 4533, on-premises AD
DS/DirSync, UCS Provisioning Service, operator-revocation evidence, approved
descriptor baseline, physical mobile execution, and explicit production
migration/cutover approval. G0–G14 remain open; no production state changed.

## Coordinator continuation — G11 Quality inspection persistence — 2026-07-28

Quality inspections now have a versioned authoritative tenant aggregate and
scope-bound lifecycle mutations. The SQLx write adapter commits source state,
aggregate snapshot, complete tenant outbox envelope, and command idempotency
receipt atomically. A separate read-side adapter validates authoritative event
identity, enforces contiguous projection versions, records checkpoints,
reconciles source/read counts, and supports clear-and-replay rebuilds. The
durable tenant projection scheduler accepts `ERPX_TENANT_PROJECTION_KIND=quality`
for incremental and rebuild worker runs.

Authenticated `QualityService` provides create, start/result/release/cancel, and
read operations with explicit product scope, entitlement/RBAC, tenant routing,
customer-scope containment, and customer audit publication. The target-neutral
UI shell adds an authenticated read-only `/quality` route; server mutation
authority remains unchanged. Focused domain, worker, server, and G11 verifier
checks pass. The live two-database Quality acceptance remains ignored pending
disposable URLs; G0–G14 are not claimed complete.

## Coordinator current-tree verification — G11 Quality — 2026-07-28

PASS: serialized `cargo fmt --all --check`, warnings-denied workspace Clippy,
and the complete non-ignored workspace test suite. Quality domain, worker,
server, UI, and G11 focused checks passed; G13 protobuf compatibility passed
9/9 and G14 local compatibility passed 5/5. Dioxus console web, desktop,
host-mobile, iOS, and Android checks passed. Folio `bun run lint` and
`bunx tsc --noEmit` passed; lint retains four pre-existing n8n warnings.
Folio remains healthy on port 3004 (`/` 200 and `/operations` 307), and no
production Next.js build ran.

QUEUED: the mandated ignored workspace sweep exited 101 with 71
environment-gated targets. Missing disposable SQLx/MinIO/PITR resources,
credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS
Provisioning Service, operator-revocation evidence, the approved protobuf
descriptor baseline, physical mobile execution, and explicit production
migration/cutover approval remain open. G0–G14 are not claimed complete; no
activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Coordinator continuation — migration embedding and live Quality acceptance — 2026-07-28

The migration crate now has an explicit build script watching the control,
control-read, tenant-write, and tenant-read migration directories. This keeps
SQLx's compile-time embedded migration sets current when a numbered migration
is added during an incremental build. A fresh disposable PostgreSQL Quality
acceptance then applied migration 0053 and passed the complete source write,
outbox, projection, duplicate, reconciliation, clear/rebuild, and read-model
flow 1/1. The exact write/read databases were dropped afterward.

The same current tree passed G14 local directory SQLx acceptance 3/3,
including API/native-worker import handoff, connector feed persistence, and
role-mapping scope containment. This is local database evidence only; no
external LDAP, AD DS, or UCS endpoint was supplied or activated.

## Coordinator current-tree verification — migration embedding fix — 2026-07-28

PASS: `cargo fmt --all --check`, workspace warnings-denied Clippy, the full
non-ignored workspace tests, G13 compatibility 9/9, G14 compatibility 5/5,
all five console Dioxus web/desktop/host-mobile/iOS/Android checks, shared UI
iOS/Android checks, Folio lint/typecheck, and port-3004 health.

QUEUED: the exact ignored workspace sweep exits 101 with 71 targets because
the environment still lacks disposable SQLx/MinIO/PITR inputs, credentialed
Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service, approved
descriptor/mobile/scale evidence, and production migration/cutover approval.
G0–G14 remain open; no activation, owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator continuation — G11 Quality read surface — 2026-07-28

The Quality read model now has a bounded `ListQualityInspections` RPC and
typed Dioxus table. SQLx reads use the separate tenant read pool and keyset
paginate by projected timestamp plus inspection UUID. The transport validates
explicit holding/company/branch containment, `quality.read`, customer scope,
tenant routing, bounded cursors, row identity/version, and audit publication.
The console submits an explicit scope and renders only validated read facts;
it has no local fixtures or mutation authority. The live two-database Quality
acceptance passed the typed page and scope-filter assertions 1/1, with both
disposable databases dropped. G0–G14 remain open.

## Coordinator current-tree verification — G11 Quality read surface — 2026-07-28

PASS: post-change `cargo fmt --all --check`, workspace Clippy with warnings
denied, the full non-ignored workspace test suite, all five console Dioxus
targets, shared UI iOS/Android checks, G13 compatibility 9/9, G14 local
compatibility 5/5, Folio `bun run lint`, `bunx tsc --noEmit`, and port-3004
health (`/` 200, `/operations` 307). The exact mandated ignored sweep exits
101 with 71 environment-gated targets; its failures remain missing SQLx,
MinIO/PITR, external Generic LDAP/AD DS/UCS, mobile/scale, and approval
inputs. G0–G14 remain open; no activation, owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator continuation — G9 SCM bounded list read — 2026-07-28

Procurement and Inventory now have a bounded, scope-authorized read list over
the existing SCM SQLx projection. The separate read pool validates checkpoints,
aggregate identity and typed scope columns, rejects invalid references, and
keyset-paginates by projected timestamp, allowlisted aggregate type, and UUID.
`ListScmAggregates` enforces authenticated `scm.read`, explicit product scope,
customer scope, tenant read routing, opaque cursor bounds, audit publication,
and typed row mapping. The console renders the validated read model with an
explicit scope form, pagination, safe errors, and no mutation authority.

Fresh disposable SCM write/read databases passed all 6/6 ignored persistence
acceptance tests, including scoped list reads, cursor continuity,
reconciliation, and clear/replay rollback; both databases were removed. G11
focused acceptance passed 8/8 and console wire tests passed 108/108. Full G9
and G0–G14 acceptance remain open for broader workflow reconciliation,
worker/vendor/device/browser evidence, external directory certification, and
production approval. No production state, activation, owner switch, Folio write
retirement, or cutover occurred.

## Coordinator current-tree verification — G9 SCM list read — 2026-07-28

PASS: serialized `cargo fmt --all --check`, workspace warnings-denied Clippy,
and the complete non-ignored workspace tests. The SCM live two-database
acceptance passed 6/6; protobuf package tests passed; G13 compatibility and
deployment checks passed 9/9 and 4/4; G14 local compatibility passed 5/5.
Console web, desktop, host-mobile, iOS, Android, and shared UI iOS/Android
checks passed. Folio `bun run lint` and `bunx tsc --noEmit` passed, retaining
four pre-existing n8n lint warnings. Port 3004 remained healthy (`/` 200,
`/operations` 307), and no production Next.js build ran.

QUEUED: the exact ignored workspace sweep exited 101 with 71 environment-
gated targets. Missing disposable SQLx/MinIO/PITR resources, credentialed
Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service,
operator-revocation evidence, approved descriptor/mobile/scale evidence, and
production migration/cutover approval remain open. G0–G14 are not claimed
complete; no activation, owner switch, Folio write retirement, or cutover
occurred.

## Coordinator continuation — G11 Asset/Maintenance bounded list read — 2026-07-28

Asset and Maintenance now expose a bounded read list over the existing
tenant-read projection. `SqlxAssetMaintenanceStore::list_for_scope` validates
the separate read-side checkpoint, aggregate identity/version, typed company
and branch columns, generated reference, and asset custody-history projection;
it keyset-paginates by projected timestamp, allowlisted aggregate type, and
aggregate UUID. The typed RPC enforces explicit product scope containment,
`asset_maintenance.read`, customer scope, tenant read routing, bounded opaque
cursors, audit publication, and safe row mapping. The Maintenance console
route renders explicit-scope loading/empty/error states and a read-only table
for assets and work orders.

A fresh disposable two-database Asset/Maintenance acceptance passed 1/1,
including source/outbox projection, scoped list rows, scope isolation,
reconciliation, custody mismatch detection, and clear/rebuild rollback. The
G11 product verifier passed 8/8, console wire-boundary tests passed 109/109,
and warnings-denied focused Clippy passed. This closes the bounded list-read
implementation slice but not the full G11 gate: broader maintenance workflow,
worker restart, vendor/device/browser evidence, and production rollback or
cutover approval remain open. No production state, Folio write ownership,
directory activation, or cutover changed.

## Coordinator final ignored-sweep record after Asset/Finance packets — 2026-07-28

The mandated post-Asset/Finance command
`cargo test --workspace --offline --no-fail-fast --quiet -- --ignored
--nocapture --test-threads=1` exited 101 with exactly 71 targets. Every
failure remained an explicit missing-resource, credential, vendor, device,
scale, approved-descriptor, or production-approval guard; no new application
assertion failure was introduced. The queued evidence includes disposable
SQLx/MinIO/PITR inputs, credentialed Generic LDAP/RFC 4533, on-premises AD
DS/DirSync, UCS Provisioning Service, operator-revocation, physical mobile,
scale, and explicit migration/cutover approval. G0–G14 remain open pending
those external inputs; no directory activation, command-owner switch, Folio
write retirement, or production cutover occurred.

## Coordinator continuation — G8 Finance close-review bounded list — 2026-07-28

The existing tenant-read Finance close projection now has a bounded
company-scoped `ListFinanceCloseReviews` read path. The SQLx adapter uses the
read pool only, joins and validates the close projection checkpoint, enforces
holding/company/period identity and source event/version continuity, and
keyset-paginates lifecycle/control/statement scalars by projected timestamp
and period UUID. The RPC requires authenticated `finance.read`, explicit
scope containment, customer scope, tenant read routing, opaque cursor bounds,
and customer audit publication. The Dioxus Finance route renders bounded
loading/empty/error/read-only table and continuation states while retaining
the existing single-period detail and close commands.

Focused strict Clippy, infrastructure close-read tests 5/5, console close
tests 3/3, workspace format, and proto/server/infrastructure checks passed.
The ignored two-database close acceptance now asserts scoped list rows,
freshness/source version, sibling-company isolation, and rebuild rollback but
remains queued without the supplied disposable tenant write/read URLs. This
closes only the bounded list-read slice; provider/credential execution,
broader Finance completion, external evidence, and production approval remain
open. No production state, Folio write ownership, directory activation, or
cutover changed.

## Coordinator continuation — G9/G11 Sales bounded list read — 2026-07-28

The Sales source/read boundary now exposes a bounded company/branch list over
the separate tenant read projection. `SqlxSalesStore::list_for_scope` enforces
read-pool-only access, keyset pagination by projected timestamp, allow-listed
aggregate type, and UUID, then revalidates checkpoint identity, aggregate
domain identity, typed scope columns, and reference bounds. The typed
`ListSalesAggregates` RPC requires explicit product and customer scope,
`sales.read`, tenant read routing, opaque cursors, and audit publication. The
console now renders authenticated read-only Sales list states and continuation
while retaining the existing source-owned commands and single-aggregate read.

The ignored Sales acceptance now covers scoped list rows, source version,
sibling-company isolation, reconciliation, and clear/replay rollback, but is
queued without disposable write/read URLs. Focused Sales console, infrastructure,
server/protobuf checks passed. This closes only the Sales list-read
implementation slice; full Lead-to-Cash, live routed replay/reconciliation,
worker restart, accounting/Operations, provider/device/browser, external
directory, and production approval evidence remain open. No production state,
Folio write ownership, directory activation, or cutover changed.

## Coordinator current-tree verification — G9/G11 Sales list — 2026-07-28

PASS: `cargo fmt --all --check`, workspace Clippy with warnings denied, the
complete non-ignored workspace tests, focused Sales server/infrastructure/
console checks, and G11 Sales transport acceptance 3/3. Protobuf tests passed;
G13 parallel acceptance passed 9/9, G13 deployment/recovery 4/4, and G14 local
compatibility 5/5. Console web, desktop, host-mobile, iOS, Android, and shared
UI iOS/Android target checks passed. Folio `bun run lint` and `bunx tsc
--noEmit` passed; lint retains four pre-existing n8n warnings. Folio remains
healthy on port 3004 (`/` 200 and `/operations` 307), and no production
Next.js build ran.

QUEUED: the exact ignored workspace sweep exited 101 with exactly 71 targets,
all at explicit missing SQLx/MinIO/PITR, credentialed Generic LDAP/RFC 4533,
AD DS/DirSync, UCS Provisioning Service, operator-revocation, approved
descriptor, physical mobile/scale, or production-approval guards. No new
application assertion failure appeared. G0–G14 remain open; no activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Coordinator continuation — G6 scheduler composite binding hardening — 2026-07-28

Control migration `0064_tenant_projection_scheduler_binding.sql` now adds
composite tenant/holding foreign keys for scheduler jobs, claim batches,
claims, and rebuilds, plus scoped job/batch references for claims. This keeps
tenant identity, holding identity, worker ownership, and scheduler relations
inseparable for recovery tooling and direct SQL writers while retaining the
repository's active-registration authorization and transaction locks.

The fresh local two-database scheduler acceptance passed 1/1 and asserted all
seven named constraints after migration. `cargo test -p erpx-migration`
passed 12/12 and formatting passed. This is a persistence-integrity slice;
G6 and G0–G14 remain open for runtime/vendor/device/scale evidence, external
directory certification, approved descriptors, and production approval. No
production state changed.

## Coordinator final verification — G6 scheduler binding packet — 2026-07-28

PASS: serialized `cargo fmt --all --check`, workspace warnings-denied
Clippy, and the complete non-ignored workspace test suite. The exact ignored
sweep again exited 101 with exactly 71 explicit environment/certification
guards and no new application assertion failure. Protobuf tests, G13
parallel/recovery checks (9/9 and 4/4), G14 local compatibility (5/5), all
console Dioxus web/desktop/host-mobile/iOS/Android checks, shared UI iOS and
Android checks, Folio `bun run lint`, and `bunx tsc --noEmit` passed. Folio
remains healthy on port 3004 (`/` 200, `/operations` 307); the four existing
n8n lint warnings remain. No production Next.js build ran.

The seven scheduler binding constraints were verified against fresh local
control write/read PostgreSQL databases, which were dropped after acceptance.
G0–G14 remain open for missing external directory/vendor/device/scale/
descriptor/PITR and explicit production-approval evidence. No activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Coordinator continuation — G3 Customer 360 bounded detail facts — 2026-07-28

`GetCustomer360Detail` now carries an additive, bounded
`Customer360SectionFact` list derived from the existing authorized Customer
360 SQLx read model. The server exposes only explicit scalar facts for the
ten allow-listed detail sections; raw projection JSON, provider references,
object keys, credentials, contact contents, and audit payloads remain outside
the client contract. The Dioxus console validates the section/key allow-list,
duplicate identities, controls, and byte bounds before rendering a read-only
detail fact grid.

The packet has no migration or write-side effect. Proto/server/console checks,
112 console tests, strict server/console Clippy, and format checks passed;
evidence is recorded in `audits/G3-CUS-DETAIL-FACTS-2026-07-28/README.md`.
G3 and G0–G14 remain open for broader workflow, live database, vendor,
device, scale, descriptor, rollback, and explicit production-approval
evidence. No activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator continuation — G10 Marketing content replacement-read list — 2026-07-28

The existing Marketing content replacement projection now exposes a bounded,
authenticated metadata list for assets, templates, forms, and domains. SQLx
reads only the tenant read pool, validates projection checkpoints and source
event receipts, rejects ambiguous or stale cursors, and returns safe labels,
statuses, IDs, source versions, and projection timestamps without aggregate
state, object bytes, tokens, actors, or credential material. The server
enforces `marketing.read`, explicit scope containment, customer scope, read
routing, cursor bounds, and audit publication. The console renders
read-only content/template/form/domain tables with fail-closed decoding and
pagination.

Evidence is recorded in
`audits/G10-MKT-CONTENT-READ-2026-07-28/README.md`; proto/server/console
checks, 113 console tests, and strict focused Clippy passed. Live disposable
tenant SQLx acceptance remains queued without the required database URLs.
G10 and G0–G14 remain open for broader workflow, external certification,
device/scale/PITR evidence, and production approval. No activation, owner
switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G11/G8/G4/G13 reconciliation and recovery packets — 2026-07-28

G11 Quality reconciliation now detects stale, ahead, missing, orphan, and
version/count-mismatch inspection projections and supports bounded rebuild
recovery. G8 Finance chart reads fail closed on normalized-account versus
authoritative-header aggregate-version drift. G4 Support reconciliation
detects tampered, missing, orphaned, stale, and oversized capability-binding
projections. G13 target recovery restores durable rollback snapshots on empty
or partial target/read state while retaining source fences, idempotent outbox
replay, and the read-only Folio bridge boundary.

Evidence is recorded in `audits/G11-QUALITY-RECONCILIATION-2026-07-28`,
`audits/G8-FINANCE-CHART-2026-07-28`,
`audits/G4-SUPPORT-RECONCILIATION-2026-07-28`, and
`audits/G13-ROLLBACK-REPLAY-2026-07-28`. Focused tests, format, strict
Clippy, and the non-ignored workspace suite passed. G13 acceptance passed
9/9 and deployment/recovery passed 4/4. Live SQLx execution remains queued
without configured disposable URLs; external directory/vendor/device/scale,
PITR, approved descriptor, and production-approval evidence remain open.
No activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Coordinator continuation — local SQLx acceptance and G4 revocation-boundary fix — 2026-07-28

The local PostgreSQL/MinIO services were used only for uniquely named
disposable acceptance databases. G4 support now rejects represented-user
bindings for non-impersonation sessions at the domain constructor, matching
the persisted schema and intersection policy. Its append-only revocation
transaction updates active capabilities before the access request, preserving
the database approval-binding trigger while still committing request,
capability, outbox, projection, reconciliation, and rollback state atomically.

Real SQLx evidence passed for support workflow 1/1, support worker projection
1/1, directory local acceptance 3/3, directory RBAC 1/1, identity 6/6,
role-review persistence 2/2, role-review notification 1/1, Quality 1/1,
Finance chart/close 1/1 each, Finance persistence 2/2, banking 2/2, Legal
3/3, POS 4/4, HR 4/4, SCM 6/6, Customer 360 worker 1/1 and detail 4/4,
Operations completion 2/2 and projector 1/1, Finance candidate 1/1,
scheduler 1/1, platform completion 1/1, tenant registry 1/1, Marketing
content/CRM 1/1 each, G13 disposable acceptance 1/1, and isolated control
plane acceptance 3/3. All disposable databases were removed after each run.

External Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service,
descriptor, mobile/device, scale, PITR/MinIO credential, and production
approval evidence remain open. No activation, command-owner switch, Folio
write retirement, or production cutover occurred.

## Coordinator current-tree verification after G4 fix — 2026-07-28

PASS: `cargo fmt --all --check`, workspace warnings-denied Clippy, and the
complete non-ignored workspace suite after the support-domain and revocation
changes. Protobuf tests passed. Console web/desktop/host-mobile,
aarch64-apple-ios, aarch64-linux-android, and shared UI host/iOS/Android
checks passed. Folio `bun run lint` and `bunx tsc --noEmit` passed; lint retains
four pre-existing n8n warnings. Port 3004 remains healthy (`/` 200,
`/operations` 307). No production Next.js build ran.

QUEUED: the exact ignored workspace invocation still exits 101 with exactly
71 explicit missing SQLx/MinIO/PITR, credentialed Generic LDAP/RFC 4533,
AD DS/DirSync, UCS Provisioning Service, operator-revocation, approved
descriptor, physical mobile/scale, or production-approval guards. Local
disposable SQLx evidence is recorded above and all such databases were
removed. G0–G14 are not claimed complete; no activation, command-owner
switch, Folio write retirement, or production cutover occurred.

## Coordinator continuation — parallel G9–G14 integrity wave — 2026-07-28

The disjoint parallel completion wave added and focused-verified fail-closed
slices without changing shared manifests, protobuf, server/worker
registration, migrations, console shell, or production ownership: G6 backup
inventory evidence; G7 Operations rebuild/projector advisory locking; G8
Finance close-control source-event reconciliation; G9 Sales source fences and
rebuild rollback, procurement receipt authority, POS payment immutability and
projection reconciliation; G10 CRM attribution, journey pause/claim races,
journey cursor overflow, content version overflow, and delivery callback
ordering; G11 HR scope/version/payroll reconciliation, Quality domain and
persistence reconciliation, Legal retention evidence, Asset/Maintenance
authoritative linkage, and product cross-tenant acceptance; G12 mobile-safe
return targets and canonical Android secure-storage payloads; and G13
rollback cardinality/digest/source-fence guards plus a Folio bridge that is
`verify-full`, read-only, and snapshot-checksum fenced.

G14 acceptance now covers exact Generic LDAP/RFC4533, AD DS/DirSync, and UCS
Provisioning pairings, TLS-only configuration, RFC4533 cursor continuity,
1,000,000-user full-import evidence, append-only role revocation, immutable
AD/UCS links, five-minute timing evidence, and approved descriptor/hash
provenance. Focused package and acceptance tests passed; no live external
directory result was fabricated. Credentialed Generic LDAP/RFC4533, AD
DS/DirSync, UCS Provisioning Service, approved descriptor, mobile/device,
physical scale/PITR, and production-approval evidence remain queued.

No activation, command-owner switch, Folio write retirement, or production
cutover occurred. The combined tree requires the serialized verification
sweep below before any further gate assessment.

## Coordinator final verification — parallel G9–G14 integrity wave — 2026-07-28

PASS: after formatting the combined tree, `cargo fmt --all --check`,
workspace warnings-denied Clippy, and `cargo test --workspace --offline
--no-fail-fast --quiet` all passed. Protobuf tests passed. G14 local
compatibility passed 6/6, G13 parallel isolation 11/11, G13 deployment and
recovery 5/5, G14 connector acceptance 9/9, and G14 readiness 7/7 with its
external preflight still explicitly ignored/queued. Console web, desktop,
host-mobile, iOS, Android, and shared UI host/iOS/Android checks all passed.
Folio `bun run lint` passed with four pre-existing n8n warnings,
`bunx tsc --noEmit` passed, and Folio remained healthy on port 3004 (`/`
200, `/operations` 307). No production Next.js build ran.

QUEUED: the exact required ignored workspace command exited 101 with exactly
71 targets, all failing their explicit missing disposable SQLx/MinIO/PITR,
credentialed Generic LDAP/RFC4533, AD DS/DirSync, UCS Provisioning Service,
approved descriptor, mobile/device/scale, or production-approval guards. No
new application assertion appeared. G0–G14 remain open pending those
authoritative external/runtime gates; no activation, command-owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator continuation — local SQLx completion evidence and directory cursor hardening — 2026-07-29

PASS: isolated local acceptance covered directory/RBAC/identity and role
review (13 checks), Finance/control-plane (29 checks after fixture repair),
Sales, SCM, POS, HR, Legal, Quality, Asset/Maintenance, Marketing, journeys,
delivery, and Operations (49 checks), plus G13/G6/backup/object-storage and
Folio bridge paths. Each lane used uniquely named disposable PostgreSQL
databases and removed them after execution; no production database or
customer directory was touched. The tenant projection-status fixture now
registers its active tenant/holding through `SqlxTenantStoreRegistry` before
inserting a scheduler row, preserving the migration's composite foreign-key
contract; its isolated incremental checkpoint/idempotency test passes.

PASS: live directory cursor advancement now fails closed with an explicit
sequence-overflow error instead of silently repeating `u64::MAX` through
saturating arithmetic. The focused worker test passes. This is additive
runtime hardening with no migration or production ownership change.

QUEUED: credentialed Generic LDAP/RFC 4533, on-prem AD DS/DirSync, UCS
Provisioning Service, approved protobuf descriptor/hash, credentialed MinIO
backup and physical PITR restore, mobile/device and scale evidence, and
explicit production approval remain unavailable. Local MinIO health alone
does not constitute backup acceptance. G0–G14 are not claimed complete; no
activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Coordinator continuation — live Generic LDAP/RFC4533 handoff and revocation hardening — 2026-07-29

PASS: the coordinator closed two authoritative directory handoff defects found
by the live certification run. Full-import pages now publish a monotonic
SQLx native-feed checkpoint, preserving the last RFC 4533 cookie through a
terminal snapshot response; empty incremental native-feed pages checkpoint
and return no work instead of spinning. SQLx was built with rustls so the
required TLS-only PostgreSQL acceptance contract is actually exercised.

PASS: incremental directory changes now reuse the canonical immutable link
established by a completed full import, reject an attempted immutable
rebind, preserve compatibility for legacy direct-feed callers, and append a
directory-identity outbox event plus five-minute revocation-SLA record for
native disable/delete changes. Focused compilation and the isolated directory
SQLx acceptance passed 3/3.

PASS: a real local OpenLDAP 2.4 environment with a custom trusted LDAPS
certificate and the syncprov overlay completed the Generic LDAP/RFC4533 live
acceptance: TLS transport, full import of two users under the one-million
bound, durable cursor continuity, canonical identity linking, provider-side
disable, durable projection/outbox, and five-minute SLA evidence. The local
PostgreSQL TLS setting used only for this acceptance was restored to its
original `ssl=off` state, the disposable database was dropped, and the
OpenLDAP container was removed. The captured temporary certificate/evidence
files are quarantined under `/tmp/erpx-ldap-cert.finished.20260729` and are
not repository state.

## Coordinator continuation — final serialized verification and isolated acceptance — 2026-07-29

The final combined-tree gates passed: `cargo fmt --all --check`,
`git diff --check`, warnings-denied workspace Clippy, the complete
non-ignored workspace suite including the one-million-user volume test,
protobuf compilation, G13 compatibility/registration (12/12), G14 directory
acceptance (9/9), all Dioxus web/desktop/host-mobile/iOS/Android checks,
Folio lint/typecheck, and the port-3004 protected-route probe (307). No
production Next.js build ran. The approved descriptor comparator executed
its fail-closed path because no approved baseline was supplied.

Fresh isolated PostgreSQL reruns passed Operations persistence (8/8),
Operations worker (2/2), stale-health provisioning, control-plane SQLx (3/3),
support persistence (8/8), support worker (1/1), and the focused
directory/identity/RBAC and product acceptance lanes. The local Samba
AD-compatible LDAPS/DirSync probe passed with a full-DN bind, but it is not
Microsoft AD DS certification; Generic LDAP/RFC4533 remains the only
completed external-style directory lane. UCS was unavailable.

Credentialed Microsoft AD DS and UCS acceptance, approved descriptor/hash,
credentialed MinIO plus physical PITR/WAL restore, physical mobile/device and
measured-scale runs, and explicit production approval remain queued. No
activation, command-owner switch, Folio write retirement, or production
cutover is authorized or claimed.

## Coordinator continuation — directory authentication and compatibility hardening — 2026-07-29

The implementation now includes a concrete TLS-only Generic LDAP/AD-compatible
directory login transport and an atomic write-side ERPX session handoff. The
transport binds using the configured directory credential, searches exactly one
principal, canonicalizes provider immutable identifiers (including AD
little-endian objectGUID), rejects disabled/deleted users, and passes verified
TLS evidence into the existing directory-authentication policy. The identity
store locks the link/projection, enforces the five-minute freshness and
revocation boundary, persists device/session/access-token state, and publishes a
credential-free authentication event. This is local/AD-compatible evidence,
not Microsoft AD DS certification.

The worker now honors persisted connector import concurrency and the legacy
same-revision replay guard compares a SHA-256 digest of the complete source
payload. Conflicting same-revision payloads fail closed while identical replays
remain idempotent. G13/G14 protobuf compatibility also compares reserved field
and enum ranges/names. These changes are covered by the focused SQLx,
warnings-denied Clippy, G13 13/13, and G14 8/8 results.

The disposable official UCS image has passed native HTTPS/UMC setup validation
and accepted primary-node configuration. Its vendor setup package/restart
phase is still running; the native Provisioning Service app and ERPX
subscription/message acknowledgement have not been certified. The plan must
continue to treat UCS as queued until that complete path is exercised. The
remaining open gates are unchanged: credentialed Microsoft AD DS, UCS native
feed, approved protobuf baseline, MinIO/physical PITR, physical mobile and
scale evidence, and explicit production approval. No activation, command-owner
switch, Folio write retirement, or production cutover is allowed.

## Coordinator continuation — final parallel completion wave and UCS auth hardening — 2026-07-29

PASS: the final local wave added authoritative G9 goods-receipt posting across
the purchase-order/receipt aggregate pair; G10 CRM read-model payload drift
detection and source-fenced repair; G6 restore-marker verification across real
dump/object-storage/restore/scoped-read paths; and G12 observable UI state,
authorization, cached-payload, and accessibility markers. Focused product and
worker/console tests passed.

PASS: UCS directory authentication now uses HTTPS-only UDM REST authentication,
verified peer TLS evidence, exactly-one-user response parsing, immutable
`entryUUID` linking, and explicit disabled/locked handling. Connector preflight
permits HTTPS only for UCS while LDAP/AD stays restricted to LDAPS or StartTLS.
Ordinary local password login is denied for directory-linked principals, leaving
local credentials behind the explicit break-glass-only boundary.

PASS: final serialized `cargo clippy --workspace --all-targets --offline
-- -D warnings` and `cargo test --workspace --offline --no-fail-fast --quiet`
passed. Formatting, G13 13/13, G14 8/8, Dioxus web/desktop/iOS/Android,
Folio lint/typecheck, Folio health (200/307), and fresh local directory SQLx
acceptance (3/3) also passed. The mandated ignored sweep exits 101 with exactly
71 explicit environment/certification guards; these remain queued blockers.

QUEUED: disposable UCS setup timed out before valid domain configuration,
Provisioning Service installation, subscription/message acknowledgement, or
ERPX SQLx feed evidence. Microsoft AD DS, approved protobuf descriptor/hash,
credentialed MinIO and physical PITR/WAL restore, physical mobile/device and
measured-scale evidence, and explicit production approval remain unavailable.
No directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator continuation — G10 owner notification and G12 route safety — 2026-07-30

G10 Marketing `NotifyOwner` now reserves a deterministic, idempotent Webhook
delivery atomically with the journey cursor, outbox, and source notification.
The existing delivery worker owns leases, retry, provider binding, and
projection; replay repairs older notifications missing their reservation.
The read seam is holding-scoped and provider credentials remain secret-free.
G12 shared UI return paths now accept only bounded app-relative routes and
reject oversized, external, encoded, whitespace, and ambiguous values.

Evidence: G10 focused infrastructure/worker tests and G12 UI 104 unit tests
plus integration tests passed. Live SQLx/provider delivery, Android/iOS
secure-storage device certification, and production approval remain queued.
No activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Coordinator continuation — G7/G8/G9 additive completion packets — 2026-07-30

G7 Operations reconciliation now binds the Folio baseline to the exact
holding and rejects global, unscoped, or cross-holding baselines. G9 generic
Sales and SCM aggregate saves now bind command ID, deterministic idempotency,
containing scope, and nested relation scope at domain, infrastructure,
server, and console boundaries. G8 Finance now has a durable bank-provider
import enqueue boundary with SQLx job state, typed outbox/audit evidence,
high-risk MFA, directory-managed finance authorization, and fail-closed
provider/credential resolution; consolidation domain calculations cover FX,
ownership, and eliminations.

Evidence is being rerun after the additive packets. Focused Operations
worker tests passed 22/22, Sales persistence tests passed 9/9, and Finance
domain tests passed 46/46. External provider credentials, disposable
database certification where unavailable, approved protobuf provenance, and
production authorization remain queued. No directory activation, command
owner switch, Folio write retirement, or production cutover occurred.

## Coordinator continuation — G8 bank-provider import runtime — 2026-07-30

PASS: the Finance bank-provider import workflow is now reachable from the
authenticated console and Finance gRPC boundary. The command validates the
explicit tenant/holding scope, account binding, opaque provider/account and
credential references, date/page/retry bounds, high-risk Finance authorization,
and fresh MFA. Active control-plane route resolution supplies the source fence.
SQLx job scheduling, append-only audit, and
`finance.bank_provider_import.queued.v1` outbox intent commit atomically;
credentials and provider payloads are never persisted.

PASS: the worker root registers `finance_bank_provider_import` with an explicit
directory/local role recheck, tenant router, credential resolver, adapter
factory, batch bound, and lease. Missing secret-manager/provider integration is
an explicit fail-closed certification queue, not a mock success path. Focused
worker, server, console, formatting, and warnings-denied Clippy checks passed.

QUEUED: credentialed bank-provider environment and concrete provider adapter,
live import scale/reconciliation/rollback rehearsal, credentialed
Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service, approved
protobuf provenance, physical PITR/device/scale evidence, live Stripe/catalog
reconciliation, and production approval. G0–G14 are not claimed 100%; no
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator continuation — final certification re-audit — 2026-07-30

PASS: the coordinator re-audited the current source and found no newly
available local implementation defect. The registered native directory
transports, authoritative journey effects, billing-policy worker, entitlement
commands, SQLx/outbox/projector boundaries, and current-tree verification
evidence remain intact.

QUEUED: all remaining gates require unavailable external inputs: credentialed
Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS Provisioning Service,
approved protobuf descriptor provenance, production TLS MinIO credentials/CA,
physical PITR/WAL and device/scale evidence, live Stripe/catalog credentials,
and explicit production authorization. The exact ignored sweep remains a
fail-closed 81-target queue. No directory activation, command-owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator continuation — final reassigned G7–G14 packets — 2026-07-29

PASS: the final reassigned lanes added G11 Legal holding-scoped retention
candidate loading with idempotent expiry evidence, G10 Journey execution stale
lease-release fencing, G7 Operations notification release/retry coverage, G12
platform/offline cache redaction markers, and G14 fail-closed descriptor
mutation coverage. All changes remained in disjoint product-owned files; no
shared manifests, protobuf, migrations, authorization, console shell, or
production command ownership changed.

PASS: the final serialized verification passed `cargo fmt --all --check`,
workspace warnings-denied Clippy, the complete non-ignored workspace tests,
protobuf tests, G13 13/13, G14 9/9, Dioxus web/desktop/Android/iOS checks,
fresh local directory SQLx 3/3, Folio `bun run lint`, `bunx tsc --noEmit`, and
port-3004 health (`/` 200, `/operations` 307). No production Next.js build
ran. The exact ignored workspace sweep still exits 101 with exactly 71
explicit missing-resource/certification guards.

QUEUED: Microsoft AD DS/DirSync, UCS Provisioning Service (UCS setup remains
unassigned/degraded without machine secret or feed evidence), approved
protobuf descriptor/hash, credentialed MinIO and physical PITR/WAL restore,
physical mobile/device and measured-scale evidence, and explicit production
approval remain unavailable. No activation, command-owner switch, Folio write
retirement, or production cutover occurred; G0–G14 are not claimed complete.

## Coordinator continuation — disposable UCS join and HTTPS UDM evidence — 2026-07-29

The disposable official UCS 5.2-6 image completed the documented primary-node
join path. `ucs.erpx.test` has a machine secret and join marker; slapd,
univention-directory-listener, and Apache are active, and the forwarded HTTPS
UDM endpoint responds with TLS-only Basic authentication. A real UDM query
returned exactly one `Administrator` object with immutable
`univentionObjectIdentifier`, `disabled=false`, and `locked=false`; this is
authoritative UCS authentication evidence, not a mock or catalog-only result.

The official App Center resolves Provisioning Service 2.2.1 and its backend
2.0, but installation is refused by the vendor guard because this disposable
image has no valid UCS license key. No license bypass was attempted. Therefore
the native Provisioning Service subscription, message acknowledgement, and
ERPX SQLx worker-feed acceptance remain queued; UCS is not treated as fully
certified from UDM authentication alone. Microsoft AD DS/DirSync, approved
protobuf descriptor/hash, credentialed MinIO and physical PITR/WAL restore,
physical mobile/device and measured-scale evidence, and explicit production
approval also remain queued. No customer directory was activated and no
production ownership or Folio write path changed.

## Coordinator final verification — current parallel completion tree — 2026-07-29

PASS: the current tree now includes the latest disjoint G7–G14 hardening:
Operations execution-time authorization and notification release/retry safety;
tenant projection dead-letter claim consistency; Finance tax, budget, subledger,
close-review, close-control, and chart replay/source-event guards; Marketing
provider onboarding, delivery checkpoint idempotency, and CRM source-history
fencing; Legal and Quality relational scope rejection; HR payroll non-empty
validation; POS payment immutability; Sales duplicate-version page rejection;
G13 source-fence validation; G14 descriptor/vendor preflight regressions; and
G12 accessibility/offline/read-only state coverage. A flaky Operations test was
made deterministic by placing its transition timestamp after independently
created metadata; its authoritative invalid-state assertion now passes.

PASS: serialized `cargo fmt --all --check`, workspace Clippy with warnings
denied, and the complete non-ignored workspace suite pass. G13 parallel
acceptance is 13/13, G13 deployment/recovery is 6/6, G14 local compatibility
is 10/10, G14 directory contract acceptance is 10/10, and the fresh local
directory SQLx acceptance is 3/3. Dioxus web, desktop, aarch64 iOS, and
aarch64 Android checks pass. Folio `bun run lint` and `bunx tsc --noEmit`
pass; lint retains four pre-existing n8n warnings. Folio remains operational
on port 3004 (`/` 200 and `/operations` 307). No production Next.js build ran.

QUEUED: the mandated ignored workspace sweep exits 101 at exactly 78 targets,
all explicit missing disposable SQLx/MinIO/PITR, credentialed external
directory/vendor, approved descriptor, mobile/device/scale, or production-
approval guards. The live Generic LDAP/RFC4533 acceptance is complete, but
Microsoft AD DS/DirSync and the UCS native Provisioning Service feed remain
uncertified; the disposable UCS UDM auth passed while official app installation
requires a vendor license key. Credentialed MinIO/physical PITR, approved
protobuf baseline, physical mobile/scale evidence, and explicit production
approval remain open. No customer directory activation, command-owner switch,
Folio write retirement, or production cutover occurred; G0–G14 remain open.

## Coordinator continuation — final directory/UI audits and serialized verification — 2026-07-29

PASS: the latest disjoint audits hardened provider-native LDAP/AD lock and
delete handling, UCS deleted/locked handling, TLS-version evidence, duplicate
identity-page rejection, disabled-connector import resume fencing, finance
bank-import source-fence validation, legal-retention actor/worker fencing,
G14 readiness evidence retention, console fail-closed vendor markers, and
Dioxus high-risk MFA rendering guards. High-risk UI state never authorizes
from local session state; local credentials remain break-glass-only.

PASS: final local verification is green: `cargo fmt --all --check`, workspace
Clippy with warnings denied, the complete non-ignored workspace test suite,
`cargo test -p erpx-proto --offline`, G13 parallel 13/13, G13 deployment /
recovery 6/6, G14 local compatibility 10/10, G14 external readiness 9/9 plus
one intentionally ignored preflight, G14 directory acceptance 10/10, vendor
harness 10/10, identity feed contracts 3/3, identity sync 22/22, Dioxus web,
desktop, Android, and iOS checks, Folio `bun run lint`, and `bunx tsc
--noEmit`. Folio remains healthy on port 3004 (`/` 200, `/operations` 307);
lint retains only the four existing `start-n8n.js` warnings. No production
Next.js build ran.

QUEUED: the exact ignored workspace invocation exits 101 at 80 targets. The
failures are explicit missing disposable SQLx/control/tenant resources,
MinIO/PITR references, credentialed external vendor references, and guarded
runtime evidence; no application assertion failure was observed. Generic
LDAP/RFC4533 live acceptance remains complete. AD DS/DirSync certification is
still queued because no AD DS environment or credentials are available. Real
UCS HTTPS UDM authentication passed against the disposable UCS VM, but the
official UCS Provisioning Service installation is stopped by its valid-license
guard, so subscription/message/native-feed SQLx certification is not claimed.
The approved protobuf descriptor baseline, credentialed MinIO and physical
PITR/WAL restore, physical mobile/device and measured-scale evidence, and
explicit production approval remain open. No customer directory activation,
command-owner switch, Folio write retirement, or production cutover occurred;
G0–G14 are not claimed 100% complete.

## Coordinator continuation — external-resource re-audit — 2026-07-29

RECHECK: the current branch remains `codex/erpx-parallel-completion` with the
Folio development server healthy on port 3004. The disposable UCS QEMU VM is
still running and its forwarded HTTPS UDM endpoint returns the expected
unauthenticated Basic challenge; no new Provisioning Service license,
subscription, or native-feed evidence is available. No AD DS image, process,
credential, or exported certification reference is present in the current
workspace environment. No customer directory activation, command-owner
switch, Folio write retirement, or production cutover was performed.

QUEUED: AD DS/DirSync and UCS Provisioning Service remain the same external
certification blockers, so the active goal remains open rather than being
marked complete or blocked.

## Coordinator continuation — cross-gate hardening and final verification wave — 2026-07-29

PASS: the latest parallel wave closed concrete local gaps across G0–G14:
directory authentication now canonicalizes binary LDAP/AD immutable UUIDs,
rejects malformed/disabled connectors, and emits UCS tombstones; directory
RBAC/read-model rebuilds require matching durable outbox facts; support,
finance banking, tenant projection, backup retention, provisioning, and
Customer 360 boundaries gained additional scope/source/identity fences; G9–G11
projection workers gained route/cursor/source-page fencing; G1 secure-session
revocation now clears persisted credentials only after server success or
unauthorized response; and Dioxus UI contracts now enforce bounded redaction,
read-only/stale/reconciliation state, typed Finance scope plus MFA-required
state, and Platform Operations tenant/readiness state.

PASS: post-change `cargo fmt --all --check`, workspace warnings-denied Clippy,
and the complete non-ignored workspace test suite pass. Protobuf tests pass;
G13 parallel acceptance is 14/14 and deployment/recovery 6/6; G14 local is
11/11, directory 10/10, and readiness 9/9 plus one intentionally ignored
preflight; vendor harness is 10/10 with external certification queued;
identity feed contracts are 3/3 and identity sync is 22/22. Dioxus web,
desktop, Android, and iOS checks pass. Folio lint/typecheck pass with four
existing n8n warnings; port 3004 remains healthy (`/` 200, `/operations` 307).
No production Next.js build ran.

QUEUED: the exact ignored workspace invocation still exits 101 at 80 explicit
missing-resource targets. AD DS/DirSync credentials and provider evidence,
UCS Provisioning Service license/subscription/native-feed evidence, the
approved protobuf descriptor baseline, disposable control/tenant DB runtime
references, credentialed MinIO/physical PITR, physical mobile/device/scale
evidence, and explicit production approval remain unavailable. No customer
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred; G0–G14 are not claimed 100% complete.

## Coordinator continuation — final parallel hardening wave — 2026-07-29

PASS: concrete local gaps were closed in the G3 Customer 360 reconciliation
health predicate and bounded UI continuation cursors; G4 Support UI route
binding and aggregate-scoped outbox claiming now fail closed, with explicit
opt-in worker registration; G5 Stripe GET requests are query-only and do not
follow redirects; G6 malformed claimed backup jobs persist failed-run receipts
before retry/dead-letter; and G9–G11 product surfaces enforce holding/tenant
scope, bounded pagination, redacted cursors, and evidence-state validation.

PASS: focused tests for those slices pass; the serialized workspace Clippy and
non-ignored test suite pass; the ignored sweep was executed and exits 101 at
80 resource-gated targets with missing-resource guards only. G13 parallel
acceptance is 14/14 and deployment/recovery 6/6; G14 local is 11/11,
directory 10/10, external readiness 9/9 plus one intentionally ignored
preflight; vendor harness is 10/10; identity feed contracts 3/3 and identity
sync 22/22. Protobuf tests, Dioxus web/desktop/Android/iOS checks, Folio lint,
Folio typecheck, and port-3004 health pass. No production Next.js build ran.

QUEUED: credentialed AD DS/DirSync, UCS Provisioning Service subscription and
native-feed certification, approved protobuf descriptor provenance, disposable
SQLx runtime references, credentialed MinIO/physical PITR, physical mobile or
measured-scale evidence, and explicit production approval remain unavailable.
Generic LDAP/RFC4533 remains the completed live lane; UCS UDM authentication
is real and TLS-only, but the official Provisioning Service license guard
prevents claiming its native-feed acceptance. No customer directory
activation, command-owner switch, Folio write retirement, or production
cutover occurred; G0–G14 are not claimed 100% complete.

## Coordinator continuation — final product hardening and verification — 2026-07-29

PASS: final G9–G12 packets hardened Sales collection-payment replay and source
reconciliation, SCM cancellation/goods-receipt and one-million-event worker
pagination, Marketing provider identity and timeout/reconciliation fences,
Legal actor/version/retention integrity, POS one-million-event source fencing
with atomic rollback, and the 12-context UI/G14 acceptance matrix. The
coordinator raised the Sales worker budget to 2,000 pages at the 500-event
ceiling and routed replacement rebuilds through the existing fenced
transactional infrastructure rebuild. Shared registration, protobuf,
manifests, migration numbering, production ownership, and cutover surfaces
were untouched.

PASS: final `cargo fmt --all --check`, workspace Clippy with warnings denied,
and the complete non-ignored workspace test suite passed. Protobuf package
tests passed; G13 parallel acceptance passed 14/14; G14 local compatibility
passed 11/11; G14 readiness passed 9/9 plus its intentionally ignored
fail-closed preflight; fresh local directory SQLx passed 3/3; directory RBAC
SQLx passed 2/2; and real PostgreSQL 18 plus TLS MinIO backup/restore passed
1/1. Dioxus web, desktop, host-mobile, native iOS, native Android, and shared
UI iOS/Android checks passed. Folio `bun run lint` and `bunx tsc --noEmit`
passed; lint retains four existing `start-n8n.js` warnings. Folio remained
healthy on port 3004 (`/` 200, `/platform` 307). No production Next.js build
ran.

The exact mandated ignored workspace sweep was rerun and exits 101 with 80
guarded targets. Every reported failure is a fail-closed missing-resource or
certification guard (disposable SQLx URLs, credentialed vendor references,
MinIO/PITR references, approved protobuf baseline, or production approval),
not a new application assertion failure. Local directory SQLx evidence is
contract/worker persistence evidence and is not Generic LDAP, AD DS, or UCS
vendor acceptance.

QUEUED: credentialed Generic LDAP/RFC 4533, Microsoft AD DS/DirSync, and UCS
Provisioning Service acceptance; the UCS native feed remains blocked by the
official valid-license guard despite real TLS UDM authentication; approved
protobuf descriptor/hash provenance; physical WAL/PITR restore, device or
simulator execution, and measured one-million-user external scale evidence;
and explicit production approval. No customer directory was activated, no
command owner changed, no Folio write path was retired, and no production
cutover occurred. G0–G14 remain open rather than being claimed 100% complete.

## Coordinator continuation — role-review authority and final target verification — 2026-07-29

PASS: role-review snapshot, recovery, notification, and worker acknowledgement
paths now bind to active authoritative role assignments. Fresh local PostgreSQL
acceptance passed role-review persistence 2/2 and worker partial-ack
reconciliation 1/1; focused infrastructure, worker, console, and UI checks
also pass.

PASS: serialized format, warnings-denied workspace Clippy, non-ignored
workspace tests, protobuf/G13/G14 local suites, Dioxus targets, Folio
lint/typecheck, and port-3004 probes pass. The mandated ignored sweep was
rerun and remains 80 guarded failures because disposable SQLx/resource,
external directory, approved-descriptor, MinIO/PITR, device/scale, and
approval inputs are absent.

QUEUED: credentialed Generic LDAP/RFC 4533, Microsoft AD DS/DirSync, and UCS
Provisioning Service certification; approved protobuf provenance; physical
WAL/PITR and device evidence; measured one-million-user external scale; and
explicit production approval. No activation, ownership switch, Folio write
retirement, or cutover occurred; G0–G14 are not claimed complete.

## Coordinator continuation — parallel completion wave and serialized gates — 2026-07-29

PASS: identity session/device persistence is atomic; directory-managed
principals cannot use ordinary password login; principal-bound session
revocation and persisted revocation ordering are enforced. G2 approver scope
intersection is wired into SQLx recovery receipts with complete parent-chain
rechecks; fresh PostgreSQL role-review acceptance passed 2/2. Operations
manager hierarchy, Finance UI scope gating, and CRM UI typed-scope/redaction
guards passed focused tests.

PASS: Finance provider balance/replay, Marketing CRM provenance/replay, SCM
receipt and reconciliation, Sales workflow linkage/source reconciliation, and
commercial provider rollback focused checks passed. Sales workflow-linkage
drift now contributes to reconciliation failure. UI passed its complete
package suite, CRM 4/4, and Finance 5/5.

PASS: final serialized format, workspace warnings-denied Clippy, and
non-ignored workspace tests passed. The exact ignored sweep exited 101 with
80 guarded targets; failures were queued missing-resource/certification
inputs only. Folio lint/typecheck passed with four pre-existing n8n warnings;
Folio probes remained `/` 200 and `/platform` 307 on port 3004.

QUEUED: AD DS/DirSync credentials, UCS Provisioning Service license and
native-feed evidence, credentialed Generic LDAP/RFC 4533 acceptance, approved
protobuf descriptor/hash, disposable SQLx/vendor/MinIO/PITR inputs, physical
device/scale evidence, and explicit production approval. No activation,
ownership switch, Folio write retirement, or cutover was performed; G0–G14
remain active.

## Coordinator continuation — final post-packet audit — 2026-07-29

PASS: the late HR expense-lifecycle hardening remains compile-safe; focused
HR domain coverage is 10/10, while the four HR SQLx acceptance targets remain
correctly guarded by the disposable tenant-write database requirement. The
one-million-user directory-volume test completed during the final workspace
run, and the final format, Clippy, workspace tests, Folio lint/typecheck, and
port-3004 probes remain green.

QUEUED: no late packet changed the external certification state. AD DS/DirSync,
UCS Provisioning Service, physical PITR/device/scale, approved descriptor,
credentialed runtime databases, and production approval remain required before
any G0–G14 completion or cutover claim.

## Coordinator continuation — final serialized verification — 2026-07-29

PASS: the completion wave is integrated on `codex/erpx-parallel-completion`.
Identity directory authentication, immutable linking, directory-managed RBAC,
LDAP/RFC 4533, AD DirSync, and UCS adapter contracts are implemented with
fail-closed feed/revocation behavior and local one-million-user volume
coverage. G7–G12 focused packets passed their domain, persistence, worker,
read-model, authorization, API, reconciliation, and UI checks as applicable;
late HR, Legal, Support, Finance close, and Marketing provider hardening is
compile-safe.

PASS: `cargo fmt --all --check`, workspace Clippy with warnings denied, and
the complete non-ignored workspace test suite passed. `cargo test -p
erpx-proto --offline --quiet` passed; G13 protobuf/registration acceptance
passed 14/14 and G14 local descriptor compatibility passed 11/11. All seven
requested Dioxus web/desktop/mobile/iOS/Android checks passed. Folio
`bun run lint` passed with four existing `start-n8n.js` warnings, and
`bunx tsc --noEmit` passed. Folio remained healthy on port 3004 (`/` 200,
`/platform` 307); no production Next.js build ran.

The exact ignored workspace sweep exited 101 at 80 targets. These are
fail-closed missing-resource or certification guards only: disposable SQLx
URLs, MinIO/PITR references, external directory credentials, approved
protobuf provenance, physical device/measured-scale evidence, or production
approval. No application assertion failure was observed in that sweep.

QUEUED: credentialed Generic LDAP/RFC 4533, on-prem Microsoft AD DS/DirSync,
and UCS Provisioning Service vendor acceptance; the UCS official license gate;
credentialed runtime databases and object storage; approved descriptor/hash
provenance; physical WAL/PITR, device, and measured one-million-user evidence;
and explicit production approval. No customer directory activation,
command-owner switch, Folio write retirement, or production cutover occurred.
G0–G14 remain open and are not claimed 100% complete.

## Coordinator continuation — final defect repair and certification audit — 2026-07-30

PASS: corrected the Operations SLA claim transaction so stale/pending cleanup,
selection, and claim mutation are holding-scoped, with worker identity and
claim-token parameters bound to their authoritative columns. The SQLx control
dead-letter fixture now uses an event-specific idempotency key, eliminating a
cross-target collision without weakening idempotency. Fresh Operations
persistence passed 9/9 and fresh control-plane replay passed 3/3.

PASS: the exact ignored workspace sweep was executed with fresh shared control
and tenant databases. Its 11 failing targets were then rerun with their
documented dedicated disposable database/configuration: directory RBAC 2/2,
local directory SQLx 3/3, Marketing scheduler 1/1, Support persistence 8/8,
provisioning 4/4, Operations projector 1/1, Support attachment expiry 1/1,
and Customer 360 1/1. Compatibility checks passed: protobuf, G13 parallel
14/14, G13 recovery 6/6, G14 local 11/11, and G14 directory contracts 10/10.
Required Dioxus targets, Folio lint/typecheck, and port-3004 probes passed;
the four existing n8n lint warnings remain. No production Next.js build ran.

QUEUED: two external gates remain genuinely unavailable: credentialed live
PostgreSQL/MinIO backup certification and the fail-closed G14 preflight for
Generic LDAP/RFC 4533, on-prem AD DS/DirSync, and UCS Provisioning Service.
Approved protobuf provenance, externally measured one-million-user/five-minute
vendor evidence, physical PITR/device execution, and explicit production
approval also remain required. No customer directory activation, command-owner
switch, Folio write retirement, or production cutover occurred; G0–G14 remain
active and are not claimed 100% complete.

## Coordinator continuation — authenticated Platform role-review read surface — 2026-07-30

PASS: closed the local G2/G12 console gap for expiring-role review. The
authenticated `/platform/role-review` route is now present in the shared
allow-list and bilingual navigation, loads the authoritative
`RoleLifecycleService::ListRoleReview` RPC with actor/session metadata, and
renders only a bounded, validated read model containing principal, role/scope,
validity, risk, classification, notification category, version, and effective
state. Missing assignments, malformed required fields, redacted values, and
responses above 1,000 items fail closed. The console exposes no local renewal
or final-role recovery mutation; those remain server-authorized, MFA-gated,
immutable-link preserving, append-only audited commands.

PASS: focused shared UI route tests, console Platform Operations/QA tests, the
role-review wire-boundary regression, and console target compilation passed
after the change. Documentation is append-only. External LDAP/AD DS/UCS,
one-million-user, five-minute revocation, and production approval evidence
remain queued as recorded above; no activation, command-owner switch, Folio
write retirement, or cutover occurred.

## Coordinator continuation — role-review packet serialized verification — 2026-07-30

PASS: after the role-review route/resource/read-model/UI change, current-tree
verification passed `cargo fmt --all --check`, workspace Clippy with warnings
denied, the complete non-ignored workspace suite, `cargo check/test -p
erpx-proto`, Dioxus console web/desktop/host-mobile/iOS/Android checks, G13
parallel 14/14, G13 recovery 6/6, G14 local compatibility 11/11, G14
directory acceptance 10/10, `bun run lint` (zero errors; four pre-existing
n8n warnings), and `bunx tsc --noEmit`. Folio remained healthy on port 3004
(`/` 200, `/platform` 307, `/operations` 307); no production Next.js build
ran.

QUEUED: the exact ignored Cargo sweep exited 101 with 80 fail-closed targets
for absent disposable SQLx runtimes, credentialed backup/MinIO, Generic LDAP,
AD DS/DirSync, UCS Provisioning Service, approved descriptor provenance,
physical PITR/device/scale execution, and production approval. Those inputs
remain the only current certification blockers surfaced by the sweep; no
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator continuation — role-review packet serialized verification — 2026-07-30

PASS: after the role-review route/resource/read-model/UI change, current-tree
verification passed `cargo fmt --all --check`, workspace Clippy with warnings
denied, the complete non-ignored workspace suite, `cargo check/test -p
erpx-proto`, Dioxus console web/desktop/host-mobile/iOS/Android checks, G13
parallel 14/14, G13 recovery 6/6, G14 local compatibility 11/11, G14
directory acceptance 10/10, `bun run lint` (zero errors; four pre-existing
n8n warnings), and `bunx tsc --noEmit`. Folio remained healthy on port 3004
(`/` 200, `/platform` 307, `/operations` 307); no production Next.js build
ran.

QUEUED: the exact ignored Cargo sweep exited 101 with 80 fail-closed targets
for absent disposable SQLx runtimes, credentialed backup/MinIO, Generic LDAP,
AD DS/DirSync, UCS Provisioning Service, approved descriptor provenance,
physical PITR/device/scale execution, and production approval. Those inputs
remain the only current certification blockers surfaced by the sweep; no
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator continuation — ERPX parallel completion hardening and final verification — 2026-07-30

PASS: the six-agent completion wave is integrated on
`codex/erpx-parallel-completion`. Identity now rejects unverified high-risk
MFA evidence, enforces connector/five-minute feed freshness, immutable
directory identity collisions, and contiguous role-fact sequencing. Generic
LDAP RFC 4533 polling uses one `refreshOnly` sync-state control; bounded full
imports use the persisted connector ceiling. HR employee updates bind all
typed source columns and require exactly one optimistic-concurrency row.
Marketing delivery checkpoints are now keyed by `(projection_name,
holding_ref)` through additive tenant-read migration 0044. G3–G6 support and
backup boundaries, G9–G11 bounded source drains/high-watermarks, and G12
console Result/redaction/page-boundary handling were hardened with focused
tests.

PASS: final `cargo fmt --all --check`, workspace Clippy with warnings denied,
the exact non-ignored workspace test suite, protobuf package check, G13
parallel acceptance 14/14, G13 deployment recovery 6/6, G14 local
compatibility 11/11, all seven Dioxus web/desktop/mobile/iOS/Android checks,
`bun run lint` (zero errors; four existing `start-n8n.js` warnings), and
`bunx tsc --noEmit` passed. Fresh-database targeted SQLx reruns passed local
directory 1/1, directory RBAC 2/2, HR 4/4, Marketing checkpoint 1/1, Sales
workflow/payment and checkpoint targets, control-plane replay 3/3, Support
1/1, Customer 360 1/1, and Operations completion 2/2. Folio remained healthy
on port 3004 (`/` 200, `/platform` 307, `/operations` 307); no production
Next.js build ran. All disposable PostgreSQL databases created for this run
were removed after testing.

QUEUED: the final exact aggregate ignored-workspace invocation exits 101 at
six targets: the live MinIO/backup target lacks its credentialed runtime, the
G14 external preflight intentionally fails closed with missing Generic LDAP,
AD DS/DirSync, and UCS Provisioning Service references, and four shared-DB
aggregate targets collide on fixed test fixtures when run in one database.
Those four application targets pass independently on fresh databases; the
external gates remain genuinely uncertified. Approved protobuf descriptor
provenance, external measured one-million-user/five-minute vendor evidence,
physical WAL/PITR and mobile/device execution, and explicit production
approval remain unavailable. No customer directory activation, production
command-owner switch, Folio write retirement, or production cutover occurred;
G0–G14 remain open and are not claimed 100% complete.

## Coordinator addendum — final audit ordering — 2026-07-30

The final defect-repair and certification-audit evidence is also recorded after
the last prior verification entry: Operations persistence 9/9, control-plane
replay 3/3, all dedicated reruns green, and only the live MinIO/backup and
credentialed Generic LDAP/AD DS/UCS certification gates remain unavailable.
G0–G14 remain active; no activation, ownership switch, Folio write retirement,
or production cutover occurred.

## Coordinator continuation — live certification recheck and volume evidence — 2026-07-30

PASS: the deterministic directory volume suite passed 5/5, including the
one-million-user bounded checkpoint/restart/replay path, local break-glass and
recent-MFA boundaries, exact five-minute disable/delete revocation deadlines,
native LDAP/AD/UCS cursor continuity, and append-only role-fact projection.
Generic LDAP live transport passed its two-target harness; AD DS/DirSync and
UCS live vendor targets passed all 10 readiness/queue tests. These runs
explicitly emitted the exact missing external references and made no vendor
acceptance claim.

QUEUED: no credentialed Generic LDAP, on-prem AD DS, or UCS Provisioning
Service environment is present, and Docker/Colima is not running a usable
directory environment. Live backup/MinIO credentials are likewise absent.
The external gates remain the only unresolved completion evidence; no
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator continuation — Operations claim regression — 2026-07-30

PASS: added and ran a fresh-database SQLx regression for Operations SLA
notification claiming. It passed 1/1 and proves holding-scoped selection,
worker identity persistence, claim-token persistence, and independent claims
for a second holding. `cargo fmt --all --check` and warnings-denied focused
Clippy for the target also passed.

## Coordinator continuation — post-regression serialized verification — 2026-07-30

PASS: after the Operations regression, `cargo fmt --all --check`, workspace
Clippy with warnings denied, and the complete non-ignored workspace suite all
passed. The full Operations ignored SQLx target passed 10/10 on fresh
write/read databases. Folio lint/typecheck passed with four existing n8n
warnings, disposable databases were removed, and port 3004 remained healthy
(`/` 200, `/platform` 307, `/operations` 307). No production Next.js build or
cutover action was performed.

QUEUED: credentialed Generic LDAP, AD DS/DirSync, UCS Provisioning Service,
and live PostgreSQL/MinIO backup certification inputs remain absent.

## Coordinator continuation — G13 target-worker optimistic-fence repair and final verification — 2026-07-30

PASS: closed the G13 lifecycle-fencing gap. `save_work_checkpoint` now checks
the expected migration version under the authoritative row lock, and the G13
target worker obtains the current version before checkpoint, reconciliation,
and rollback evidence writes. Fresh two-database SQLx acceptance passed 1/1,
including version-2 progress and stale-version rejection; idempotency,
outbox evidence, and read-model projection remain green.

PASS: final current-tree format, warnings-denied Clippy, complete workspace
tests, protobuf, G13 14/14 plus recovery 6/6, G14 local 11/11, G14 directory
10/10, all Dioxus target checks, console/shared-UI focused tests, Folio lint,
TypeScript, and port-3004 probes passed. Four existing n8n lint warnings
remain; no production Next.js build ran and disposable databases were removed.

QUEUED: the aggregate ignored sweep exits 101 at 80 resource-gated targets
because disposable SQLx, credentialed backup/MinIO, Generic LDAP/AD DS/UCS,
approved protobuf, physical PITR/device/scale, and production-approval inputs
are absent. No external certification, customer directory activation,
command-owner switch, Folio write retirement, or production cutover occurred;
G0–G14 remain active and are not claimed 100% complete.

## Coordinator continuation — G13 source-fence binding repair and current-tree verification — 2026-07-30

PASS: G13 migration work enqueue now requires a SHA-256 source fence that
matches the authoritative immutable `migration_runs.source_contract_hash`.
Both ordinary and user-authorized command enqueue paths validate the binding
under a row lock before idempotency reservation or work insertion. Control
migration 0065 adds the same content-addressed fence constraint for direct
SQL and recovery writers, and the worker bridge duplicate reconcile event field
was removed.

PASS: fresh two-database SQLx acceptance passed 1/1 with mismatched-fence
rejection and no work-row side effects. Current-tree format, warnings-denied
Clippy, complete non-ignored workspace tests, protobuf checks, G13/G14
acceptance, directory volume, Dioxus/UI targets, Folio lint/typecheck, and
port-3004 probes passed. The exact ignored sweep was rerun and exits with 80
explicit missing-resource/certification gates only; no vendor certification,
production approval, directory activation, command-owner switch, Folio write
retirement, or cutover occurred.

## Coordinator continuation — UCS full-import terminal boundary and current-tree verification — 2026-07-30

PASS: UCS Provisioning Service full-import mode now treats an authoritative
empty queue (`204 No Content` or JSON `null`) as an end-of-snapshot boundary,
allowing the durable import checkpoint to enter finalization. Normal native
change-feed mode still returns `NoMessage` and retries; it does not synthesize
a snapshot boundary. The terminal page preserves provider sequence continuity,
contains no user records, and cannot acknowledge a UCS message. Regression
coverage passed for both the live terminal-page construction and durable
checkpoint transition.

PASS: current-tree worker tests passed 341/341 with two documented ignored
database guards; the corrected G14 ordered source marker and directory
acceptance passed 10/10. Required serialized checks passed: format,
warnings-denied workspace Clippy, complete non-ignored workspace tests,
protobuf tests, web/desktop/host-mobile/iOS/Android Dioxus target checks,
console/UI focused suites, Folio `bun run lint`, and `bunx tsc --noEmit`.
Lint has four pre-existing warnings in `infra/scripts/start-n8n.js` and zero
errors. Folio remained healthy on port 3004 (`/` 200, `/platform` 307,
`/operations` 307); no production Next.js build ran.

QUEUED: the exact ignored workspace invocation still exits 101 at 80
fail-closed resource/certification targets because this environment has no
disposable SQLx/backup runtime, credentialed Generic LDAP/RFC 4533, on-prem
AD DS/DirSync, UCS Provisioning Service, approved protobuf provenance,
physical PITR/device/scale evidence, or production approval. The dedicated
UCS/AD/LDAP readiness paths continue to queue those missing references without
claiming vendor certification. No customer directory activation, command-owner
switch, Folio write retirement, or production cutover occurred; G0–G14 remain
active and are not claimed 100% complete.

## Coordinator continuation — authenticated Platform role-review read surface — 2026-07-30

PASS: closed the local G2/G12 console gap for expiring-role review. The
authenticated `/platform/role-review` route is now present in the shared
allow-list and bilingual navigation, loads the authoritative
`RoleLifecycleService::ListRoleReview` RPC with actor/session metadata, and
renders only a bounded, validated read model containing principal, role/scope,
validity, risk, classification, notification category, version, and effective
state. Missing assignments, malformed required fields, redacted values, and
responses above 1,000 items fail closed. The console exposes no local renewal
or final-role recovery mutation; those remain server-authorized, MFA-gated,
immutable-link preserving, append-only audited commands.

PASS: focused shared UI route tests, console Platform Operations/QA tests, the
role-review wire-boundary regression, and console target compilation passed
after the change. Documentation is append-only. External LDAP/AD DS/UCS,
one-million-user, five-minute revocation, and production approval evidence
remain queued as recorded above; no activation, command-owner switch, Folio
write retirement, or cutover occurred.

## Coordinator continuation — role-review packet serialized verification — 2026-07-30

PASS: after the role-review route/resource/read-model/UI change, current-tree
verification passed `cargo fmt --all --check`, workspace Clippy with warnings
denied, the complete non-ignored workspace suite, `cargo check/test -p
erpx-proto`, Dioxus console web/desktop/host-mobile/iOS/Android checks, G13
parallel 14/14, G13 recovery 6/6, G14 local compatibility 11/11, G14
directory acceptance 10/10, `bun run lint` (zero errors; four pre-existing
n8n warnings), and `bunx tsc --noEmit`. Folio remained healthy on port 3004
(`/` 200, `/platform` 307, `/operations` 307); no production Next.js build
ran.

QUEUED: the exact ignored Cargo sweep exited 101 with 80 fail-closed targets
for absent disposable SQLx runtimes, credentialed backup/MinIO, Generic LDAP,
AD DS/DirSync, UCS Provisioning Service, approved descriptor provenance,
physical PITR/device/scale execution, and production approval. Those inputs
remain the current certification blockers surfaced by the sweep; no directory
activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Coordinator continuation — real local SQLx acceptance refresh — 2026-07-30

PASS: with newly created disposable PostgreSQL databases and no mocks, 34
focused assertions passed across 18 isolated acceptance targets: directory
connector/import/feed persistence 3/3, directory RBAC scope authority 2/2,
identity persistence 6/6, Operations authoritative completion 2/2, Finance
journal persistence 2/2, Finance consolidated trial balance 1/1, Thai tax
projection 2/2, Finance chart persistence 1/1, Finance budget persistence
1/1, CRM read projection 1/1, marketing delivery checkpoint 1/1, marketing
profile erasure 1/1, marketing delivery persistence 5/5, Support workflow
1/1, backup retention 1/1, tenant registry 1/1, G13 migration persistence
1/1, and role-review persistence 2/2. The corrected identity write/read
split and all read-model assertions exercised SQLx migrations, durable
outbox/projector or worker paths, scope boundaries, idempotency, and
rollback/fail-closed behavior where applicable. All disposable databases were
removed after the runs.

QUEUED: this local evidence does not replace credentialed Generic LDAP/RFC
4533, on-prem AD DS/DirSync, UCS Provisioning Service, measured external
one-million-user/five-minute revocation, physical PITR/device/scale,
approved-provenance, or production-approval evidence. No directory activation,
command-owner switch, Folio write retirement, or production cutover occurred;
G0–G14 remain active and are not claimed 100% complete.

## Coordinator continuation — final serialized verification — 2026-07-30

PASS: after the local acceptance refresh, `cargo fmt --all --check`, workspace
warnings-denied Clippy, the complete non-ignored workspace suite, protobuf
check/test, Dioxus web/desktop/host-mobile/iOS/Android checks, Folio
`bun run lint`, and `bunx tsc --noEmit` all passed. Folio remained operational
on port 3004 (`/` 200, `/platform` 307, `/operations` 307). Lint retained
four pre-existing warnings in `infra/scripts/start-n8n.js` and zero errors.
No production Next.js build ran.

QUEUED: the mandated exact ignored workspace command completed with exit 101
at 80 explicit guarded targets. The reported blockers remain unavailable
disposable SQLx/backup resources in the unconfigured aggregate environment,
credentialed Generic LDAP/RFC 4533, on-prem AD DS/DirSync, UCS Provisioning
Service, approved protobuf provenance, physical PITR/device/scale evidence,
external one-million-user/five-minute revocation measurements, and production
approval. The dedicated fresh SQLx lanes recorded above passed; no directory
activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Coordinator continuation — expanded real SQLx acceptance matrix — 2026-07-30

PASS: three parallel waves added 67 assertions across 24 fresh isolated
PostgreSQL targets with no mocks. The first wave covered Operations 10/10,
Platform 2/2, Support 8/8, Legal 3/3, HR 1/1, Marketing Content 1/1, Sales
4/4, and SCM 10/10. The second covered Finance Banking 2/2, Finance Subledger
1/1, Finance Close persistence 2/2, Finance Close projection 1/1, POS 8/8,
Quality 1/1, Marketing Journey execution 2/2, and CRM persistence 1/1. The
third covered Customer 360 1/1, access-audit 1/1, banking import jobs 1/1,
provider-account authority 1/1, delivery-provider flow 1/1, tenant projection
scheduling 1/1, control-plane SQLx persistence 3/3, and organization scoping
1/1. Every target used real SQLx migration/write/read paths and passed its
available authoritative, event/worker/projector, scope, idempotency,
reconciliation, and rollback/fail-closed assertions. All 43 disposable
databases were removed after the waves.

QUEUED: external Generic LDAP/RFC 4533, on-prem AD DS/DirSync, UCS
Provisioning Service, measured one-million-user/five-minute revocation,
approved-provenance, physical PITR/device/scale, and production-approval
evidence remain unavailable; no customer directory activation, command-owner
switch, Folio write retirement, or production cutover occurred.

## Coordinator continuation — post-matrix current-tree verification — 2026-07-30

PASS: after the expanded acceptance matrix, current-tree `cargo fmt --all
--check`, workspace Clippy with warnings denied, the complete non-ignored
workspace test suite, protobuf check/test, Dioxus web/desktop/host-mobile/iOS/
Android checks, Folio `bun run lint`, and `bunx tsc --noEmit` passed. Folio
remained healthy on port 3004 (`/` 200, `/platform` 307, `/operations` 307).
Lint retained four pre-existing `start-n8n.js` warnings and zero errors; no
production Next.js build ran.

QUEUED: the latest exact ignored workspace result remains exit 101 at 80
explicit prerequisite/certification targets. No source implementation changed
after that sweep; the remaining external requirements are credentialed Generic
LDAP/RFC 4533, on-prem AD DS/DirSync, UCS Provisioning Service, approved
protobuf provenance, measured one-million-user/five-minute revocation,
physical PITR/device/scale, and production approval. No activation, command-
owner switch, Folio write retirement, or production cutover occurred.

## Coordinator continuation — backup and recovery SQLx matrix — 2026-07-30

PASS: fresh real PostgreSQL lanes passed eight additional assertions: backup
retention reconciliation 1/1, backup external-blocker evidence 1/1, backup
retention-blocker retry evidence 1/1, tenant status checkpointing 1/1, tenant
projection scheduler rollback fencing 1/1, Finance subledger handoff 1/1,
Finance budget replay fail-closed behavior 1/1, and Finance chart projection
read state 1/1. These exercised durable control/tenant write-read paths,
authoritative inventory/blocker persistence, worker retry/reconciliation,
checkpoint idempotency, and rollback boundaries without mocks. All 14
disposable databases were removed.

QUEUED: credentialed TLS MinIO backup certification is still unavailable even
though the host MinIO process is running; no unconfigured endpoint or default
credential was treated as evidence. Generic LDAP/RFC 4533, on-prem AD
DS/DirSync, UCS Provisioning Service, approved provenance, measured
one-million-user/five-minute revocation, physical recovery/scale, and
production approval remain queued. No activation, owner switch, Folio write
retirement, or cutover occurred.

## Coordinator continuation — finance, marketing, legal, and asset SQLx wave — 2026-07-30

PASS: eight fresh real PostgreSQL targets added nine passing assertions:
Finance legacy backfill 1/1, Finance provider-account resolution 1/1,
Marketing provider onboarding 1/1, Marketing public-abuse window 1/1,
Marketing consent/suppression fencing 1/1, Marketing CRM source-fence rollback
1/1, Legal read projection 1/1, and Asset/Maintenance persistence 2/2. The
targets exercised durable migrations, authoritative writes, outbox/projector
state, provider and scope fences, idempotency, rollback, and fail-closed
behavior without mocks. All 12 disposable databases were removed.

QUEUED: the remaining vendor and operational certification inputs are still
credentialed Generic LDAP/RFC 4533, on-prem AD DS/DirSync, UCS Provisioning
Service, TLS MinIO backup credentials/CA, measured one-million-user/five-minute
revocation, approved provenance, physical recovery/scale, and production
approval. No activation, ownership switch, Folio write retirement, or
cutover occurred.

## Coordinator continuation — worker and scoped-read SQLx wave — 2026-07-30

PASS: eight fresh real PostgreSQL targets passed 14 assertions: role-review
notification failure handling 1/1, Support attachment expiry 1/1, Customer 360
detail/audit/reconciliation 4/4, POS read projection 1/1, HR persistence
envelopes 4/4, Legal document scope integrity 1/1, Quality scope integrity
1/1, and Marketing Journey persistence 1/1. These exercised worker state
transitions, read-only/security boundaries, authoritative outbox envelopes,
scope-drift rollback, cursor/read projections, and retry-safe behavior without
mocks. All 11 disposable databases were removed.

QUEUED: Operations and Finance candidate activation still require their
dedicated real admin/template database orchestration; vendor LDAP/AD DS/UCS,
TLS MinIO, measured scale/revocation, approved provenance, physical recovery,
and production approval remain unavailable. No activation, owner switch, Folio
write retirement, or cutover occurred.

## Coordinator continuation — admin/template routing acceptance — 2026-07-30

PASS: real PostgreSQL admin/template harnesses passed Operations projection
through an activated tenant route 1/1 and Finance subledger candidate
generation activation/checkpoint projection 1/1. The tests created unique
control, tenant, read, and candidate databases, ran authoritative migrations,
exercised provisioning/activation and worker projection paths, verified the
physical read database/checkpoint state, and cleaned up their resources. A
separate corrected Operations control write/read pair was also removed.

QUEUED: credentialed LDAP/AD DS/UCS certification, TLS MinIO inputs, measured
scale/revocation, approved provenance, physical recovery, and production
approval remain unavailable. No customer directory activation, owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator continuation — provisioning and G13 acceptance wave — 2026-07-30

PASS: ten real acceptance assertions passed: provisioning replacement 1/1,
readiness observations 1/1, G13 disposable PostgreSQL import 1/1, G13 target
import 1/1, Sales source pagination 1/1, Marketing delivery scheduler 1/1,
Finance close-authority replay 1/1, provisioning activation guard 1/1,
immutable owner-seed registration 1/1, and tenant organization bootstrap 1/1.
These exercised real admin/template provisioning, migrations, RLS-backed
tenant write/read separation, G13 import/replay/rollback, source cursors,
worker scheduling, immutable identity seeding, and authoritative/read
projection state. All databases created by this wave were removed. The
pre-existing legacy physical-provisioning fixtures were preserved because
their fixed names were not owned by this run.

QUEUED: full physical-provisioning fixture replay remains constrained by those
pre-existing fixed database names; no destructive replacement was performed.
Credentialed LDAP/AD DS/UCS, TLS MinIO, measured scale/revocation, approved
provenance, physical recovery, and production approval remain unavailable. No
activation, ownership switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — Finance close-control completion — 2026-07-30

PASS: the final two unexecuted local Finance close-control targets passed on
fresh tenant-write databases: missing-outbox replay fail-closed behavior 1/1
and scoped immutable source-evidence replay 1/1. Both used real migrations and
authoritative persistence; the test databases were removed afterward.

QUEUED: only the preserved legacy fixed-name physical-provisioning fixture and
external LDAP/AD DS/UCS, TLS MinIO, measured scale/revocation, approved
provenance, physical recovery, and production-approval evidence remain outside
the available local proof. No activation, ownership switch, Folio write
retirement, or production cutover occurred.

## Coordinator continuation — isolated physical-provisioning verification — 2026-07-30

PASS: the ignored physical-provisioning acceptance now accepts test-only
database-name overrides while preserving the legacy defaults. A fresh run
created separate forced-RLS tenant write/read databases, applied real
migrations, verified write/read isolation, and cleaned all four databases
afterward (1/1). The post-change verification then passed `cargo fmt --all
--check`, workspace Clippy with warnings denied, the offline workspace test
suite, protobuf check/test, Dioxus web/desktop/mobile/iOS/Android checks,
`bun run lint`, and `bunx tsc --noEmit`. Folio remained on port 3004; no
production Next.js build was used.

QUEUED: credentialed Generic LDAP/RFC 4533, on-prem AD DS/DirSync, UCS
Provisioning Service, TLS MinIO credentials/CA, measured one-million-user and
five-minute revocation certification, approved provenance, physical
recovery/scale, and production approval remain unavailable. The full ignored
database sweep remains an explicit certification queue with guarded failures
when those external inputs are absent. No customer-directory activation,
ownership switch, Folio write retirement, or production cutover occurred.

The post-change full ignored sweep was executed as
`cargo test --workspace --offline --no-fail-fast -- --ignored
--test-threads=1`: it exited 101 with exactly 80 guarded failing suites, all
due to absent disposable database, vendor, credential, or certification
inputs. The run left zero `erpx_goal_1785357010*` databases behind.

## Coordinator continuation — immutable identity-link read boundary — 2026-07-30

PASS: the directory administration read path now includes an authoritative,
cursor-bounded `ListDirectoryIdentityLinks` RPC. `SqlxDirectoryStore` reads
immutable link rows from the write-side relation with a bounded page and
exclusive canonical-key cursor; the server enforces authenticated platform
directory-read authorization, records an audit event, and returns only
secret-free identity metadata. The web console consumes the first page and
marks truncation instead of presenting a large connector as complete. Fresh
SQLx acceptance passed the two-page ordering/immutability assertions 1/1;
G13 protobuf compatibility passed 14/14, G14 directory acceptance 10/10,
workspace Clippy passed with warnings denied, and the complete non-ignored
workspace suite passed.

QUEUED: credentialed Generic LDAP/RFC 4533, on-prem AD DS/DirSync, UCS
Provisioning Service, TLS MinIO credentials/CA, measured one-million-user and
five-minute revocation certification, approved provenance, physical
recovery/scale, and production approval remain unavailable. The latest full
ignored sweep exited 101 with exactly 80 guarded suites and zero
`erpx_goal_1785357011*` database residue. No customer-directory activation,
ownership switch, Folio write retirement, or production cutover occurred.

## Coordinator continuation — native feed transport audit — 2026-07-30

PASS: current source inspection confirms that the registered worker path uses
the concrete live transports: `ldap3` TLS/StartTLS searches with RFC 4533
sync controls and durable cookies, AD DS DirSync controls with critical
deleted-object controls and opaque cookies, and UCS HTTPS Provisioning Service
message polling plus explicit message confirmation. The bounded worker uses
RFC 4533 `refreshOnly` intentionally so each native cookie is committed with
the SQLx page before acknowledgement; the documentation now states that
contract precisely. The worker unit suite passed 343 tests with 2 documented
ignored database guards.

QUEUED: these source-level and local transport tests are not vendor
certification. Credentialed Generic LDAP/RFC 4533, on-prem AD DS/DirSync,
UCS Provisioning Service, measured scale/revocation, and provider-side
failure/restart evidence still require their real environments and remain
non-activatable until certified.

## Coordinator continuation — current-tree completion audit and serialized verification — 2026-07-30

PASS: the coordinator re-audited the latest G0–G14 review packets and current
source markers for local P1/P2 gaps. Previously reported local findings are
superseded by the integrated role-review notification/receipt path, Marketing
callback ingress and delivery projection, product SQLx waves, immutable
directory identity-link read boundary, and native-feed transport hardening.
No new unblocked local implementation defect was found. The remaining open
items are external certification, physical-device/scale/PITR evidence,
approved protobuf provenance, or explicit production authorization.

PASS: on `codex/erpx-parallel-completion`, serialized current-tree checks
passed `cargo fmt --all --check`, workspace Clippy with warnings denied, and
the complete non-ignored workspace suite. The suite included 341 passing
worker tests (2 ignored guards), 105 identity tests, 129 console tests, and
the one-million-user bounded directory volume test (50.97s). Focused
protobuf, G13 parallel (14/14), G13 deployment recovery (6/6), G14 directory
(10/10), identity, console, and all five Dioxus web/desktop/mobile-host/
iOS/Android target checks passed. Folio `bun run lint` had zero errors and
four pre-existing `start-n8n.js` warnings; `bunx tsc --noEmit` passed. Folio
probes remained `/` 200, `/platform` 307, and `/operations` 307 on port
3004. `git diff --check` passed and no production Next.js build ran.

QUEUED: the mandated full ignored workspace sweep exited 101 with exactly 80
guarded targets. Every failure was an explicit missing disposable SQLx,
backup/MinIO, AD DS/DirSync, UCS Provisioning Service, approved descriptor,
physical device/scale/PITR, or production-approval input; no application
assertion failure was observed. Exact idle `erpx_goal_*` databases left by
earlier isolated acceptance runs were removed and the PostgreSQL catalog was
verified clean. No customer directory activation, command-owner switch,
Folio write retirement, or production cutover occurred; G0–G14 remain active
and are not claimed 100% complete.

## Coordinator continuation — authoritative journey effects — 2026-07-30

PASS: the production Marketing Journey execution registration now uses a
tenant-routed SQLx decision source. Branch predicates read exact profile
attributes from the authoritative write database and enforce execution-scope
containment. Email and SMS channel nodes construct deterministic delivery
aggregates from authoritative profile contacts and reserve their delivery
idempotency key, snapshot, and delivery outbox event in the same transaction
as the journey cursor advance; the existing delivery projector and provider
worker therefore consume real queued work. Focused worker execution tests
passed 9/9 and warnings-denied Clippy passed for the infrastructure and
worker packages.

QUEUED: Social, Push, Webhook, SegmentChange, Conversion, profile-tag/
notification/integration Action nodes still fail closed because their real
provider or aggregate boundary is not present. Those nodes are not claimed as
complete; provider credentials, external integration acceptance, and the
remaining vendor/physical certification inputs remain queued. No customer
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

PASS: the post-audit G14 local protobuf compatibility suite passed 11/11,
including generated Directory/PITR round trips, reserved-field and duplicate
declaration rejection, wire-stability checks, and fail-closed handling when
the approved descriptor baseline is absent. The missing approved descriptor
remains queued rather than inferred from the generated tree.

## Coordinator continuation — final current-tree verification — 2026-07-30

PASS: serialized formatting, warnings-denied workspace Clippy, and the full
non-ignored workspace suite passed after the authoritative journey-effect
packet. Focused protobuf passed; G13 parallel 14/14 and recovery 6/6; G14
local 11/11 and directory 10/10; console web/desktop/mobile plus iOS/Android;
shared UI host/iOS/Android; Folio lint (zero errors, four pre-existing n8n
warnings); TypeScript; diff hygiene; and port-3004 probes `/` 200,
`/platform` 307, `/operations` 307 all passed. No production Next.js build
ran and the development server remains running.

QUEUED: the exact ignored workspace command exited 101 with exactly 80 guarded
targets. Missing inputs are disposable SQLx/control databases, live backup/
MinIO, credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning
Service, approved protobuf provenance, physical PITR/device/scale evidence,
and production approval. Local disposable target tests that had inputs passed
and cleaned up; the PostgreSQL catalog has no `erpx_goal_*` residue. G0–G14
remain active and are not claimed 100% complete.

## Coordinator continuation — directory import progress boundary — 2026-07-30

The directory administration packet now includes an authoritative
`GetDirectoryImportStatus` query. `SqlxDirectoryStore` reads the latest or
explicit persisted full-import checkpoint, derives bounded progress for up to
1,000,000 users, validates the native cursor protocol, and deliberately keeps
opaque cursors, external IDs, source payloads, and raw failure text out of the
read model. The server requires an authenticated platform query context,
directory-read permission, audit evidence, and a real SQLx write-side row.

The Dioxus console validates and renders the status as a read-only checkpoint
per connector, including state, users/pages applied, native protocol kind,
timestamps, and safe failure category. No-import and not-found states remain
empty/read-only; no local fallback or provider command was added. Contract
and SQLx acceptance coverage now exercises the persisted one-million-user
checkpoint shape.

The current verification evidence is recorded in the latest ledger entry.
This packet does not close G0–G14: credentialed Generic LDAP/RFC 4533, AD
DS/DirSync, UCS Provisioning Service, disposable database matrix,
backup/MinIO/PITR, physical scale/device evidence, approved protobuf
provenance, and production approval remain queued. Directory activation,
command-owner changes, Folio write retirement, and production cutover remain
prohibited without explicit product-owner approval.

## Coordinator verification — directory status fix and final local acceptance wave — 2026-07-30

The directory import-status SQLx boundary now decodes PostgreSQL `INT4` page
sizes safely before applying the one-million-user limit, validates the
persisted native cursor kind, and returns only bounded progress and safe
failure categories. Fresh isolated PostgreSQL acceptance passed the API/native
worker handoff (1/1), local G14 directory import (1/1), directory RBAC scope
authority (2/2), and identity persistence (6/6). Additional isolated
infrastructure and worker lanes passed their local SQLx acceptance targets;
shared-DB fixture-contamination failures were not treated as application
evidence.

The latest serialized verification passed formatting, warnings-denied
workspace Clippy, the non-ignored workspace suite, protobuf, G13 parallel and
deployment/recovery, G14 local and directory acceptance, all Dioxus console
web/desktop/host-mobile/iOS/Android checks, shared UI native checks, the
Android bridge, Folio lint/typecheck, and port-3004 probes. The exact ignored
workspace sweep remains queued with 80 explicit environment/approval guards.
The host MinIO process is plain HTTP and has no approved CA/credential inputs,
so the HTTPS-only backup adapter correctly refuses it. Generic LDAP/RFC 4533,
AD DS/DirSync, UCS Provisioning Service, approved protobuf provenance,
physical PITR/device/scale, and production approval remain queued.

`InvokeApprovedIntegration` remains fail-closed until an authoritative
approved-integration aggregate and real provider executor exist; no mock
success path was introduced. All explicitly created `erpx_goal_*` databases
were dropped and catalog cleanup verified. G0–G14 remain active and are not
claimed 100% complete; no directory activation, command-owner change, Folio
write retirement, or production cutover occurred.

## Coordinator continuation — isolated SQLx acceptance expansion — 2026-07-30

PASS: five parallel focused waves ran 40 fresh disposable SQLx targets and
passed 111 real assertions across directory RBAC/local directory import and
identity persistence, control-plane/G13/tenant registry, Operations, Customer
360, Support diagnostics, role review, Finance, Marketing, Legal, Quality,
POS, Sales, SCM, HR, Asset/Maintenance, and journey integration. The runs
exercised authoritative migrations, forced-RLS write/read separation,
outbox/projector or worker state, idempotency, scope fences, reconciliation,
and rollback behavior as defined by each target. Every database created by
these waves was removed; no current-turn disposable database residue remains.

QUEUED: the exact ignored workspace sweep remains an 81-target fail-closed
queue when its required environment is absent. The unresolved inputs are the
credentialed Generic LDAP/RFC 4533, AD DS/DirSync, and UCS Provisioning
Service environments, TLS MinIO credentials/CA, approved protobuf descriptor,
physical PITR/device/scale evidence, and production approval. No customer
directory activation, command-owner change, Folio write retirement, or
production cutover occurred; G0–G14 remain active and are not claimed 100%.

## Coordinator final verification — 2026-07-30

PASS: the final serialized current-tree run passed `cargo fmt --all --check`,
workspace Clippy with warnings denied, and the complete non-ignored workspace
and doc-test suite. Protobuf tests and all required Dioxus console web,
desktop, iOS, Android plus shared UI host/iOS/Android and Android bridge
checks passed. Folio `bun run lint` and `bunx tsc --noEmit` passed; lint has
zero errors and the four pre-existing `start-n8n.js` warnings. `git diff
--check` passed, and Folio remains live on port 3004 with `/` 200 and the
protected routes 307. No production Next.js build ran.

QUEUED: the exact ignored sweep remains 81 explicit prerequisite/certification
guards. Real local SQLx coverage is recorded above; unresolved requirements
are credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning
Service, TLS MinIO credentials/CA, approved protobuf descriptor provenance,
physical PITR/device/scale evidence, and production approval. No directory
activation, command-owner change, Folio write retirement, or production
cutover occurred.

## Coordinator final verification after checkpoint rollback fix — 2026-07-30

PASS: the native-directory audit fixed the full-import checkpoint boundary;
staged pages no longer advance the ordinary connector checkpoint before
canonical finalization, and restart recovery publishes the staged source
cursor only after finalization succeeds. Focused worker and identity tests
passed.

PASS: the current tree passed the serialized Cargo gate, protobuf, G13
(14/14 and 6/6), G14 (11/11 and 10/10), all required Dioxus target checks,
shared UI native checks, Android bridge, Folio lint/typecheck, diff hygiene,
and port-3004 probes.

QUEUED: the exact ignored sweep remains 81 fail-closed prerequisite or
certification guards. Vendor LDAP/AD/UCS, approved descriptor, backup/MinIO,
physical PITR/device/scale, and production approval inputs remain
unavailable. No directory activation, owner switch, Folio write retirement,
or production cutover occurred; G0–G14 remain active and are not claimed
100% complete.

## Coordinator continuation — holding-scoped Directory transport — 2026-07-30

PASS: Directory administration now honors both platform and holding connector
scopes. Platform principals may administer platform or holding connectors; a
holding principal may access only the connector whose opaque holding reference
resolves to that holding's stable UUID. Company, branch, and subtree contexts
remain rejected. Connector reads, import start/status, identity-link reads,
role-mapping reads/writes, and mapping containment all use the same scope
fence while retaining directory permission checks, MFA, audit, and SQLx
authority.

PASS: identity and server focused tests passed (106 identity tests and 2
server directory-scope tests), warnings-denied server Clippy passed, and the
focused G14 transport acceptance target passed. Live LDAP/AD/UCS certification
is not claimed; its credentialed external targets remain queued.

## Coordinator correction — approved Marketing integration boundary — 2026-07-30

The current tree now contains the authoritative approved-integration boundary:
journey execution re-resolves an active, verified holding-bound provider,
persists an idempotent invocation/outbox/read-model record, and the registered
worker performs TLS-only provider delivery with bounded retry and
reconciliation. Older plan entries describing this node as absent are
historical. Live external provider certification and credentials remain
queued, so this does not imply activation or production cutover.

## Coordinator continuation — platform worker registration hardening — 2026-07-30

The compatibility handler name `platform_completion` now delegates to the
same `PlatformCompletionRuntimeAdapter` used by `platform_completion_runtime`.
Both executable names consequently enforce the same actor/request-hash/read-
model event validation, control write/read split, worker identity check, and
bounded claim configuration. The names remain aliases and deployment must
select only one claimant at a time.

Focused worker tests and warnings-denied Clippy passed. This is a local
registration correction, not live worker, vendor, PITR, device, migration, or
production acceptance. No customer directory was activated and Folio remains
the operational write owner.

## Coordinator continuation — opaque holding RBAC compatibility — 2026-07-30

The Directory authorization boundary now resolves both UUID and opaque
holding references to the same immutable UUIDv5 identity. Connector
configuration and append-only role history retain the supplied opaque
reference for backwards-compatible audit/replay semantics; shared
authorization normalizes mapping keys at read time, so legacy rows cannot
silently bypass or lose a five-minute directory revocation decision.
`DirectoryScope::permits` uses the same stable identity for connector
containment, and malformed/non-directory mapping keys fail closed.

Evidence: 107 identity library tests, identity warnings-denied Clippy, the
full G14 migrator acceptance target (11/11 directory contract tests plus
local compatibility targets), fresh disposable PostgreSQL directory SQLx
acceptance (3/3), and fresh disposable PostgreSQL RBAC scope authority (2/2).
The ignored SQLx/vendor target remains queued when its external environment is
absent; no synthetic vendor pass is claimed.

## Coordinator verification — opaque holding RBAC current tree — 2026-07-30

PASS: serialized current-tree verification passed `cargo fmt --all --check`,
workspace warnings-denied Clippy, and the complete non-ignored workspace and
doc-test suite. Focused protobuf, G13 parallel/recovery, G14 acceptance,
directory volume (5/5 including the one-million-user bounded test), console
and UI suites, Android bridge, host web/desktop/mobile, iOS, Android, and
UI-Android checks also passed. Fresh disposable PostgreSQL directory SQLx
acceptance passed 3/3 and RBAC scope authority passed 2/2; both temporary
databases were dropped.

PASS: Folio `bun run lint` completed with zero errors and four pre-existing
`infra/scripts/start-n8n.js` warnings; `bunx tsc --noEmit` passed. Folio
probes remained `/` 200, `/platform` 307, and `/operations` 307 on port 3004.
No production Next.js build ran.

QUEUED: the mandated ignored workspace sweep exited 101 with exactly 81
explicit prerequisite/certification guards. Missing inputs remain credentialed
Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS Provisioning Service,
TLS MinIO/backup and PITR evidence, approved protobuf provenance, physical
device/scale evidence, and production approval. G0–G14 remain active and are
not claimed 100% complete; no directory activation, command-owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator continuation — TLS backup round-trip acceptance — 2026-07-30

PASS: the real `PostgresMinioBackupAdapter` completed a PostgreSQL/MinIO
full-backup, TLS object upload, inventory, restore-test, forced-RLS candidate
verification, and artifact cleanup round-trip against an ephemeral TLS-only
MinIO endpoint. The acceptance used the installed PostgreSQL 18
`pg_dump`/`pg_restore` clients against the PostgreSQL 18 host and a generated
test CA explicitly supplied to the adapter; the temporary databases,
credentials, certificate material, and MinIO process were removed afterward.

QUEUED: this closes local G6 adapter evidence but does not constitute
production backup approval. The approved production CA/credential set,
physical PITR/WAL recovery evidence, and cutover authorization remain
unavailable. Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS
Provisioning Service, approved protobuf provenance, physical device/scale,
and production approval remain queued.

## Coordinator continuation — ignored SQLx matrix isolation — 2026-07-30

The complete serialized ignored workspace sweep was rerun with separate
disposable control-write, control-read, tenant-write, tenant-read, directory
RBAC, local-directory, and Marketing scheduler databases. The aggregate run
returned five targets: the TLS MinIO backup target and the intentionally
fail-closed G14 external certification preflight remain prerequisite-gated;
the control-plane, support-workflow, and Customer 360 targets were only
affected by sharing a database with other ignored targets in the aggregate
process.

Fresh isolated SQLx reruns passed control-plane 3/3, support workflow 1/1,
Customer 360 1/1, Directory RBAC 2/2, local Directory SQLx 3/3, and Marketing
scheduler 1/1. The real TLS MinIO/PostgreSQL backup round-trip already passed
1/1 with an ephemeral HTTPS MinIO endpoint and PostgreSQL 18 clients. No
application assertion failure remains in the locally runnable ignored SQLx
targets; vendor directory certification, production backup inputs, physical
PITR/device/scale evidence, and production approval remain queued.

## Coordinator verification — isolated SQLx and current tree — 2026-07-30

PASS: the serialized current-tree gate passed `cargo fmt --all --check`,
workspace warnings-denied Clippy, and the complete non-ignored workspace and
doc-test suite. Focused protobuf, G13 parallel/recovery, G14 directory/local
compatibility, console/UI tests, Android bridge, and console web/desktop/
mobile/iOS/Android plus shared UI iOS/Android target checks passed. Folio
`bun run lint` had zero errors and four pre-existing n8n warnings;
`bunx tsc --noEmit`, `git diff --check`, and port-3004 probes passed (`/` 200,
protected routes 307). No production Next.js build ran.

QUEUED: the real local SQLx lanes pass with isolated disposable databases,
but external Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service,
approved protobuf provenance, physical PITR/device/scale evidence, and
production approval are still unavailable. G0–G14 remain active and are not
claimed complete; no activation, ownership switch, Folio write retirement, or
production cutover occurred.

## Coordinator verification — platform worker registration hardening — 2026-07-30

After the shared worker edit, the serialized current-tree checks passed
`cargo fmt --all --check`, workspace warnings-denied Clippy, and the complete
non-ignored workspace/doc-test suite. Focused protobuf, G13 parallel/recovery,
G14 directory/local compatibility, all requested console and shared-UI target
checks, Folio lint/typecheck, diff hygiene, and port-3004 probes passed. Folio
remained healthy at `/` 200 with protected `/platform` and `/operations`
redirects 307; no production Next.js build ran.

The ignored SQLx sweep and external certification remain queued when required
inputs are absent. Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning
Service, approved protobuf provenance, physical PITR/device/scale evidence,
and production approval are not claimed. G0–G14 remain active; no directory
activation, command-owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — bounded billing-policy worker adoption — 2026-07-30

The existing authoritative commercial billing-policy command is now adopted by
the registered `stripe_billing_policy` worker. It uses bounded
`FOR UPDATE SKIP LOCKED` selection for due `past_due` and expired-`grace`
lifecycles, deterministic row/time idempotency references, and one transaction
for domain transition, snapshot, outbox, audit, and receipt persistence. The
legacy Stripe semantic handler remains provider-evidence-only so payment
recovery and cancellation authority are not moved into the policy worker.

PASS: infrastructure commercial tests 29/29 active, worker registration 1/1,
warnings-denied focused Clippy, and fresh disposable PostgreSQL acceptance for
the infrastructure atomic worker boundary 1/1 and worker replay behavior 1/1.
The worker fixture advanced one due row and its immediate replay advanced zero.

QUEUED: live Stripe credentials/provider/catalog reconciliation, external
directory certification, approved protobuf provenance, physical PITR/device/
scale evidence, and production approval remain unavailable. G0–G14 are not
claimed 100%; no directory activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator verification — billing-policy worker current tree — 2026-07-30

PASS: after the billing-policy worker packet, serialized `cargo fmt --all
--check`, workspace warnings-denied Clippy, and the complete non-ignored
workspace/doc-test suite passed. The exact ignored invocation
`cargo test --workspace --offline --no-fail-fast --quiet -- --ignored` was
rerun and exited 101 with exactly 81 explicit missing-resource or
certification guards; no application assertion failed. Focused protobuf,
G13 parallel/recovery (14/14 and 6/6), G14 local/directory (11/11 and 11/11),
all console web/desktop/host-mobile/iOS/Android, shared UI host/iOS/Android,
Android bridge, Folio lint/typecheck, `git diff --check`, and port-3004 probes
passed. Folio lint had zero errors and four pre-existing n8n warnings; no
production Next.js build ran.

PASS: the new billing-policy worker ignored SQLx acceptance passed 1/1 with
fresh disposable PostgreSQL databases, and the existing infrastructure batch
boundary passed 1/1. Temporary databases were removed. QUEUED: credentialed
Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service, approved
protobuf provenance, TLS MinIO/production backup and physical PITR, physical
device/measured-scale evidence, live Stripe acceptance, and production
approval remain unavailable. G0–G14 are not claimed 100%; no activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Coordinator continuation — entitlement command runtime and console — 2026-07-30

PASS: G5 commercial entitlement allocation and activation are now exposed as
authoritative SQLx-backed commands. Allocate-company, activate-company, and
activate-branch use expected-version fencing, aggregate locking, durable
idempotency, snapshots, typed outbox/state events, append-only audit, and
receipts in one transaction. The server RPC boundary enforces authenticated
actor matching, holding/company/branch containment, subscription lifecycle,
module entitlement, directory-compatible role access, and SoD. The ERPX
console now has a Billing & Entitlements surface that evaluates the server
read model and submits the real commands.

Evidence: infrastructure commercial active tests 29/29, fresh disposable
PostgreSQL command acceptance 1/1, server tests 94/94, and focused console
desktop compilation passed. Full current-tree verification is rerun after
this packet. Production Stripe/catalog reconciliation, credentialed external
LDAP/AD/UCS certification, approved protobuf provenance, physical recovery /
device / scale evidence, and production approval remain queued. No directory
activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Coordinator verification — entitlement MFA boundary — 2026-07-30

PASS: entitlement allocation and company/branch activation now require a
fresh one-time MFA step-up at the server boundary. MFA is transported only in
the RPC request, never placed in command context, idempotency, audit, outbox,
receipt, error, or UI status text. The console presents the high-risk step-up
field and the existing expected-version/idempotency command flow.

PASS: after this additive protobuf change, serialized formatting, workspace
warnings-denied Clippy, the complete non-ignored workspace/doc-test suite,
protobuf, G13 parallel/recovery (14/14 and 6/6), G14 local/directory
(11/11 and 11/11), all console web/desktop/host-mobile/iOS/Android and shared
UI/Android bridge checks, Folio lint/typecheck, diff hygiene, and port-3004
probes passed. Folio lint still has zero errors and four pre-existing n8n
warnings; no production Next.js build ran.

The exact ignored sweep remains a fail-closed 81-target resource/certification
queue with no application assertion failure. Credentialed LDAP/AD/UCS,
approved descriptor, TLS MinIO/physical PITR/device/scale, live Stripe, and
production approval inputs remain unavailable; G0–G14 are not claimed 100%.
No directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator final local verification — 2026-07-30

PASS: the final serialized gate passed `cargo fmt --all --check`, workspace
Clippy with warnings denied, and the complete non-ignored workspace suite.
`erpx-proto` tests/docs passed; G13 protobuf/migration compatibility passed
6/6 and parallel acceptance 14/14; G14 local compatibility passed 11/11 and
directory contracts 11/11. Console Dioxus web, desktop, host-mobile, iOS,
and Android checks passed, as did shared UI host/iOS/Android checks and the
Android bridge test. Folio `bun run lint` returned zero errors with four
pre-existing `infra/scripts/start-n8n.js` warnings; `bunx tsc --noEmit`
passed. Port 3004 remained healthy: `/` 200 and protected routes 307.

The exact ignored invocation exited 101 with exactly 81 explicit
missing-resource or certification guards and no application assertion
failure. The remaining evidence requires credentialed Generic LDAP/RFC 4533,
on-premises AD DS/DirSync, UCS Provisioning Service, disposable PostgreSQL
URLs for the full SQLx matrix, approved protobuf descriptor provenance, live
TLS MinIO/physical PITR, device-backed secure storage, measured scale, live
Stripe/provider credentials, and production approval. G0–G14 therefore
remain active rather than falsely claimed 100%; no directory activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Current coordinator audit — 2026-07-30

PASS: final formatting, warnings-denied workspace Clippy, the complete
non-ignored workspace suite, protobuf/docs, G13 recovery/parallel (6/6 and
14/14), G14 local/directory (11/11 and 11/11), all requested Dioxus
console/shared-UI targets, Android bridge, Folio lint/typecheck, and port 3004
probes passed. The exact ignored sweep returned exactly 81 explicit
missing-resource or certification guards and no unexpected assertion failure.

PASS: isolated local SQLx evidence includes identity 6/6, RBAC 2/2, directory
3/3, Finance consolidation 2/2, Marketing journey 6/6, delivery 5/5,
scheduler 1/1, HR 5/5, Sales 4/4, SCM 10/10, POS 9/9, Quality 2/2 plus scope
1/1, Finance controls 10/10, and Operations slices. Local PostgreSQL was
non-TLS-only and is not production or vendor certification evidence.

QUEUED: external LDAP/AD/UCS environments and credentials, remaining SQLx
database URLs, approved protobuf provenance, TLS MinIO/physical PITR,
device/scale evidence, live provider credentials, G13 runtime references, and
production approval. G0–G14 remain active; no activation, command-owner switch,
Folio write retirement, or production cutover occurred.

## Final verification receipt — 2026-07-30

IMPLEMENTED: bounded directory TLS freshness and trust-anchor pinning, native
UCS/RFC 4533 feed correctness, fail-closed directory RBAC scope/revocation,
and tenant/holding lifecycle validation for platform completion commands.

VERIFIED: all required serialized Cargo gates, protobuf/G13/G14 checks,
Dioxus web/desktop/Android/iOS and bridge checks, Folio lint/typecheck, diff
hygiene, and port 3004. The exact ignored sweep remains 81 explicit
resource/certification guards; G0–G14 are not claimed 100%.

QUEUED: credentialed LDAP, AD DS, UCS, remaining SQLx databases, approved
descriptor provenance, TLS MinIO/physical PITR, device/scale/provider
evidence, G13 runtime references, and production approval. No activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Current coordinator receipt — 2026-07-30

IMPLEMENTED: the final parallel review corrected bounded directory TLS
certificate evidence and late bind-deadline handling, source/outbox locking
for Marketing content projection rebuilds, Operations source-owned outbox
fencing and invalid-cursor rejection, accessible IDs for repeated G12 panels,
provider-bound Generic LDAP revocation evidence, and directory-feed attribute
and checkpoint validation. Finance replay, transaction-digest, budget/chart,
and banking projection evidence was also completed with SQLx persistence and
fail-closed reconciliation.

VERIFIED: the clean serialized workspace suite passed; format, warnings-denied
Clippy, protobuf/G13/G14 compatibility, all requested Dioxus targets and
Android bridge, Folio lint/typecheck, `git diff --check`, and port 3004 passed.
The exact ignored sweep completed with 81 explicit missing-resource or
certification guards and no unexpected assertion failure. No disposable ERPX
databases remain.

QUEUED: credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning
Service, remaining SQLx URLs, approved descriptor provenance, TLS MinIO and
physical PITR, device/scale/provider evidence, G13 runtime references, and
production approval. G0–G14 remain active and unclaimed at 100%; no directory
activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Coordinator continuation receipt — 2026-07-30

IMPLEMENTED: corrected the Thai-tax SQLx acceptance fixture so it writes both
authoritative fact and outbox rows before projecting; this now verifies the
real source/read reconciliation contract rather than an outbox-only fixture.

VERIFIED: isolated disposable PostgreSQL reruns passed identity/RBAC 8/8,
local directory 3/3, Thai tax, all infrastructure commercial/support/
Finance library targets, infrastructure backup retention and role-review
targets, all worker database targets including operations, Customer 360,
access audit, banking import, support, Stripe, and backup-blocker paths. The
clean workspace suite, format, warnings-denied Clippy, and repository hygiene
also passed. All temporary databases were removed; Folio remains healthy on
port 3004.

QUEUED: the exact no-environment ignored sweep remains 81 explicit guards.
Credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service,
live MinIO/PITR backup, approved descriptor provenance, measured external
scale/device/provider evidence, and production approval remain unavailable.
No missing-resource result is promoted to certification; G0–G14 remain active
and unclaimed at 100%, with no activation, owner switch, Folio write
retirement, or production cutover.

## Coordinator continuation readiness receipt — 2026-07-30

VERIFIED: current-tree G14 external readiness static checks passed 9/9. The
ignored certification preflight fails closed and enumerates the exact missing
Generic LDAP/RFC 4533, on-premises AD DS/DirSync, and UCS Provisioning Service
database, endpoint, trust-anchor, bind, import-scale, native-revocation, and
evidence references. No vendor acceptance is inferred from local adapter or
static harness coverage.

QUEUED: the external inputs remain absent in the current environment, as do
live backup/MinIO credentials, approved protobuf descriptor provenance, and
physical/device/scale/provider evidence. Folio is still operational on port
3004. The active G0–G14 goal remains open; no activation, owner switch, Folio
write retirement, or production cutover occurred.

## Coordinator continuation — real G13 disposable acceptance — 2026-07-30

VERIFIED: `g13_disposable_postgres_acceptance_is_real_and_cutover_free` passed
1/1 against the local PostgreSQL admin service. It created six isolated
databases, migrated control/tenant write/read/rollback stores, extracted a
real Folio source snapshot, ran the target import/projector/reconciliation and
rollback evidence, enforced the post-cutover Folio-write fence, and cleaned
up every temporary database. No command ownership or production state was
changed.

QUEUED: this closes the local G13 disposable-runtime evidence gap only;
credentialed G14 directory certification, live backup/MinIO/PITR, approved
descriptor provenance, physical/device/scale/provider evidence, and explicit
production approval remain required.

## Coordinator continuation — directory transport audit — 2026-07-30

VERIFIED: the current production worker path uses real rustls-only LDAP/BER
transports and UCS HTTPS provisioning, with provider-specific RFC 4533, AD
DirSync, and UCS native cursors. Provider-neutral feed/volume contracts passed
3/3; vendor adapter harnesses passed 10/10; Generic LDAP harnesses passed
9/9; and the one-million-user, immutable-link, five-minute revocation,
directory-managed role, TLS, and break-glass boundaries remain covered by
current executable contracts and SQLx evidence.

QUEUED: these local proofs do not constitute live vendor acceptance. Credentialed
Generic LDAP, on-premises AD DS, and UCS endpoints plus their trust/credential,
revocation handoff, measured-scale, and provider evidence are still absent.

## Coordinator continuation — real TLS MinIO backup acceptance — 2026-07-30

VERIFIED: `real_postgres_minio_backup_restore_and_cleanup_are_authoritative`
passed 1/1 against a disposable HTTPS MinIO server signed by a generated test
CA and the local PostgreSQL service. The run used the PostgreSQL 18
`pg_dump`/`pg_restore` clients, real S3-compatible object transport, forced-RLS
candidate verification, tenant-marker verification, inventory, artifact
cleanup, and disposable database cleanup. No mocks or production credentials
were used. Temporary MinIO state and certificate material were moved to the
OS trash after the run; no `erpx_goal26_*` database residue remains. The
existing unrelated `erpx_bk_manual` database was preserved.

QUEUED: approved production MinIO/TLS credentials, physical PITR/WAL recovery,
external directory certification, approved protobuf descriptor provenance,
physical device/scale/provider evidence, and explicit production approval
remain unavailable. This local acceptance does not authorize production use or
cutover; no directory activation, command-owner change, Folio write retirement,
or production cutover occurred.

## Coordinator continuation — serialized final verification — 2026-07-30

VERIFIED: `cargo fmt --all --check`, workspace warnings-denied Clippy, the
complete non-ignored workspace test suite, protobuf tests, and the selected
G13/G14 compatibility suites passed. The mandated ignored workspace sweep was
rerun and returned exactly 81 fail-closed missing-resource or certification
guards; no unexpected application assertion was observed. Dioxus console web,
desktop, host-mobile, iOS, and Android checks passed, as did shared UI web/iOS/
Android and the Android bridge. Folio `bun run lint` passed with only the four
pre-existing n8n-script warnings, `bunx tsc --noEmit` passed, `git diff --check`
passed, and Folio remained healthy on port 3004 (`/` 200, protected route 307).

QUEUED: the 81 ignored guards remain explicit prerequisites rather than passes:
credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS
Provisioning Service, approved protobuf descriptor provenance, production
backup/PITR and provider credentials, physical device/scale evidence, and
production approval. G0–G14 remain active and are not claimed at 100%; no
directory activation, command-owner change, Folio write retirement, or
production cutover occurred.

## Coordinator continuation — physical PITR capture/restore acceptance — 2026-07-30

IMPLEMENTED: worker backup execution now routes persisted `pitr` runs through
the real PostgreSQL physical-capture adapter only when explicit WAL-archive
configuration is present; absent configuration remains fail-closed. Physical
restore is still not registered in the worker and remains an explicit
coordinator-controlled rehearsal seam. Capture now waits, within its bounded
command timeout, for the requested target-LSN WAL segment before writing the
immutable manifest. Restore hardens the extracted PostgreSQL data directory,
handles the existing private `postgresql.auto.conf`, requires a regular
`recovery.signal`, and preserves checksum/object validation and cleanup.

VERIFIED: the ignored `real_postgres_physical_pitr_capture_restore_and_cleanup_are_authoritative`
acceptance passed 1/1 without mocks against an isolated PostgreSQL 18 cluster
with WAL archiving and a disposable HTTPS MinIO server signed by a generated
CA. It used explicit PostgreSQL 18 `pg_basebackup`, `pg_ctl`, and `psql`
programs, uploaded/read back the real base/WAL/manifest artifacts, restored
the data directory, verified PostgreSQL recovery mode, removed object
artifacts, dropped the source database, and left no test service running.

QUEUED: this is local physical-PITR evidence only. Approved production
backup/PITR credentials and restore-target authorization, Generic LDAP/RFC
4533, on-premises AD DS/DirSync, UCS Provisioning Service, approved protobuf
descriptor provenance, physical/device/scale/provider evidence, and explicit
production approval remain unavailable. G0–G14 are not claimed 100%; no
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Coordinator continuation — serialized completion verification — 2026-07-30

VERIFIED: after the PITR changes and Clippy cleanup, `cargo fmt --all --check`,
workspace Clippy with warnings denied, the complete non-ignored workspace test
suite, and protobuf check/tests passed. Console Dioxus web, desktop,
host-mobile, aarch64-apple-ios, and aarch64-linux-android checks passed;
shared UI host/iOS/Android and the Android bridge target also passed. Folio
`bun run lint` reported zero errors and the four pre-existing n8n warnings;
`bunx tsc --noEmit` and `git diff --check` passed. Folio remained operational
on port 3004 (`/` 200, `/platform` 307, `/operations` 307).

QUEUED: the exact no-environment ignored workspace sweep exited 101 with 82
explicit fail-closed resource/certification guards, including disposable
SQLx URLs, Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS Provisioning
Service, backup/PITR credentials, approved protobuf provenance, and physical
device/scale/provider evidence. No unexpected assertion was observed. G0–G14
remain active and are not claimed at 100%; no directory activation,
command-owner change, Folio write retirement, or production cutover occurred.

## Coordinator correction — approved integration audit — 2026-07-30

VERIFIED: the current tree contains the authoritative `InvokeApprovedIntegration`
path: holding-scoped SQLx control-plane binding resolution, durable
idempotent invocation enqueueing, multi-tenant worker registration, projection
and reconciliation, secret-manager credential resolution, HTTPS-only provider
routes, and invoke/reconcile handling. The no-environment ignored failures
for this path are queued SQLx/provider certification prerequisites, not a
missing local implementation or a mock success path.

## Coordinator continuation — directory, retention, migration, and UI safety — 2026-07-30

IMPLEMENTED: native directory ingestion now fails closed when a non-empty page
does not carry an advanced committed cursor; AD terminal DirSync cookies are
retained, UCS empty provisioning responses are not treated as full-import
completion, and RFC 4533 full imports use bounded composite sync-cookie plus
LDAP-paged-results cursors with a durable terminal cookie. Directory-managed
RBAC writes now use the typed immutable identity/fact/outbox contract and
normalize opaque holding scopes without UUID-only assumptions. Role-chain
tail reads are transaction-locked, and import rollback failures preserve the
original and rollback errors.

IMPLEMENTED: platform outbox claims are filtered to the five platform aggregate
families and duplicate platform completion registration was removed. G13 work
claims and phase transitions are fenced to their authorized phases; transitions
cannot advance over unfinished work, and rollback rehearsal requires persisted
successful source-preserved/read-model-rebuilt evidence. PITR retention now
selects typed physical artifacts, inventories bounded exact prefixes, and
deletes only the tenant/chain/backup prefix. Projection and restore rollback
failures are logged explicitly instead of being silently discarded.

IMPLEMENTED: shared dialogs close on Escape with an autofocus close control,
and console navigation exposes tablist/tab/tabpanel semantics. Focused identity
(116/116), worker (355 passed, 3 ignored), UI (110/110), infrastructure/worker
compile, and console compile checks passed after these changes.

QUEUED: the exact ignored workspace sweep remains an exit-101 certification
queue with 82 explicit guards in the current no-environment run. Credentialed
Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS Provisioning Service,
real external 1M-user scale, approved protobuf descriptor provenance, mobile /
device / provider evidence, approved backup/PITR targets, and production
approval remain unavailable. G0–G14 are not claimed at 100%; no directory
activation, command-owner change, Folio write retirement, or production
cutover occurred.

## Coordinator correction — final local verification and dev-runtime handoff — 2026-07-30

VERIFIED: the final exact tree passed `cargo fmt --all --check`, workspace
Clippy with `-D warnings`, and the complete non-ignored workspace suite. The
exact ignored sweep exited 101 with 82 explicit fail-closed guards and no
unexpected assertion failure. Protobuf check/tests, G13 acceptance (15/15),
G14 local compatibility (11/11), G14 directory acceptance (12/12), console
Dioxus web/desktop/mobile, shared UI iOS/Android, and Android bridge target
checks passed. Folio `bun run lint` had zero errors and four pre-existing n8n
warnings; `bunx tsc --noEmit` and `git diff --check` passed.

RUNTIME: the required local Folio service remains on port 3004. Next's
Turbopack dev process repeatedly hit its CSS-worker timeout, so the process
was restarted with the equivalent webpack development server without changing
application code; `/` returns 200 and protected `/platform` and `/operations`
return 307.

QUEUED: the 82 guards remain real SQLx/resource/vendor prerequisites:
credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS
Provisioning Service, approved protobuf descriptor provenance, production
backup/PITR authorization, measured 1M-user/device/provider evidence, and
production approval. G0–G14 are not claimed at 100%; no directory activation,
command-owner change, Folio write retirement, or production cutover occurred.

## Coordinator final addendum — current-tree evidence — 2026-07-30

CURRENT TREE: integrated local ERPX work is verified on
`codex/erpx-parallel-completion`; the exact ignored sweep remains a recorded
exit-101 queue of 82 fail-closed external-resource guards. Local acceptance
and compile evidence does not substitute for the unavailable LDAP/AD/UCS,
vendor-scale, descriptor, backup/PITR, device/provider, or production
approval evidence. G0–G14 therefore remain open for certification only.
## Coordinator final EOF addendum — current-tree evidence — 2026-07-30

The integrated local ERPX tree is verified on `codex/erpx-parallel-completion`.
The exact ignored workspace sweep remains an exit-101 queue of 82 fail-closed
external-resource guards. Unavailable prerequisites are credentialed
Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS Provisioning Service,
measured 1M-user vendor-scale evidence, approved protobuf descriptor
provenance, production backup/PITR targets, device/provider evidence, and
explicit production approval. G0–G14 and vendor acceptance remain open for
certification; no activation, owner switch, Folio write retirement, or
production cutover occurred.
## Coordinator final verification addendum — current tree — 2026-07-30

PASS: serialized `cargo fmt --all --check`, workspace Clippy with warnings denied, and the complete non-ignored workspace test suite. Protobuf tests, G13 deployment/parallel acceptance, G14 local compatibility and directory acceptance, console Dioxus web/desktop/mobile plus wasm/iOS/Android, shared UI iOS/Android, and Android bridge target checks all passed. Folio `bun run lint` had zero errors and four pre-existing n8n warnings; `bunx tsc --noEmit`, the Folio bridge suite (6/6), and `git diff --check` passed. Folio remained healthy on port 3004 (`/` 200, `/platform` 307, `/operations` 307).

QUEUED: the exact ignored workspace sweep exited 101 with 82 explicit fail-closed guards because disposable SQLx URLs, Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS Provisioning Service, approved descriptor provenance, backup/PITR targets, scale/device/provider evidence, and production approval are unavailable. These are certification blockers, not claimed passes. G0–G14 and vendor acceptance remain open for certification; no directory activation, command-owner switch, Folio write retirement, or production cutover occurred.

## Coordinator final integration addendum — POS integrity and clean local acceptance — 2026-07-31

IMPLEMENTED: POS source persistence now rotates the aggregate command identity
with each accepted command and validates it against the latest immutable
command record. The POS source snapshot verifier accepts an authoritative
zero-payment sale and compares every persisted payment fact by cardinality and
identity. Fresh local POS SQLx persistence/projection acceptance passed 9/9,
including append-only payment facts, forged payload rejection, source-fenced
rebuild rollback, replay integrity, and watermark checks.

PASS: the current tree passed `cargo fmt --all --check`, workspace Clippy with
warnings denied, the complete non-ignored workspace tests, protobuf tests, G13
acceptance, G14 local compatibility/directory acceptance, all requested Dioxus
targets, Folio lint/typecheck/bridge tests, and port-3004 runtime probes.
Folio lint retains four pre-existing n8n warnings and zero errors.

PASS: isolated clean local ignored targets passed directory RBAC 4/4, platform
completion 2/2, quality scope integrity 1/1, control-plane SQLx 3/3, support
workflow 1/1, and Customer360 rebuild 1/1. The fresh single-threaded
aggregate ignored command exited 101 with eight explicit outcomes: five
shared disposable-database fixture collisions, credentialless backup/PITR
live-resource guards, and the expected credentialless G14 vendor preflight.
The five local collisions are not unresolved product failures because their
clean per-target SQLx runs passed; the aggregate log is
`/tmp/erpx_ignored_final_20260730_v2.log`.

QUEUED: genuine external certification remains unavailable for credentialed
Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS Provisioning Service,
TLS MinIO/physical PITR, approved protobuf descriptor provenance, measured
1M-user/vendor/device/provider evidence, and explicit production approval.
No directory activation, command-owner switch, Folio write retirement, or
production cutover occurred; G0–G14 and vendor acceptance are not claimed at
100% until those external gates are supplied and executed.

## Coordinator current-tree addendum — isolated local acceptance — 2026-07-31

The coordinator hardened disposable SQLx fixture isolation for quality,
directory RBAC, control-plane, support, Customer360, and G13 target-import
acceptance. Fresh isolated local runs passed directory RBAC 4/4, directory SQLx
3/3, identity SQLx 6/6, infrastructure ignored library 12/12, platform 2/2,
quality 1/1, control-plane 3/3, support workflow 1/1, support persistence 8/8,
Customer360 1/1, marketing scheduler 1/1, role-review notifications 1/1, G13
target import 1/1, and server owner-seed 1/1. Full non-ignored workspace,
formatter, warnings-denied Clippy, Dioxus targets, Folio lint/typecheck/bridge,
and port-3004 probes passed.

The ignored suite remains a matrix, not a single shared-database command:
locally runnable targets still require isolated per-target PostgreSQL URLs, and
credentialed backup/PITR plus Generic LDAP/RFC 4533, AD DS/DirSync, and UCS
Provisioning Service certification remain unavailable. Approved protobuf
descriptor provenance, measured one-million-user/vendor/device/provider
evidence, and explicit production approval also remain queued. The coordinator
created and then removed only the exact disposable 2026-07-31 databases used by
these checks. Folio remains operational on port 3004, with no activation,
owner-switch, Folio-write retirement, or production cutover.

## Coordinator current-tree addendum — local ignored matrix complete — 2026-07-31

All locally runnable ignored SQLx targets have now been executed with isolated
per-target PostgreSQL databases. The tenant lane passed 43 targets / 93 tests;
the infrastructure control lane passed 15 targets / 26 tests; special
infrastructure targets passed support persistence 8/8, marketing scheduler
1/1, role-review persistence 2/2, and the ignored infrastructure library
passed 12/12. Identity/server passed 14/14, while worker, provisioning,
readiness, and G13 passed 18 target invocations / 24 tests. Fresh databases
were cleaned up and pre-existing databases were preserved.

Only the deliberate external guards remain queued: backup live requires
credentialed TLS MinIO and backup PostgreSQL inputs; physical PITR requires a
WAL-archiving PostgreSQL cluster, restore tooling, TLS MinIO, and recovery
inputs; and G14 vendor preflight requires Generic LDAP/RFC 4533, AD DS/DirSync,
and UCS Provisioning Service environments with TLS, native-feed, import,
revocation, and scale evidence. No external endpoint was contacted. G0–G14
and vendor acceptance remain open for those external gates, with no directory
activation, command-owner switch, Folio-write retirement, or production
cutover.

## Coordinator current-tree integration — Folio bridge, scoped Operations, and platform lifecycle — 2026-07-31

The Folio bridge now has a durable transactional shadow outbox for Folio-owned
platform commands. An explicitly enabled drain publishes idempotent events to
the isolated ERPX control outbox without changing Folio command ownership.
Role lifecycle events are append-only and linked to assignment versions;
renewal revokes the previous version atomically. Bridge reconciliation is
fail-closed until counts, immutable identity digests, unlinked-target parity,
and source/target event high-watermarks all agree.

High-risk platform role changes are server-enforced with fresh signed,
actor/action-bound step-up MFA evidence and an independent approver. Operations
scope is resolved from isolated ERPX control-read authority, branch and
holding filters are enforced at both projected-read and legacy break-glass
boundaries, and the platform console exposes support approval/revocation and
role-management actions.

Current-tree verification passed formatting, warnings-denied workspace Clippy,
the complete non-ignored workspace/doc suite, protobuf/G13/G14 compatibility,
all console Dioxus web/desktop/mobile/iOS/Android checks, shared UI native
checks, Android bridge, Folio focused bridge/step-up/Operations tests, Folio
lint/typecheck, diff hygiene, and port-3004 probes. The exact aggregate
ignored command exited 101 with 82 unavailable-resource/certification targets
and is recorded as `/tmp/erpx-ignored-20260731.log`; isolated local SQLx lanes
remain the authoritative local evidence. External LDAP/AD/UCS, backup/PITR,
approved descriptor, measured-scale/device/provider, and production-approval
gates remain queued. No activation, owner switch, Folio-write retirement, or
production cutover occurred.

## Coordinator current-tree addendum — bridge aggregate identity hardening — 2026-07-31

The Folio bridge drain now preserves the source aggregate identifier when
publishing a shadow event into the ERPX control outbox; the bridge queue row
identifier remains the event identity. Bridge event type, aggregate type,
aggregate ID, and idempotency key inputs are bounded and reject control
characters before persistence. Folio remains the command owner and shadow
publishing remains explicitly opt-in.

Focused bridge reconciliation and shadow-identity tests (10/10), Folio lint, TypeScript checking,
diff hygiene, identity unit/integration contract tests (122 unit tests plus
directory contract suites), and port-3004 probes passed. External directory,
backup/PITR, scale, descriptor, device/provider, and production approval
gates remain queued; no activation or cutover occurred.

## Coordinator current-tree addendum — directory synchronization preflight — 2026-07-31

Directory synchronization now requires an explicit provider-native readiness
preflight before any LDAP, RFC 4533, AD DirSync, or UCS Provisioning Service
page/change ingestion; DN/filter/cursor inputs are bounded, and a revocation
breach may transition only after the strict five-minute deadline. Local
directory identity evidence covers 1M-user capacity, immutable links, native
cursors, append-only RBAC, and fail-closed revocation; credentialed vendor
certification and measured external scale/revocation remain queued.

## Coordinator current-tree addendum — directory/auth, Finance replay, Operations close, and UI session safety — 2026-07-31

Directory break-glass grants now retain authoritative issuance time and reject
future-dated grants at the authentication boundary; authenticated and durable
evidence carries that issuance fact without credentials. Finance journal
replays now require matching authoritative source rows, immutable payload and
version facts, and exactly one matching outbox event on both replay paths.
Operations now exposes an authoritative `Resolved → Closed` lifecycle action
through the authenticated, tenant-scoped gRPC transport. Dioxus secure-session
restore and refresh/revoke flows remain in `Restoring`/revoked states until
server revalidation and clear protected sessions on revocation.

Focused identity, Finance, Operations, UI, server-check, and bridge evidence
passed locally. External LDAP/AD/UCS, TLS MinIO/PITR, measured scale,
descriptor provenance, native-device/provider certification, and explicit
production approval remain queued; no activation, owner switch, Folio-write
retirement, or cutover occurred.

## Coordinator current-tree addendum — provisioning TLS enforcement — 2026-07-31

Platform provisioning now rejects PostgreSQL admin and tenant URLs unless the
single explicit transport mode is `sslmode=verify-full`, before SQLx connects
or any mutation can occur. Backup/provisioning focused checks passed locally;
credentialed MinIO, physical PITR, and disposable live-database certification
remain queued and no external resource was activated.

## Coordinator current-tree addendum — role-review worker safety — 2026-07-31

The role-review notification worker now uses deterministic run timestamps,
caps claims at 100, validates `ExpiringSoon` versus `Expired` source timing,
and releases claims safely when source responses are malformed, oversized, or
inconsistent. Focused worker evidence passed; live SQLx role-review acceptance
remains queued without its disposable database reference.

## Coordinator current-tree addendum — product, backup, directory-worker, and UI bounds — 2026-07-31

Marketing journey integration persistence now provides source-fenced,
transactional read-model rebuild with rollback preservation, a 1M-event bound,
advisory projection locking, and strict gap/duplicate/payload reconciliation.
Backup retention releases claims fail-closed after reconciliation/history/report
errors under retry/dead-letter limits; PITR requires registry-owned storage
namespaces. The directory worker rejects full-import progress beyond connector
capacity, including overflow and empty-page abuse. UI route/deep-link parsing
rejects inputs over 4,096 bytes before parsing, component action labels are
bounded and redacted, and revoked state discards protected payloads.

Focused local evidence passed where dependencies were available. External
provider/MinIO/PITR, SQLx fixtures, native-device, LDAP/AD/UCS, and production
approval gates remain queued; no activation or cutover occurred.

## Coordinator final local verification addendum — 2026-07-31

The current tree passes `cargo fmt --all --check`, warnings-denied workspace
Clippy, the complete non-ignored workspace test/doc suite, protobuf tests,
G13/G14 compatibility tests, all requested Dioxus console/UI Android/iOS
checks, Android bridge tests, Folio lint/typecheck, focused bridge/step-up/
Operations tests, diff hygiene, and port-3004 runtime probes. The two local
regressions found during verification were corrected: the worker debug fixture
now uses TLS-only PostgreSQL URLs, and the G14 console source-bound marker
tracks the actual hash deep-link dispatch. The full workspace suite was then
rerun successfully.

The exact required ignored invocation exited 101 with 84 targets failing
closed because disposable PostgreSQL URLs, credentialed Generic LDAP/RFC 4533,
AD DS/DirSync, UCS Provisioning Service, MinIO/PITR, approved descriptor,
device/provider, or production-approval evidence is unavailable. Its raw
result is `/tmp/erpx-ignored-20260731-final.log`; these failures are queued
certification blockers, not mocked or schema-only passes. G0–G14 and vendor
acceptance remain open for those external gates. No directory activation,
command-owner switch, Folio-write retirement, or production cutover occurred.

## Coordinator current-tree final verification correction — 2026-07-31

After the PITR verifier correction, serialized verification was rerun on the
current tree: formatting, warnings-denied workspace Clippy, the complete
non-ignored workspace/doc suite, protobuf/G13/G14 compatibility, all console
and shared-UI target checks, Folio lint/typecheck, bridge/step-up/Operations
tests, diff hygiene, and port-3004 probes passed. The real ignored backup and
physical-PITR targets were rerun after the final source change and each passed
with disposable PostgreSQL 18/TLS and HTTPS MinIO resources; those resources
were stopped and cleaned up afterward.

The exact no-input ignored workspace sweep was rerun and exited 101 with 86
environment-gated targets, recorded at `/tmp/erpx-ignored-20260731-postpitr.log`.
This is expected fail-closed behavior for the unconfigured aggregate; the
isolated SQLx matrix and the real backup/PITR targets provide the available
local database evidence. Credentialed Generic LDAP/RFC 4533, AD DS/DirSync,
UCS Provisioning Service, approved descriptor provenance, physical device
execution, measured external scale/revocation, and explicit production
approval remain queued. No activation, owner switch, Folio-write retirement,
or production cutover occurred.

## Coordinator current-tree correction — local backup and PITR certification — 2026-07-31

The earlier queued local backup/PITR statement is superseded for the available
local infrastructure. Against PostgreSQL 18 with `sslmode=verify-full`, a real
HTTPS MinIO instance, the PostgreSQL 18 client binaries, and disposable
databases, the live backup acceptance passed end to end: dump, upload,
inventory, candidate restore, RLS/tenant marker verification, routed activation,
failed rollback, and cleanup. The live physical PITR acceptance also passed:
base backup, WAL archive capture, HTTPS object integrity, isolated paused
recovery at or beyond the requested LSN, corrupted-artifact failure evidence,
and disposable candidate cleanup.

The PITR verifier accepts the provider's valid recovery behavior of replaying
within the target WAL segment by requiring paused recovery and
`pg_last_wal_replay_lsn() >= target_lsn`; it no longer relies on an exact
segment-boundary equality. Credentialed Generic LDAP/RFC 4533, AD DS/DirSync,
UCS Provisioning Service, external one-million-user/revocation measurement,
and final production-approval gates remain queued. No directory activation,
command-owner switch, Folio-write retirement, or production cutover occurred.

## Coordinator current-tree external readiness and disposable LDAP probe — 2026-07-31

The focused current-tree directory acceptance tests passed 12/12 and the
generic/vendor worker harnesses passed 20/20 in fail-closed mode, recording
the exact missing vendor endpoints, credentials, and database references.
No absent environment was presented as certification.

A disposable Colima `epsx-e2e` OpenLDAP 1.5.0 probe reached TLS startup, but
the image's generated CA is expired relative to the current date and its
bootstrap rewrite behavior does not work with the host bind mount. It was
removed without producing acceptance evidence. The earlier real local
OpenLDAP/LDAPS/RFC4533 run remains the authoritative Generic LDAP evidence.
Microsoft AD DS, UCS Provisioning Service licensing, external scale/revocation,
approved descriptor/device inputs, and explicit production approval remain
open; no activation or cutover occurred.

## Coordinator current certification-input audit — 2026-07-31

The resumed audit found no `ERPX_*`, LDAP, AD, UCS, backup, PITR, or tenant
database certification inputs in the environment. Local media inspection found
only the previously used UCS KVM image and overlays; no Microsoft Windows
Server/AD DS installation media or credentialed AD environment is present. The
UCS image remains license-gated for native Provisioning Service installation.
No license bypass, Samba compatibility result, or synthetic provider response
was promoted to vendor acceptance. The available local Generic LDAP/RFC4533,
SQLx matrix, backup/PITR acceptance, and fail-closed readiness tests remain
authoritative; unavailable vendor and approval gates stay queued.

## Coordinator resumed current-tree serialized verification — 2026-07-31

After the resumed audit, the current tree passed `cargo fmt --all --check`,
warnings-denied workspace Clippy, and `cargo test --workspace --offline
--no-fail-fast`. `bun run lint` passed with zero errors and the four
pre-existing n8n warnings; `bunx tsc --noEmit`, the G14 structural probe, and
port-3004 runtime probes also passed (`/` 200, protected routes 307, bridge
unauthenticated 401). No production Next.js build ran.

The full workspace output reports only deliberately ignored resource-backed
tests; those remain covered by the isolated matrix or queued fail-closed
external prerequisites. No vendor acceptance, directory activation, command-
owner switch, Folio-write retirement, or production cutover was claimed.

## Coordinator isolated ignored SQLx acceptance expansion — 2026-07-31

Isolated TLS PostgreSQL runs closed 70 locally runnable ignored assertions:
local G13/G14 directory SQLx 3/3, identity persistence 6/6, directory RBAC
scope authority 4/4, G13 migration persistence 1/1, G13 disposable migration
1/1, G13 target import 1/1, infrastructure library commercial/finance/support
persistence 12/12, and eight disjoint tenant-context lanes totaling 42/42
across Assets/Maintenance, Finance, HR, Legal, Operations, POS, Quality, and
SCM. These runs exercised real migrations, forced-RLS write/read separation,
outbox/projector state, idempotency, scope fences, reconciliation, and rollback
behavior. Every coordinator-created database was removed; no pre-existing
database was touched.

The exact no-input ignored command was rerun with `pipefail` and exited 101 at
86 explicit missing-resource/vendor guards. No unexpected application
assertion failure was observed and no local result was promoted to Microsoft
AD DS/UCS/vendor acceptance.

## Coordinator second isolated ignored SQLx wave — 2026-07-31

PASS: eight disjoint fresh TLS PostgreSQL lanes passed 32/32 ignored
assertions: Customer360 detail reads 8/8, Marketing CRM persistence 1/1,
Marketing delivery persistence 5/5, Marketing journey execution persistence
6/6, Support persistence 8/8, tenant projection scheduler 1/1, Finance
banking persistence 2/2, and HR payroll projection 1/1. These runs exercised
real migrations, forced-RLS write/read separation, outbox/projector state,
idempotency, scope/reconciliation, and rollback behavior. Every
coordinator-created database was removed; no pre-existing database was
touched.

## Coordinator resumed current-tree final verification — 2026-07-31

PASS: `cargo fmt --all --check`, workspace Clippy with `-D warnings`, the
complete non-ignored workspace suite, `cargo test -p erpx-proto --offline`,
G13 parallel/recovery (15/15 and 6/6), G14 local compatibility (13/13), G14
directory acceptance (12/12), all console Dioxus web/desktop/host-mobile/
iOS/Android checks, shared UI host/iOS/Android checks, and the Android bridge
target check. Folio `bun run lint` returned zero errors with four pre-existing
`infra/scripts/start-n8n.js` warnings; `bunx tsc --noEmit`, `git diff --check`,
and the port-3004 probes passed (`/` 200, `/platform` 307, `/operations` 307,
and the unauthenticated bridge 401). No production Next.js build ran.

QUEUED, FAIL-CLOSED: the mandated no-input ignored aggregate remains exit 101
with 86 explicit missing-resource/vendor guards; the isolated SQLx waves now
cover 102/102 locally runnable ignored assertions. Remaining closure gates
are credentialed/on-prem AD DS with native DirSync, licensed UCS Provisioning
Service with native feed evidence, measured external one-million-user and
five-minute vendor revocation evidence, approved protobuf descriptor
provenance, and explicit production approval. No directory activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Coordinator expanded locally runnable ignored acceptance — 2026-07-31

PASS: additional isolated TLS PostgreSQL lanes closed 56/56 assertions,
bringing the locally runnable ignored total to 158/158. The wave covered
Customer/worker read and projector paths, Operations authoritative and routed
projection paths, Finance close/budget/chart/subledger/consolidation/Thai-tax
and candidate activation paths, Legal projection/scope paths, Marketing CRM/
content/journey/delivery/provider/erasure/abuse paths, Quality and
organization paths, Platform provisioning/readiness/scheduler/status paths,
backup job/retention/blocker/reconciliation paths, server owner-seed
transaction, and worker commercial handlers. All coordinator-created
databases were removed; pre-existing databases were not touched.

The first three harness attempts in this expansion were discarded for
misbound admin/template input or a database-name collision; corrected reruns
passed and no application assertion failure remained. The mandated no-input
ignored aggregate is still exit 101 with 86 explicit missing-resource/vendor
guards. No local result was promoted to AD DS/UCS/vendor acceptance.

## Coordinator current G14 external-readiness recheck — 2026-07-31

PASS: the current tree's Generic LDAP live harness passed 10/10, including
one explicit queued live-transport case and the RFC 4533, TLS, immutable-link,
one-million-user, RBAC/revocation, and evidence-boundary checks. The AD/UCS
vendor harness passed 10/10 with both live vendor cases explicitly queued;
the external-readiness suite passed 10/10 with one intentionally ignored
credentialed preflight. All missing references were reported by name without
secret values, and no provider socket was contacted.

QUEUED, FAIL-CLOSED: Generic LDAP, on-premises AD DS/DirSync, and UCS
Provisioning Service still lack the required credentialed endpoints,
certificates, native-feed evidence, and measured provider-side revocation
action. No local harness result was promoted to vendor acceptance.

## Coordinator current-tree verification after G14 recheck — 2026-07-31

PASS: after the current G14 readiness recheck, `cargo fmt --all --check`,
workspace Clippy with `-D warnings`, and the complete non-ignored workspace
suite passed. Folio `bun run lint` returned zero errors with four pre-existing
`infra/scripts/start-n8n.js` warnings; `bunx tsc --noEmit`, `git diff --check`,
and the port-3004 probes passed (`/` 200, `/platform` 307, `/operations` 307).
No production Next.js build ran, and no vendor socket, directory activation,
command-owner switch, Folio-write retirement, or production cutover occurred.

## Coordinator third consecutive external-state audit — 2026-07-31

QUEUED, FAIL-CLOSED: the external state is unchanged after the third
consecutive goal audit. No AD DS/UCS credentials, endpoints, Windows Server
media, or running vendor environments are available; the local UCS image
remains license-gated. The native transports, worker registration, local
Generic LDAP/RFC 4533 evidence, SQLx persistence/projectors, and required
serialized verification remain green. No synthetic vendor response,
Samba-compatibility result, or local readiness result is promoted to AD DS or
UCS acceptance.
