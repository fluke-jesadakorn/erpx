# ERPX production recovery

The active recovery procedure is [Mac deployment and recovery](../macos/README.md), per [ADR-024](../../docs/adr/ADR-024-local-postgresql-runtime.md). This deployment uses local PostgreSQL logical backups and private filesystem snapshots. It has no managed-provider PITR or off-machine backup.

1. Inspect `/Library/Application Support/ERPX/logs/` and run `sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' doctor`.
2. Select a complete root-private snapshot under `backups/`. Keep the matching release installed. Do not use a `.partial` snapshot.
3. Run `... erpxctl.py restore <absolute-snapshot-path>`. The command verifies checksums and database identities, saves current state, pauses web/jobs, restores both database and files, and selects the matching release. A failed restore leaves maintenance active; diagnose the failure before resuming requests.
4. Verify `/api/health`, `/api/ready`, HTTPS login and a private file range download. Check the jobs heartbeat, backlog and dead-letter state.
5. Use `... erpxctl.py rollback <release-id>` only when schema hashes match. Different schema versions require the corresponding database/files backup.

Record UTC timestamps, operator, release, snapshot, checksums, elapsed recovery time and pass/fail. Practice on a separate test cluster. Existing PostgreSQL 5432 and unrelated tunnels must remain running. Full Mac reboot testing requires the owner's scheduled acceptance window.
