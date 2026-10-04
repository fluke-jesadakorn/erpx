# ERPX contract policy — Next.js era (from 2026-09-02)

The Rust protobuf era is archived on `backup/rust-2026-09-02` (`rust-archive-2026-09-02`; verified refs in `CUTOVER.md`).
The archived 37 `*.proto` files and descriptors under `docs/contracts/proto/` (pruned on `development`)
and 28 SQLx migrations under `docs/schema/migrations*` are reference-only.

**Living contracts on this branch are versioned REST + PostgreSQL on Node.js (ADR-024):**

- API surface uses versioned REST routes under `src/app/` App Router route handlers. Workspace APIs mirror canonical immutable-slug routes below `/api/v1/platform/...` and `/api/v1/holdings/{holdingSlug}/...`, including Branch and Department prefixes.
- Existing route/procedure names, field names and semantics are never reused with a different meaning. Removed fields remain reserved.
- Commands carry an idempotency key and authoritative tenant/holding scope (carry-over from `crates/proto` policy).
- Update the affected contract and relevant compatibility checks. Add a PostgreSQL migration only when storage changes; update route tables or schema references only when their represented surface changes. Documentation-only edits do not require unrelated schema, policy, traceability or ADR changes.
- Breaking changes require a new ADR and a new entry in `CUTOVER.md`.
