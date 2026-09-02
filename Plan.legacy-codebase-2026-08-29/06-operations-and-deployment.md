# 06 — Operations & Deployment (Codebase-Derived)

**Source:** `README.md`, `xtask/src/*`, `apps/bootstrap`, `apps/migrator`, `apps/wal-archive`, `ops/`, `CUTOVER.md`.

## 6.1 Local development

```bash
cargo xtask reconfigure-local   # idempotent env/TLS bootstrap
cargo xtask reset-dev --confirm RESET_ERPX_DEV  # destructive, allow-listed
cargo xtask seed-demo           # idempotent; prints bootstrap recovery once
cargo xtask dev --detach        # hot reload (debug-off); --fresh for reconcile; --web-debug/--hot-patch optional
cargo xtask status              # recorded child processes + PITR lease
cargo xtask stop
```

Runtime `.runtime/erpx/` holds logs/PIDs; ignored in git. Tailnet HTTPS via `tailscale serve --bg --https=443 http://127.0.0.1:3004` when `ERPX_DEV_TAILSCALE_FQDN` set.

## 6.2 Verification gates

- **Quick:** `cargo xtask check-affected [--dry-run]` — format + owning-package `cargo check`.
- **Thorough:** `cargo xtask test-affected [--dry-run]` — format + changed-package tests + reverse-dep checks + mapped harnesses.
- **Full:** `cargo xtask verify` — fmt, clippy (warnings denied), workspace tests, SQLx disposable-DB acceptance, Dioxus checks, Web build, descriptor/legacy scan, live `/health` `/ready` :3004 smoke.
- Single-harness: `cargo xtask test <harness> [filter]` (harnesses in `README.md`).

## 6.3 Migrations & cutover

- Migrations in `crates/migration/migrations/*.sql` applied by `apps/migrator`. Current baseline is 0001..0007.
- Atomic hard cutover model (ADR-016): singular stable routes (`/reference/item/:id`), no compatibility aliases for legacy `/workspaces`, `/api/bff`, `/references/:id`, `IFC-SVC-001..065`, event namespaces.
- Recovery baseline `cc0887dd...` preserved in `CUTOVER.md`; Folio artifacts deleted without archival (no data rollback).

## 6.4 Backups & PITR

- `holding_backup_*` + `platform_pitr_*` inventory; `cargo xtask pitr rehearsal` acquires 2h lease / 4 GiB WAL budget, verifies LSN/time restore, auto-purges; `cargo xtask pitr cleanup-local --confirm PURGE_ERPX_LOCAL_PITR`.

## 6.5 Observability

- `worker_heartbeats`, `holding_database_health`, `holding_storage_health`, `holding_projection_switches/jobs/claims/rebuilds`, `audit_events`, `global_read_audits`.
