# ADR-002: CQRS and replaceable read models

> **Superseded by ADR-020 (Next.js stack, 2026-09-02). Retired — see ADR-020 for App Router + Prisma/Drizzle + REST/tRPC.**
> See `CUTOVER.md` (2026-09-02) and `ADR-020-nextjs-stack.md`.


**Status:** Accepted for implementation  
**Decision:** PostgreSQL aggregate state on the write database is authoritative. Outbox events are committed atomically with aggregate changes. Workers project events into a physically separate read database with checkpoints, idempotency keys, aggregate ordering, bounded retries, dead letters, replay, and replacement-read-database switching.

**Rationale:** Operational writes need transactional consistency; global dashboards and customer views need independently rebuildable projections. A fully event-sourced aggregate store is not required by the target architecture.

**Consequences:** Every command carries aggregate version and idempotency context. Read responses expose projection freshness/consistency metadata. Losing a read database must not stop holding writes.

**Evidence required:** atomicity rollback test, duplicate/out-of-order projector tests, DLQ replay, full rebuild, checkpoint, and safe projection switch tests.