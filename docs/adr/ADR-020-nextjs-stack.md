# ADR-020: Next.js App Router stack

Accepted 2026-09-02; current implementation clarified 2026-10-03. Supersedes ADR-002 and the Rust/Dioxus runtime decisions ADR-010/014/016. Runtime/deployment is governed by [ADR-024](ADR-024-local-postgresql-runtime.md), shared PostgreSQL by [ADR-032](ADR-032-shared-local-postgresql-cluster.md), and configured MinIO file storage by [ADR-035](ADR-035-minio-object-and-configuration-boundary.md).

## Decision

Next.js 16.4 App Router, React 19.3, TypeScript and Tailwind 4 own the application. Bun 1.4.3 manages the pinned dependencies; new standalone web/jobs releases target Node 26.11.1. The currently installed private production service remains on Node 24.20.0 until a reviewed release is deployed. Versioned REST route handlers live under `src/app/api/`; business authorization and domain services live under `src/server/`. This branch uses Better Auth and PostgreSQL runtime adapters/Drizzle; active SQL migrations are in `migrations/postgres/`. The scaffold's Prisma/NextAuth/Clerk/tRPC alternatives are not implemented contracts.

Holding isolation, current scoped assignments, owner-controlled human decisions, idempotency, immutable evidence and transactional audit/outbox survive the stack change. PostgreSQL jobs and rebuildable projections replace Tonic/SQLx transports and Cloudflare Workers persistence. Native integration, `bun run lint`, `bun run typecheck` and the isolated `bun run deploy:build` are current verification tools.

[Owner contracts](../contracts/next/README.md) and the [route table](../contracts/next/route-table.md) describe the implementation; [the production plan](../production-plan.md) owns delivery scope. A documentation change does not require unrelated schema/traceability edits. Breaking interfaces require a new ADR and cutover entry under [contract policy](../../CONTRACT_POLICY.md).

## Recovery

[CUTOVER](../../README.md#recovery) retains the Rust tag and verified commit boundaries. Reintroducing Cargo/Dioxus/Tonic requires a new architectural decision and an isolated recovery checkout. Historical decisions/evidence do not grant current runtime acceptance.
