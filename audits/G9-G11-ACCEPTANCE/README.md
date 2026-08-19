# G9–G11 acceptance evidence audit

**Audit date:** 2026-07-27  
**Scope:** bounded, read-only acceptance audit of the current product-owned G9–G11 slices.  
**Ownership:** this audit and its owned migrator verifier do not replace or modify
[`../G9-G11-COMPLETION/README.md`](../G9-G11-COMPLETION/README.md), product source,
manifests, protobuf generation, migrations, registrations, `MASTER_PLAN.md`, or
`EXECUTION_LEDGER.md`.

## Verdict

**G9–G11 are not acceptance-complete.** The current tree now contains code-level
transport, worker-composition, and console read-surface evidence for the previously
missing Sales, SCM/Procurement–Inventory, and Asset/Maintenance seams. That evidence
is not live SQLx, vendor, device, rollback, or production acceptance evidence.

No disposable holding database pair, product-vendor credential, payment/acquirer
device harness, or product-specific external endpoint was configured for this audit.
The live PostgreSQL/reconciliation and vendor/device gates therefore remain queued.
No production database, customer directory, external vendor, Folio write path,
directory activation, or cutover command was changed or activated.

## Evidence run

Commands run from `erpx/`:

| Check | Result |
|---|---|
| `cargo test -p erpx-migrator --test g11_product_context_acceptance -- --nocapture` | **PASS** — 7 passed, 0 failed; static transport, server, worker-registration, console read-surface, Folio-bridge, and blocker-reporting checks passed; no live claims were made |
| `cargo test -p erpx-migrator --test g11_sales_transport_acceptance -- --nocapture` | **PASS** — 3 passed, 0 failed; Sales source/read safety, protobuf/server save/get parity, authorization and write/read routing, registration, Folio bridge, and explicit blocker-reporting checks passed; no live claims were made |
| `cargo test -p erpx-migrator --test g11_asset_maintenance_acceptance -- --nocapture` | **PASS** — 4 passed, 0 failed; authoritative source/read reconciliation, forged-event rejection, coordinator transport, worker registration, and explicit live-DB blocker reporting passed |
| `rustfmt --edition 2021 --check apps/migrator/tests/g11_asset_maintenance_acceptance.rs` | **PASS** |
| `cargo fmt --all --check` | **PASS** |
| `ERPX_TEST_HOLDING_WRITE_DATABASE_URL` | **unset** |
| `ERPX_TEST_HOLDING_READ_DATABASE_URL` | **unset** |
| `ERPX_TEST_HOLDING_DATABASE_URL_TEMPLATE` | **unset** |
| `ERPX_TEST_ADMIN_DATABASE_URL` | **unset** |
| Product vendor/device references | **unavailable** — no live vendor or device certification environment was supplied |

The focused verifier intentionally reports code seams separately from live
acceptance. Its optional local package-suite runner remains opt-in through
`ERPX_G11_RUN_LOCAL_SUITES=1`; the focused run does not promote static source
presence or local unit coverage to live SQLx acceptance.

## Acceptance matrix

Status meanings: **code-partial** means authoritative domain/SQLx/transport,
worker, and/or read-surface code exists; **blocked-live** means required live
database, vendor, device, or rollback evidence could not run; **open** means a
required acceptance artifact is still absent.

| Gate/context | Code evidence | Live SQL/reconciliation evidence | Vendor/device and rollback evidence | Status |
|---|---|---|---|---|
| G9 Sales | `apps/migrator/tests/g11_sales_transport_acceptance.rs` records 3 passing focused checks for Sales source/read safety, protobuf/server save/get parity, authorization and write/read routing, worker/server registration, and the Folio bridge. `crates/domain/src/sales.rs` and `crates/infrastructure/src/sales_persistence.rs` provide scoped lifecycle facts, source/outbox persistence, read-event paging, projection/checkpoints, mismatch reporting, and disposable clearing. `crates/proto/proto/erpx/platform/v1/platform.proto` contains `SalesService` and typed save/get messages; `apps/server/src/main.rs` contains `SalesRpcImpl` and server registration; `apps/worker/src/main.rs` contains `ProductProjectionExecutor::Sales` and the `sales` scheduler branch; `apps/console/src/product_completion.rs` contains the read-only Sales context surface contract. | No disposable write/read database pair or live Sales write/project/reconcile/rebuild run was supplied. The packet explicitly queues missing `ERPX_TEST_HOLDING_WRITE_DATABASE_URL`, `ERPX_TEST_HOLDING_READ_DATABASE_URL`, and `ERPX_TEST_HOLDING_DATABASE_URL_TEMPLATE` references. | `ERPX_G11_SALES_VENDOR_ENDPOINT` is not configured; no Sales vendor certification packet exists. `clear_projection` and rebuild are code seams only; no executed rollback/replay evidence. | **code-partial / blocked-live** |
| G9 SCM / Procurement & Inventory | `crates/domain/src/procurement_inventory.rs` and `crates/infrastructure/src/scm_persistence.rs` provide scoped aggregates, source outbox, reads, receipts/checkpoints, reconciliation, and clearing. `platform.proto` now contains `ScmService` and typed save/get messages; `apps/server/src/main.rs` contains `ScmRpcImpl` and registration. `apps/worker/src/main.rs` contains `ProductProjectionExecutor::Scm` and the `scm` scheduler branch. `product_completion.rs` contains read-only Procurement and Inventory context surfaces. | `crates/infrastructure/tests/scm_persistence.rs::live_scm_write_outbox_projection_scoped_read_and_reconciliation` is ignored and requires disposable holding databases. It was not run because the required URLs were unset. | No live supplier/ERP vendor endpoint or certification packet. The ignored source contains clear/replay steps, but no executed rollback evidence. | **code-partial / blocked-live** |
| G9 POS | `crates/domain/src/pos.rs`, `pos_persistence.rs`, and `pos_projection.rs` provide normalized sale persistence/projection, receipts, checkpoints, scoped reads, reconciliation, and clearing. `PosService` and typed save/list RPCs exist. `apps/worker/src/main.rs` contains `ProductProjectionExecutor::Pos` and the `pos` scheduler branch. The console read-only product surface includes Point of Sale. | `pos_persistence::live_pos_write_is_normalized_scoped_and_idempotent` and `pos_projection_read::live_pos_read_projection_is_scope_bound_bounded_and_cursor_stable` are ignored and were not run; no live reconciliation/rebuild result exists. | No live acquirer/payment-provider or POS-device certification. Projection clear/replay is not live evidence. | **code-partial / blocked-live** |
| G10 Marketing / CRM | Marketing/CRM domain and SQLx persistence provide source writes, outbox, read projection, receipts/checkpoints, reconciliation, and clearing. Existing CRM/Marketing RPCs and read-only console surfaces remain present. `apps/worker/src/main.rs` includes the direct `marketing` projection branch in addition to auxiliary delivery/journey/content/privacy handlers. | Marketing CRM, read projection, provider-flow, onboarding, and erasure tests are ignored and require disposable holding/control databases; no live pair was supplied. | Existing provider-flow material uses test boundaries and synthetic callback/provider values; it is not live vendor certification. No live rollback/rebuild rehearsal. | **code-partial / blocked-live** |
| G11 Asset / Maintenance | `crates/domain/src/asset_maintenance.rs` and `crates/infrastructure/src/asset_maintenance_persistence.rs` provide source state, outbox, read projection, reconciliation, and disposable clearing. `platform.proto` now contains `AssetMaintenanceService` and typed save/get messages; `apps/server/src/main.rs` contains `AssetMaintenanceRpcImpl` and registration. `apps/worker/src/main.rs` contains `ProductProjectionExecutor::AssetMaintenance` and the `asset_maintenance` scheduler branch. `product_completion.rs` contains the read-only Maintenance context surface contract. `apps/migrator/tests/g11_asset_maintenance_acceptance.rs` now provides the focused source/read, forged-event, transport, worker, and blocker verifier; its run passed 4/4. | The focused verifier explicitly reports `ERPX_TEST_HOLDING_WRITE_DATABASE_URL`, `ERPX_TEST_HOLDING_READ_DATABASE_URL`, `ERPX_TEST_HOLDING_DATABASE_URL_TEMPLATE`, and `ERPX_TEST_ADMIN_DATABASE_URL` as unset. No live Asset/Maintenance PostgreSQL write/read/project/reconcile/rebuild result is claimed. | No maintenance/asset vendor, fleet, machine, or device certification packet. No live rollback/replay evidence. | **code-partial / blocked-live** |
| G11 HR / Payroll | `crates/domain/src/hr.rs`, `hr_persistence.rs`, and `hr_projection.rs` provide source/read separation, outbox projection, receipts/checkpoints, reconciliation, and clearing. `HrService` save/list RPCs exist. `apps/worker/src/main.rs` contains the `hr` scheduler branch and the registered HR completion handler. The console product read surface includes HR/Payroll. | `hr_payroll_projection::live_hr_write_projection_and_read_are_idempotent_and_separate` is ignored and requires two disposable holding databases; it was not run. | No live payroll-provider certification or rollback/replay rehearsal. | **code-partial / blocked-live** |
| G11 Legal / Documents | `crates/domain/src/legal.rs`, `legal_persistence.rs`, and `legal_projection.rs` provide scoped source state, outbox/read projection, retention, reconciliation, receipts/checkpoints, and clearing. `LegalService` save/record/list RPCs exist. `apps/worker/src/main.rs` contains the `legal_documents` scheduler branch. The console product read surface includes Legal. | Ignored Legal write, retention, and projection tests require a disposable database pair; no live execution was available. | No live object-storage/MinIO or document-provider acceptance. Rebuild/clear remains unexecuted rollback evidence. | **code-partial / blocked-live** |

## Console and directory read-surface evidence

The console now has two distinct code-level read surfaces:

- `apps/console/src/main.rs` renders `DirectoryAdministrationRouteSurface` and
  fetches directory connector and role-mapping summaries through the typed
  `DirectoryService` read RPCs.
- `apps/console/src/product_completion.rs` defines the typed, read-only
  `DirectoryAdministrationSurface` and bounded product context surfaces for
  Sales, Procurement, Inventory, Maintenance, HR/Payroll, POS, Legal, and
  Marketing/CRM. They expose loading/empty/denied/stale/error/read-only states
  without mutation callbacks, secrets, credentials, or provider endpoints.

These are console code and transport-contract observations. They do not prove a
browser/device acceptance run, populated customer read model, live authorization
decision, or live product projection. The Folio bridge remains a read-only
runtime boundary and does not authorize directory activation or production cutover.

## Exact open evidence

### Live SQL and reconciliation

The following ignored acceptance targets remain unexecuted because the required
database references were unset:

- `scm_persistence::live_scm_write_outbox_projection_scoped_read_and_reconciliation`
- `pos_persistence::live_pos_write_is_normalized_scoped_and_idempotent`
- `pos_projection_read::live_pos_read_projection_is_scope_bound_bounded_and_cursor_stable`
- `hr_payroll_projection::live_hr_write_projection_and_read_are_idempotent_and_separate`
- `legal_persistence::live_legal_write_is_normalized_scoped_and_idempotent`
- `legal_persistence::live_legal_retention_sweep_expires_due_contract_once`
- `legal_projection_read::live_legal_projection_is_typed_scoped_idempotent_and_rebuildable`
- `marketing_crm_persistence::live_marketing_crm_write_boundary_is_atomic_idempotent_and_scoped`
- `marketing_crm_read_projection::crm_events_are_allow_listed_idempotent_rebuildable_and_rls_scoped`
- `marketing_delivery_provider_flow::queued_delivery_provider_acceptance_callback_and_projection_state_are_atomic`
- `marketing_provider_onboarding::provider_onboarding_is_idempotent_verified_before_activation_and_audited`
- `marketing_profile_erasure::profile_erasure_redacts_source_and_rebuildable_read_state`

Sales and Asset/Maintenance now have transport and worker code seams, but no
equivalent executed live infrastructure acceptance scenario was supplied.
Local domain/SQLx/worker tests and static source checks remain implementation
evidence, not persisted write/read/reconcile/rebuild evidence.

### Worker composition

The executable composition root now covers the product projection kinds
`sales`, `scm`, `asset_maintenance`, `legal_documents`, `pos`, and `hr`, plus
the direct `marketing` projection branch. The focused verifier checks the
projection kinds plus executor/runner registration markers. Scheduler/claim/retry behavior,
holding routing, idempotency under restart, and dead-letter handling still need
runtime evidence against disposable databases.

### Vendor and device evidence

No live product vendor credentials, provider endpoints, or device harnesses were
available. The queued certification scope includes Sales integrations, SCM
supplier/ERP endpoints, POS acquirer/payment and device behavior, Marketing
providers, payroll providers, maintenance/asset systems or devices, and Legal
document storage. Test endpoints, opaque typed references, synthetic provider
results, and synthetic callbacks are not vendor/device acceptance.

### Rollback evidence

The product stores expose disposable projection clearing and the worker exposes
rebuild/replay composition. That is code evidence only. No executed rollback or
replay report proves authoritative source preservation, append-only event history,
read-model rebuild, mismatch recovery, or approval boundaries for the product
contexts. No evidence authorizes source-row deletion, production cutover, or
Folio-write retirement.

## Re-acceptance packet

To close this audit, provide without mocks:

1. disposable, physically distinct holding write/read PostgreSQL URLs (and
   control write/read URLs where provider onboarding requires them);
2. live executions of every ignored target above, plus equivalent Sales and
   Asset/Maintenance scenarios, proving atomic source/outbox persistence, scoped
   reads, idempotent projection, mismatch detection, rebuild, and rollback;
3. runtime scheduler/claim/retry/restart evidence for each registered product
   projector;
4. browser/device acceptance of the typed server and console read surfaces,
   including authorization, stale/offline/error states, and redaction;
5. real vendor/device certification packets with secrets excluded from logs and
   artifacts; and
6. a rollback report showing source preservation, append-only event history,
   disposable read rebuild, and explicit approval before any production cutover.

Until those artifacts exist, this README remains an open acceptance audit and
does not change the status recorded by the existing completion handoff.

## Coordinator revalidation — current tree — 2026-07-27

The unset-environment evidence above predates the coordinator’s corrected
fresh disposable-database run. The serial ignored workspace suite exercised
the available G9–G11 SQLx targets, including Sales, SCM/Procurement–Inventory,
POS, HR, Legal, Marketing/CRM, Asset/Maintenance, Operations projection, and
Finance candidate routing. Source/outbox writes, scoped read models,
projection replay, mismatch detection, reconciliation, and rebuild/rollback
assertions passed for the targets that expose those scenarios. Unique
disposable databases were removed afterward.

This closes the local database-execution gap recorded above; it does not close
the G9–G11 gates. Vendor/device certification, browser/device role journeys,
broader accounting and Operations reconciliation, and product-specific
production rollback evidence remain open. No production or Folio write state
was changed.

## Coordinator revalidation — disjoint G9–G11 SQLx packet — 2026-07-28

The coordinator reran the following ignored acceptance targets in parallel,
each with its own disposable holding write/read PostgreSQL pair, then removed
the pair with an explicit cleanup trap:

- Sales persistence: 2/2 passed;
- SCM/Procurement–Inventory persistence: 5/5 passed;
- HR/payroll projection and envelope: 4/4 passed;
- Legal projection and persistence: 4/4 passed;
- POS persistence and read projection: 5/5 passed;
- Marketing CRM persistence/read projection and delivery callback state: 5/5
  passed.

These are real SQLx source/outbox, scoped read, checkpoint/rebuild,
idempotency, mismatch, rollback, and retention assertions where each target
defines them. They do not certify external vendors, device journeys, full
end-to-end accounting/Operations reconciliation, or production cutover. No
source rows were deleted and no Folio or ERPX production owner changed.

## Coordinator continuation — G9 Sales lifecycle transition — 2026-07-28

The Sales transport now has a typed `TransitionSalesAggregate` command. It is
explicitly holding/holding scoped, role-authorized with `sales.write`, bound to
the caller's expected source version, and limited to domain lifecycle actions.
The writer loads authoritative write state, invokes the domain transition, and
persists the source/snapshot/shared outbox/idempotency receipt through the
existing atomic SQLx path. The console exposes the transition only after an
authorized read and refreshes from the read model on success.

The fresh disposable Sales acceptance passed with the transition plus
projection, reconciliation, clear-and-replay, and scope-boundary assertions.
This is local G9 workflow evidence only; full Lead-to-Cash breadth,
Operations/accounting reconciliation, worker restart, vendor/device, and
production evidence remain queued.

## Coordinator continuation — G9 SCM procurement lifecycle transition — 2026-07-28

`ScmService.TransitionScmAggregate` now carries the safe Procurement lifecycle
boundary through authenticated `scm.write` authorization, explicit scope,
optimistic source versions, typed domain transitions, atomic source/outbox
persistence, and read-model refresh. Purchase Request/Order/Goods Receipt
transitions rotate command identity so lifecycle writes cannot reuse creation
idempotency keys. Receipt posting and stock mutations remain relationship-aware
domain operations.

The fresh disposable SCM SQLx acceptance passed Purchase Request
submit/approve, source/outbox projection, reconciliation, clear-and-replay,
and scope-separated read evidence. The broader G9 gate remains open for full
Procure-to-Pay, inventory/accounting/Operations reconciliation,
worker/vendor/device, and production acceptance.

## Coordinator continuation — G11 HR employee lifecycle transition — 2026-07-28

`HrService.TransitionHrEmployee` now carries the EmployeeRecord lifecycle
through authenticated `hr.write` authorization, explicit company/branch
containment, optimistic source versions, typed domain validation, audit, and
atomic holding-write source/outbox/idempotency persistence. The HR console
surface sends only bounded employee identity/status inputs and reloads the
authorized read projection after a receipt.

A fresh disposable PostgreSQL acceptance passed Active → Suspended, source
status/version, two-event outbox, and stale-version rejection. Broader HR and
payroll workflow, reconciliation/restart, device/vendor, and production
acceptance remain open; no activation or cutover occurred.

## Coordinator continuation — G9 SCM bounded list read — 2026-07-28

Procurement and Inventory now expose a bounded, read-only SCM aggregate list
over the existing holding read projection. `SqlxScmStore::list_for_scope`
uses the separate read pool, explicit holding/company/optional-branch scope,
checkpoint identity/version validation, typed aggregate decoding, reference
validation, and keyset pagination over projected timestamp, aggregate type, and
aggregate UUID. The `ListScmAggregates` RPC adds authenticated `scm.read`
authorization, product entitlement and caller-scope containment, customer
scope, holding read routing, opaque cursor bounds, audit publication, and safe
typed row mapping. The Dioxus Procurement and Inventory surfaces submit an
explicit scope and render only validated projection facts with pagination and
read-only/error states.

A fresh disposable two-database SCM acceptance passed 6/6, including source
outbox projection, scoped list rows, scope isolation, cursor continuity,
reconciliation, and clear/replay rollback assertions; both exact databases
were dropped afterward. The G11 product verifier passed 8/8, the console
wire-boundary suite passed 108/108, and server/infrastructure checks passed.
This closes the bounded SCM list-read implementation slice but not the full G9
gate: broader Procure-to-Pay and inventory/accounting/Operations reconciliation,
worker restart, vendor/device certification, browser/device evidence, and
production rollback/cutover approval remain open. No production state, Folio
write ownership, directory activation, or cutover changed.

## Coordinator continuation — G11 Asset/Maintenance bounded list read — 2026-07-28

Asset and Maintenance now expose a bounded read list over the existing
holding-read projection. `SqlxAssetMaintenanceStore::list_for_scope` validates
the separate read-side checkpoint, aggregate identity/version, typed company
and branch columns, generated reference, and asset custody-history projection;
it keyset-paginates by projected timestamp, allowlisted aggregate type, and
aggregate UUID. The typed RPC enforces explicit product scope containment,
`asset_maintenance.read`, customer scope, holding read routing, bounded opaque
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

## Coordinator current-tree verification — G9 SCM list read — 2026-07-28

PASS: serialized `cargo fmt --all --check`, workspace Clippy with warnings
denied, and the complete non-ignored workspace test suite. The SCM live
two-database acceptance passed 6/6; protobuf package tests passed; G13
compatibility/deployment checks passed 9/9 and 4/4; G14 local compatibility
passed 5/5. Console web, desktop, host-mobile, iOS, Android, and shared UI
iOS/Android target checks passed. Folio `bun run lint` and `bunx tsc --noEmit`
passed, with the four pre-existing n8n lint warnings. Folio remains healthy on
port 3004 (`/` 200 and `/operations` 307); no production Next.js build ran.

QUEUED: the exact ignored workspace sweep exited 101 with 71 environment-
gated targets. Missing disposable SQLx/MinIO/PITR resources, credentialed
Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS Provisioning Service,
operator-revocation evidence, approved protobuf descriptor baseline, physical
mobile execution, and explicit production migration/cutover approval remain
open. G0–G14 are not claimed complete; no activation, command-owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator focused verification after Asset/Maintenance — 2026-07-28

PASS: post-Asset workspace tests, warnings-denied workspace Clippy, protobuf
tests, G13 compatibility/deployment 9/9 and 4/4, G14 local compatibility 5/5,
all console/shared-UI Dioxus target checks, and the 109-test console suite.
Folio lint/typecheck remain green with four pre-existing n8n warnings; port
3004 remains healthy. The exact ignored sweep inventory is unchanged at 71
environment-gated targets. G0–G14 remain open for supplied external/vendor,
scale, device, rollback, and explicit production approval evidence.
