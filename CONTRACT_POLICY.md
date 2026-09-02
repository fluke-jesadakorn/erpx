# ERPX contract policy — Next.js era (from 2026-09-02)

The Rust protobuf era is archived on `backup/rust-2026-09-02` (`rust-archive-2026-09-02`, `f97b74d6`).
The archived 37 `*.proto` files and descriptors under `docs/contracts/proto/` (pruned on `development`)
and 28 SQLx migrations under `docs/schema/migrations*` are reference-only.

**Living contracts on this branch are REST/tRPC + Prisma/Drizzle:**

- API surface uses versioned REST routes / tRPC procedures under `src/app/` (App Router route handlers). Namespaces are versioned (`/api/v1/...` / `trpc.<context>.procedure`).
- Existing route/procedure names, field names and semantics are never reused with a different meaning. Removed fields remain reserved.
- Commands carry an idempotency key and authoritative tenant/holding scope (carry-over from `crates/proto` policy).
- Every contract change updates: the OpenAPI/tRPC definition, the Prisma/Drizzle schema, `docs/plan/traceability.md`, and `docs/contracts/next/` (once route table is materialized), plus compatibility tests.
- Breaking changes require a new ADR and a new entry in `CUTOVER.md`.
