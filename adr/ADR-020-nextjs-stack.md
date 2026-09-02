# ADR-020: Next.js stack (App Router) — successor to ADR-002/010/014/016

> **Status:** Accepted (2026-09-02) — living on `development`. Supersedes ADR-002 (CQRS), deprecates ADR-010 (Dioxus target), ADR-014 (CQRS modular monolith), ADR-016 (Dioxus Native cutover).

## Context

On 2026-09-02 the Rust/Dioxus/Tonic/SQLx monorepo (137 crates) was hard-cut to a docs-only workspace
with a fresh Next.js scaffold (`f97b74d6` → `d15909bb` → `29136c81`). The prior stack is archived on
`backup/rust-2026-09-02` (`rust-archive-2026-09-02`, baseline `cc0887d`). New code lives in `src/app/`.

## Decision

- **App framework:** Next.js 16 App Router, React 19, TypeScript 5, Tailwind 4 (`@tailwindcss/postcss`, `src/app/globals.css`). Node 20+ / npm 11+.
- **API surface:** REST route handlers under `src/app/api/**` and/or tRPC procedures (versioned namespaces). Replaces Tonic gRPC (`:50051`) and Dioxus Server Functions (`/api/server-fn/*`).
- **Data:** Prisma or Drizzle (TBD) with Postgres. Replaces SQLx embedded migrations (28 files archived). New migrations live under `prisma/` or `drizzle/` and are referenced from `CONTRACT_POLICY.md` and `Plan/traceability.md` living columns.
- **Auth:** NextAuth or Clerk (TBD) — replaces ERPX custom identity provider for this branch; `AuthorityScope` / `HoldingId` isolation semantics from ADR-001/013 remain authoritative but are re-implemented in Next.js middleware.
- **Build/verify:** `npm run lint` + `npm run build` + `next.config.ts` route typecheck. Replaces `cargo xtask verify` (fmt, clippy, SQLx acceptance, Dioxus build, descriptor check). CI is `.github/workflows/nextjs.yml` (replaces `erpx-targets.yml` which is now Deprecated).
- **Docs truth:** `Plan/traceability.md` gains living `src/app` columns; archived Rust RPC table is marked `Archived` and points to `docs/contracts/proto/` on backup. `docs/contracts/next/` is the future living contract folder (OpenAPI/tRPC).

## Consequences

- ADR-002/010/014/016 are not deleted — they are marked Superseded/Deprecated and remain on `backup/rust-2026-09-02` for audit.
- `CONTRACT_POLICY.md` now governs REST/tRPC + Prisma, not protobuf field numbers.
- `CUTOVER.md` 2026-09-02 entry is the authority for the cutover; `Plan/LEGACY.md` points to backup for old baselines.
- Reintroducing Cargo/Dioxus/Tonic requires a new ADR and a branch from `backup/rust-2026-09-02`.

## Evidence required

- `npm run lint` and `npm run build` pass on `development` (`src/app/page.tsx` smoke).
- `docs/TOC.md` is the single hub linking business plan → glossary → ADRs → archive → code.
