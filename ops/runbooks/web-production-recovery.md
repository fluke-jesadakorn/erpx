# ERPX Web Production Recovery Runbook

> **Deprecated — Dioxus era (archived 2026-09-02).** Describes Postgres PITR / object recovery for the Rust monorepo. Archived on `backup/rust-2026-09-02`. The Next.js era will derive new runbooks under `ops/runbooks/nextjs-*.md`.

## Backup restore

Restore the latest logical backup into a disposable, isolated database and
object namespace. Verify checksums, schema version, RLS enforcement, tenant
scope containment, representative records, and object retrieval before marking
the drill passed.

## PostgreSQL PITR

Use the approved recovery point and an isolated target. Verify the base-backup
checksum, WAL continuity, timeline, recovery marker, database inventory, RLS,
and representative records. Promotion requires the explicit fenced production
procedure and a second operator.

## Object recovery

Restore versioned objects into a disposable namespace, verify content hashes
and authorization boundaries, and record the source backup plus correlation ID.
Do not overwrite the live namespace during a drill.

## Evidence

Record UTC timestamps, operator, release revision, target identifiers, measured
RPO/RTO, checksums, and pass/fail outcomes. `cargo xtask verify-web-production`
accepts only recent, versioned, all-pass evidence and never fabricates recovery
status from runtime health.
