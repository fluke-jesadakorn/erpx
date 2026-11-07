# ERPX production recovery

The active recovery procedure is [Mac deployment and recovery](../macos/README.md), per [ADR-024](../../docs/adr/ADR-024-local-postgresql-runtime.md). This deployment uses local PostgreSQL logical backups and private filesystem snapshots. It has no managed-provider PITR. Encrypted off-machine export is supported by the operator only when separately configured and verified; do not infer that an off-machine backup exists from source support alone.

1. Inspect `/Library/Application Support/ERPX/logs/` and run `sudo /usr/bin/python3 '/Library/Application Support/ERPX/ops/erpxctl.py' doctor`.
2. Select a complete root-private snapshot under `backups/`. Keep the matching release installed. Do not use a `.partial` snapshot.
3. Run `... erpxctl.py restore <absolute-snapshot-path>`. The command verifies checksums and database identities, saves current state, pauses web/jobs, restores both database and files, and selects the matching release. A failed restore leaves maintenance active; diagnose the failure before resuming requests.
4. Verify `/api/health`, `/api/ready`, HTTPS login and a private file range download. Check the jobs heartbeat, backlog and dead-letter state.
5. Use `... erpxctl.py rollback <release-id>` only when schema hashes match. Different schema versions require the corresponding database/files backup.

Record UTC timestamps, operator, release, snapshot, checksums, elapsed recovery time and pass/fail. Practice on a separate test cluster. Existing PostgreSQL 5432 and unrelated tunnels must remain running. Full Mac reboot testing requires the owner's scheduled acceptance window.

## Catalogue restore verification

The complete schema restore uses PostgreSQL18.6 `pg_restore --transaction-size=64 --exit-on-error`. Restoring every existing table, index and trigger in one transaction exhausted the shared local server's lock table during the engineering rehearsal. PostgreSQL documents bounded restore transactions as a way to limit those locks ([PG18 pg_restore](https://www.postgresql.org/docs/18/app-pgrestore.html)). The operator keeps maintenance active after any failed restore batch, before replacing files or activating the matching release. A failed restore requires recovery from the retained safety snapshot; it does not imply an atomic rollback of every database.

`tests/integration/shared.evidence/engineering-recovery.test.ts` verifies a full isolated schema/data snapshot, age export/import, exact restored table digests and FileStore bytes, compatible release rollback, rejection of an incompatible schema and ciphertext tampering. Final engineering verification supplies `ERPX_ENGINEERING_RELEASE_DIR`, `ERPX_ENGINEERING_SOURCE_FINGERPRINT` and a private `ERPX_ENGINEERING_EVIDENCE_DIR`; the test then verifies and uses the candidate's packaged operator and compiled artifact digests. Without those variables the test reports only an isolated controller fixture and cannot prove the release gate.
