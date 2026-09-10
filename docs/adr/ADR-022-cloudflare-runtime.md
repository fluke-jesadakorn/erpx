# ADR-022: Cloudflare runtime and persistence

> **Runtime/deployment superseded by [ADR-024](ADR-024-local-postgresql-runtime.md) — 2026-09-08. Historical decision accepted 2026-09-06.**

## Decision

ERPX keeps Next.js 16 App Router and React 19, builds it for Cloudflare Workers with the OpenNext adapter, and uses Bun for dependency and tool execution. Cloudflare D1 is the authoritative relational store, private R2 stores files, and Queues plus a scheduled/consumer Worker perform durable background work.

The control plane and every fixture Holding have distinct write and read D1 databases. The read databases are rebuildable projections. Holding isolation, Company/Branch placement, permissions, lifecycle guards, and separation of duties are enforced by server policy and constrained D1 statements because D1 does not provide PostgreSQL roles or row level security.

Business commands commit their aggregate mutation, command receipt, decision evidence, audit event, and outbox event in one D1 batch. A constrained assertion row forces rollback when a guarded update did not produce the expected command marker and version. Queue consumers are idempotent because delivery is at least once.

R2 buckets remain private. The application authorizes each intent and download, validates bytes and signatures, uses unique staging and immutable revision keys, and reconciles failures between D1 and R2.

## Consequences

- PostgreSQL, MinIO, Docker infrastructure, and a persistent Bun worker are removed from the active architecture.
- Interactive SQL transactions and PostgreSQL RLS assumptions cannot be used.
- JavaScript integer precision, bounded D1 batches, database limits, projection lag, duplicate queue delivery, and cross service recovery require explicit handling.
- A successful Next.js build is insufficient; OpenNext build, local Workers preview, Workers integration tests, and a deployed smoke test are release gates.
- Live payments, banking, payroll, directory, and campaign providers remain unavailable until their adapters and evidence contracts are configured.

## Supersedes

This narrows the runtime and persistence portions of ADR-020 and supersedes local PostgreSQL/MinIO assumptions in the earlier 2026-09-06 plan. The product and ownership boundaries in ADR-001, ADR-004, ADR-013, ADR-017, ADR-019, ADR-020, and ADR-021 remain in force where they do not conflict with this decision.
