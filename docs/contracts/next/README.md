# docs/contracts/next — Living contracts (Next.js era)

> Skeleton — living contracts for Next.js (archived Rust 37 proto on `backup/rust-2026-09-02`, see `docs/plan/ARCHIVE.md`).

## Intended contents

- `openapi.yaml` — REST route handlers under `src/app/api/**` (versioned)
- `trpc/` — tRPC router definitions (if tRPC is adopted)
- `route-table.md` — human-readable route → context → spec matrix (living counterpart to `docs/plan/traceability.md` archived table)

## Rules (from CONTRACT_POLICY.md + ADR-020)

- Every change here updates `CONTRACT_POLICY.md`, `docs/plan/traceability.md` (living columns), and the Prisma/Drizzle schema.
- Field/procedure names are never reused with a different meaning; removed fields remain reserved.
- Commands carry idempotency key + tenant/holding scope.
- Breaking changes require a new ADR and `CUTOVER.md` entry.

## Recovery

Archived Rust contracts: `git checkout backup/rust-2026-09-02 -- docs/contracts/proto/`
