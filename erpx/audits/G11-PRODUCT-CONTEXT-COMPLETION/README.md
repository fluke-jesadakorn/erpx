# G11 product-context completion verifier

**Audit date:** 2026-07-27  
**Scope:** Sales, Procurement/Inventory, Asset/Maintenance, HR/Payroll,
Legal/Documents, POS, and Marketing/CRM.  
**Ownership:** this packet owns only this README and
`apps/migrator/tests/g11_product_context_acceptance.rs`.

The verifier reads the existing authoritative product source and runs the
local domain, SQLx-library, and worker-library suites.  It does not modify
product source, manifests, protobuf, migrations, registrations, shared
authorization, console composition, `MASTER_PLAN.md`, or
`EXECUTION_LEDGER.md`.

## Verdict

G11 is not acceptance-complete.  The seven product contexts have domain
contracts, holding/scope validation, write-side SQLx stores, append-only
outbox/idempotency seams, disposable read projections, reconciliation, and
rebuild/clear paths.  The verifier also sees typed API/server/UI evidence for
HR/Payroll, Legal/Documents, POS, and Marketing/CRM.

The following gates remain open or blocked and are deliberately reported by
the executable verifier rather than inferred as complete:

- The current tree exposes typed API/server authorization handoffs and
  read-only console routes for all seven contexts, and the worker composition
  root registers all seven product projection handlers. This is local contract
  evidence, not live workflow acceptance.
- No disposable, physically distinct tenant write/read PostgreSQL pair was
  supplied, so live atomic write/outbox, routed scope, projection,
  reconciliation, mismatch, rebuild, and rollback evidence is not claimed.
- No live product-vendor acceptance was supplied for suppliers, payment
  providers, payroll, document storage, or Marketing/CRM providers.
- Folio is verified only as a guarded, read-only control-plane bridge.  This
  packet performs no command-owner switch, customer-directory activation,
  Folio write retirement, or production cutover.

## Acceptance matrix

| Context | Authoritative implementation seams | Transport/UI observation | Runtime status |
|---|---|---|---|
| Sales | `domain/sales.rs`, `infrastructure/sales_persistence.rs`, `SalesService`, `/sales` read surface, and the bounded `SalesProjectionRunner` | Typed protobuf/server authorization/read routing and read-only console aggregate surface are present; aggregate payload bytes are discarded | Local contract evidence; live SQLx/reconciliation/rollback and vendor proof remain open |
| Procurement/Inventory | `domain/procurement_inventory.rs`, `infrastructure/scm_persistence.rs`, `ScmService`, `/procurement` and `/inventory` read surfaces, and `scm_projection.rs` | Typed protobuf/server authorization/read routing and read-only console aggregate surfaces are present | Local contract evidence; live SQLx/reconciliation/rollback and vendor proof remain open |
| Asset/Maintenance | `domain/asset_maintenance.rs`, `infrastructure/asset_maintenance_persistence.rs`, `AssetMaintenanceService`, `/maintenance` read surface, and the bounded projector | Typed protobuf/server authorization/read routing and read-only console aggregate surface are present | Local contract evidence; live SQLx/reconciliation/rollback and vendor proof remain open |
| HR/Payroll | `domain/hr.rs`, `hr_persistence.rs`, `hr_projection.rs`, typed HR service/server paths, and `/hr` read surface | Typed HR employee read route validates company/branch scope and lifecycle state; payroll values remain redacted | Local contract evidence; live SQLx/payroll-provider and rollback proof remain open |
| Legal/Documents | `domain/legal.rs`, `legal_persistence.rs`, `legal_projection.rs`, Legal service/server paths, and `/legal` read surface | Typed Legal contract metadata route validates retention/party scope and discards object bytes and identities | Local contract evidence; live SQLx/storage/vendor and rollback proof remain open |
| POS | `domain/pos.rs`, `pos_persistence.rs`, `pos_projection.rs`, POS service/server paths, and `/pos` read surface | Typed POS sale page validates company/branch/status/truncation and discards customer/register/payment payloads | Local contract evidence; live SQLx/payment-provider and rollback proof remain open |
| Marketing/CRM | `domain/marketing.rs`, `domain/marketing_crm.rs`, marketing persistence/projector, CRM/Marketing services, and read-only CRM UI handoff | Typed CRM/Marketing services and read-only CRM surface exist; delivery commands remain server-owned | Local contract evidence; live SQLx/provider and rollback proof remain open |

The matrix distinguishes implementation seams from acceptance evidence.  A
source marker, local unit test, or unexecuted ignored test cannot substitute
for a real routed database pair, external vendor, or registered worker run.

## Executable verifier

Run from the ERPX workspace:

```text
cargo test --offline -p erpx-migrator --test g11_product_context_acceptance -- --nocapture
```

Set `ERPX_G11_RUN_LOCAL_SUITES=1` when the shared Cargo target directory is
quiet and the package-library executions are desired:

```text
ERPX_G11_RUN_LOCAL_SUITES=1 cargo test --offline -p erpx-migrator --test g11_product_context_acceptance -- --nocapture
```

The verifier:

1. fails if any context loses its domain validation/scope/idempotency and
   reconciliation contract;
2. fails if any context loses its SQLx write/read separation, outbox,
   idempotency, source-event paging, projection, reconciliation, or clear
   path;
3. fails if any context loses its bounded worker projector/rebuild seam;
4. reports the current API, server authorization/scope, UI, and worker
   registration state without treating missing surfaces as successful;
5. optionally executes the local `erpx-domain`, `erpx-infrastructure`, and
   `erpx-worker` library suites when `ERPX_G11_RUN_LOCAL_SUITES=1` is set; and
6. reports missing disposable database references, vendor references, and
   Folio port `3004` reachability without leaking credentials or claiming live
   acceptance.

The test intentionally does not run ignored PostgreSQL tests by inventing
URLs.  When a disposable write/read pair is provisioned, the coordinator must
run the existing ignored product tests plus equivalent Sales and
Asset/Maintenance scenarios covering source write, outbox commit, idempotent
projection, scope denial, mismatch detection, clear-and-replay rollback, and
source-row preservation.

## Verification recorded for this packet

- `rustfmt --edition 2024 --check apps/migrator/tests/g11_product_context_acceptance.rs` — passed.
- `cargo test --offline -p erpx-migrator --test g11_product_context_acceptance -- --nocapture` — **7 passed**. The run reports all seven worker registrations and all seven typed API/server/console handoffs present; it queues the missing disposable database/vendor references and live Folio bridge ownership evidence without claiming live acceptance.
- `ERPX_G11_RUN_LOCAL_SUITES=1 cargo test --offline -p erpx-migrator --test g11_product_context_acceptance -- --nocapture` — **7 passed**. The nested domain, infrastructure, and worker library suites completed in 62.48 seconds; the test still reports missing live database/vendor references separately and makes no live acceptance claim.

## Required closure evidence

G11 can be re-audited only after all of the following are attached to the
coordinator-owned completion record:

- registered worker handlers with claim, retry, heartbeat, and replay
  evidence for every product projection;
- typed API and UI acceptance for Sales, Procurement/Inventory, and
  Asset/Maintenance, including loading, denied, stale, empty, unavailable,
  and read-only states where applicable;
- real two-database SQLx runs for every product context, including
  reconciliation mismatch and rollback/rebuild rehearsal;
- vendor acceptance packets with real endpoint/credential references kept out
  of logs and artifacts; and
- a Folio bridge boundary report proving read-only control-plane behavior and
  explicit approval before any command-owner or production cutover action.

## Rollback boundary

This verifier and audit are append-only evidence artifacts.  Removing them
does not remove authoritative product rows or outbox events.  Product
projection rollback must clear only the disposable read model and replay the
authoritative source outbox; no source-row deletion, Folio write retirement,
customer-directory activation, or production command-owner switch is
authorized by this packet.

## Coordinator continuation — product replay integrity — 2026-07-27

The current local implementation now has live disposable SQLx evidence for
the remaining product-context replay boundaries: Sales, SCM, POS, HR, Asset
Maintenance, Marketing delivery/CRM, and Legal. Their writes/projectors
validate holding scope, immutable event identity, source/outbox payloads,
version continuity, and idempotent replay; Legal human commands additionally
require session provenance. SCM projection also continues across bounded
source cursors. These are local acceptance results, not customer cutover or
vendor certification; Folio remains the operational write owner.

## Coordinator continuation — product shell route coverage — 2026-07-28

The target-neutral Dioxus UI now includes an authenticated, read-only
allow-listed shell registry covering Sales, Procurement, Inventory, Assets,
Maintenance, HR/Payroll, Legal, POS, and Marketing/CRM. Route resolution
removes query/fragment material, rejects credential-bearing links and path
traversal, and emits only safe navigation items. The updated acceptance
verifier reports 8/8, and the UI test suite passes.

This is shell contract evidence only. It does not replace live role journeys,
two-database product workflow/reconciliation/rollback, vendor/provider
acceptance, or Folio parity evidence; G11 remains open.

## Coordinator continuation — Sales bounded list read — 2026-07-28

Sales now has a separate-read-pool, company/branch-scoped, checkpoint-validated
and keyset-paginated list over the existing authoritative projection. The typed
`ListSalesAggregates` RPC enforces `sales.read`, explicit product and customer
scope containment, tenant read routing, bounded opaque cursors, and audit
publication. The console renders validated metadata with loading, empty,
denied/offline/error, and next-page states; aggregate payloads are discarded at
the list boundary. Evidence is recorded in
`audits/G9-SALES-READ-LIST-2026-07-28/README.md`.

This closes the Sales list-read implementation marker only. Disposable
two-database Sales acceptance, full Lead-to-Cash workflow, worker restart and
rollback, accounting/Operations reconciliation, vendor/provider journeys, and
Folio parity remain open.
