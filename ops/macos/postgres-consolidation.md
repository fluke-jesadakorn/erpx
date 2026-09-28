# PostgreSQL 5432 consolidation (2026-09-24)

Production (`erpx_*`) and staging (`erpx_stage_*`) now share the existing PostgreSQL 18.6 cluster at `127.0.0.1:5432` with development (`erpx_dev_*`) and other local projects. Web/jobs still run as separate services on `3101` and `3103`; development remains on `3100`. No Tunnel route, auth secret, or file directory changed. The old `5433` and `5435` data directories remain untouched under each ERPX home.

## Service and data map

| Environment | Web | Database names | Files | Backup |
| --- | --- | --- | --- | --- |
| Development | `3100` | `erpx_dev_*` on `5432` | workspace dev data | existing dev process |
| Production | private `3101` | `erpx_*` on `5432` | `/Library/Application Support/ERPX/data/files` | `com.erpx.backup` at 03:15 |
| Staging | `3103` | `erpx_stage_*` on `5432` | `/Library/Application Support/ERPX-Staging/data/files` | `com.erpx.staging.backup` at 03:45 |

`com.erpx.shared-postgres` runs the **existing** `/Users/fluke/.local/share/dev-infra/postgresql/18/data` directory as user `fluke` from a system LaunchDaemon. The previous `gui/501/com.localdev.postgresql` LaunchAgent is disabled but its plist remains. The old `com.erpx.postgres` and `com.erpx.staging.postgres` daemons are disabled. Do not enable either one during normal operation. No new cluster was initialized and `max_connections` remains 180.

The target cluster's `pg_hba.conf` has an ERPX block for each migrated environment, ahead of the general local rules. Its exact role list can connect to that environment's database names over `127.0.0.1`; other database names, IPv6 and local sockets are rejected for those roles. This is needed because some unrelated databases grant `CONNECT` to `PUBLIC`. Database grants inside the imported databases remain scoped. The HBA file and a previous copy are stored in each root-private `backups/consolidation-*` directory. The copied PostgreSQL admin users remain superusers with root-only credentials; runtime app/jobs/migration roles are not superusers.

## Cutover record

- Target and sources reported PostgreSQL 18.6. Target `max_connections` was 180 with 24 active sessions at preflight. Available disk space was approximately 135 GiB.
- Before cutover there were three PostgreSQL 18 main server processes (one per cluster); afterward there is one. The three pre-cutover main-process RSS values were about 5.2, 5.2, and 6.9 MiB, while the shared main process later reported about 35.6 MiB. These are process RSS samples, not total cluster memory measurements, so they do not establish a net memory saving. `max_connections` was not raised.
- Production had 42 databases (about 472 MB) and staging had 8 (about 150 MB). The source runtime roles had no superuser flag.
- Before each cutover, the operator created a complete, checksummed snapshot under that environment's `backups/`. The cutover stopped only that environment's web/jobs, confirmed source database sessions had exited, then wrote final frozen custom-format dumps, files, configs, and a per-database inventory under a root-private `backups/consolidation-*` directory.
- Target roles retain their SCRAM password hashes and attributes. `pg_restore --create` retained database/object owners and grants. Every restored database matched the source's public-table counts, schema object count, and migration count before its manifests switched to `5432`.
- Staging moved first, then production. Each environment passed local `/api/health` and `/api/ready` before its former PostgreSQL daemon was unloaded and disabled. The old data directories and the final frozen dumps remain available for recovery.
- The former user LaunchAgent was replaced by the system LaunchDaemon with the same data directory. The system service started and local/public health checks passed afterward. A physical machine reboot has **not** been performed as part of this migration; verify start after the next scheduled reboot.
- The installed production and staging backup commands completed after cutover. A staging control dump restored into a temporary `erpx_test_restore_*` database with the same table counts, schema object count, and migration count, then that test database was dropped. Both operator `doctor` commands completed. All 24 staging and 84 production manifest login roles reached their own database and were denied on sampled dev, test, foreign ERPX, and `postgres` databases.
- Local and public `/api/health` checks returned HTTP 200 for development and staging. Local production and staging `/api/ready` returned HTTP 200; public staging `/api/ready` also returned 200. Staging administrator sign-in returned 200 through the local origin with its canonical host, and an authenticated data page returned 200. The Chrome session available to the agent was signed out, so an authenticated browser page and private production sign-in were not visually verified.

## Normal operations

Use the installed operator for a scoped backup:

```sh
sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' backup
sudo /usr/bin/python3 '/Library/Application Support/ERPX-Staging/ops/erpxctl.py' --home '/Library/Application Support/ERPX-Staging' backup
```

Each command pauses only its own web/jobs and reads only the database names in its own manifest. The same `--home` argument scopes staging restore, export, import, rollback, and doctor. The operator refuses production, staging, dev, or test names outside its selected environment. Its installed Python modules must include `erpxctl.py`, `offsite.py`, and `recovery_status.py` together. An imported backup keeps current database login manifests and restores app secrets and files for that environment; live restore uses recorded object owners.

Check service state with `launchctl print system/com.erpx.shared-postgres` and `launchctl print-disabled system`. If `5432` is down, inspect the shared service and its data/log paths. Do not start the old clusters to repair a shared-cluster outage.

## Rollback

Before any target writes, stop the selected environment's web/jobs, put its maintenance marker in place, restore that environment's original `config/admin.json` and all original manifests from its latest `backups/consolidation-*/original-config/`, enable and bootstrap only its old PostgreSQL daemon, then start its jobs/web and verify health and readiness. Remove only that environment's HBA block after confirming no process uses target credentials. Leave the other environment and dev untouched.

After writes have reached `5432`, **do not point web/jobs at the old source cluster directly**: it is stale. Stop writers, take a new scoped backup from `5432`, transfer the changed databases and files back to the old cluster, compare inventory and grants, and only then switch config and restart. Keep the shared target intact until that reconciliation has been verified. A code rollback alone does not reverse data changes.

The original isolated-cluster installers, `install.py` and `install-staging.py`, still hard-code `5433`/`5435`. They are legacy installation references and must be updated before any new shared-cluster installation.
