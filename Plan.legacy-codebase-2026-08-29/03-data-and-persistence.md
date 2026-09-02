# 03 — Data & Persistence (Codebase-Derived)

**Source:** `crates/migration/migrations/0001_baseline.sql` (4346 lines) + `0002..0007`, `crates/infrastructure/src/*.rs`, `crates/adapters/*`.

## 3.1 Schemas

- Migrations: `0001_baseline`, `0002_outbox_commit_order`, `0003_platform_record_collaboration_phase2`, `0004_global_collaboration_thread_documents`, `0005_global_collaboration_attachment_authority`, `0006_global_collaboration_typed_mentions`, `0007_ai_provider`.
- Control schema `erpx_control` owns: `customers`, `support_tickets`, `support_sessions`, `role_assignments`, `audit_events`, `outbox_events`, `projection_checkpoints`, `projection_event_receipts`, `holding_registry`, `global_read_audits`, `commercial_catalog_revisions`, `billing_lifecycles`, `entitlement_allocation_books`, `stripe_event_inbox`, `support_*`, `incidents`, `sod_decisions`, `holding_provisioning`, `holding_database_health`, `holding_storage_health`, `holding_backup_*`, `holding_projection_*`, `customer_scope_bindings`, `object_access_bindings`, `worker_heartbeats`, `role_review_*`, `marketing_*`, `finance_bank_*`, `holding_owner_seeds`, `migration_*`, `command_ownership`, `demo_*`, `platform_pitr_*`, `customer_holding_bindings`, `stripe_*`, `billing_grace_extensions`, `holding_platform_access_audit`, plus tenant `brand_*`, `collaboration_*`, `finance_*`, `operations_*`, etc. (105+ tables in baseline; full list via `grep CREATE TABLE crates/migration/migrations/0001_baseline.sql`).

## 3.2 CQRS & outbox

- Every command writes aggregate row + `outbox_events` (idempotent key, `aggregate_type/id/version`, `event_type/version`, `occurred_at/available_at`, claim/publish/dead-letter columns) in one TX. Workers claim by `available_at, occurred_at`.
- Projections are rebuildable; `projection_checkpoints(projection_name, aggregate_type, aggregate_id)` + `projection_event_receipts` guarantee idempotent convergence.
- `Item Directory` is a disposable federated read projection, not a bounded context.

## 3.3 Isolation & routing

- Physical per-holding (and optional per-branch) PostgreSQL pairs. `apps/server/src/target_transport.rs` resolves `HoldingDataRouteScope` and `RoutedDatabaseSide` per request.
- Control-plane branch route is authoritative; migration/rollback fence branch writes.

## 3.4 Search & reporting

- `customers.search_document` GIN index, plus per-context read models for list/query RPCs (every `*QueryService` is a projection consumer).
- PITR via `wal-archive` + `platform_pitr_*` chains, leased rehearsal (`cargo xtask pitr rehearsal`).

## 3.5 Non-goals

- No cross-holding joins. No long-term dual writes. No synthetic provider success in dev.
