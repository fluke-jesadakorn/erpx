# Persistence gate evidence

Confirmed current gaps, not accepted behavior:

- `erpx/apps/server` constructs `InMemoryControlPlane` and seeds a demo customer at startup.
- `ControlPlanePools` exists as an unused SQLx connection primitive; no production repository implements the application ports.
- No worker app, durable outbox, multi-worker claim, retry/DLQ/replay, persistent checkpoint, or read-database rebuild/switch exists at the G0 boundary.
- Platform SQL tables are co-located in Folio's `folio_db` and writes use the Folio pool; physically separated `erpx_control_write/read` databases do not yet exist.
- No tenant write/read database provisioning, MinIO namespace registry, backup/PITR/restore test, or migration command-owner switch exists.

G1-001 through G1-004 and G6 must replace these gaps with clean migration, restart persistence, atomicity, isolation, replay, rebuild, backup, and rollback evidence before any production claim.
