# Next.js Production Recovery Runbook

> **Living — Postgres PITR + Object recovery for Next.js.** Replaces `docs/archive/ops-dioxus-2026-09-02/runbooks/web-production-recovery.md`.

## Backup restore

Restore latest logical backup into disposable isolated DB + object namespace (Neon/Supabase `pg_dump` / S3 `restore`). Verify checksums, schema `prisma migrate`, RLS, tenant containment, `GET /api/health`.

## Postgres PITR

Use provider PITR (Neon `branch` / Supabase `PITR`) with isolated target. Verify base-backup checksum, WAL continuity, `DATABASE_URL` switch.

## Object recovery

Restore versioned objects (R2/S3) into disposable namespace, verify hashes, auth boundaries.

## Evidence

Record UTC timestamps, operator, revision, target, RPO/RTO, checksums, pass/fail — `ops/web-production-evidence.example.json` (REQ-E2E-*).
