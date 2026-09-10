# ERPX — Next.js (rebuilt 2026-09-02)

Active deployment: [Node.js/PostgreSQL on this Mac through Cloudflare Tunnel](ops/macos/README.md), per [ADR-024](docs/adr/ADR-024-local-postgresql-runtime.md). Cloudflare Workers configuration is historical. Use `bun run deploy:build` for production.

Rust/Dioxus/Tonic/SQLx 137 crates (`:3004/:50051`) hard-cut to **Next.js 16** on `development` (`d15909bb`). Docs preserved, code not migrated. Recovery:

```bash
git checkout backup/rust-2026-09-02        # full Rust monorepo
git show rust-archive-2026-09-02 --stat    # f97b74d6
git show cc0887d --stat                    # baseline
# 37 proto + 28 SQL pruned at 99a66de7, see docs/plan/ARCHIVE.md; living REST/tRPC+Prisma in src/app/ + docs/contracts/next/
```

## Stack

Next.js 16 App Router + React 19 + TypeScript + Tailwind 4, built as a standalone Node.js 24.20.0 release. Bun 1.2.17 manages the toolchain; PostgreSQL 18.6 stores relational data and durable jobs, and private filesystem adapters store evidence. Cloudflare Tunnel provides HTTPS ingress. No Cargo.

```bash
bun install --frozen-lockfile
# Configure absolute data/manifest paths and a separate development PostgreSQL URL first.
bun run seed:local           # development data only; never production
bun run seed:verify:local   # read-only seed checks
bun run dev                # :3100 UI development
bun run deploy:build       # isolated production release
bun run lint; bun run typecheck; bun run test:integration
```

The rich local dataset and demo personas are documented in [docs/local-seed.md](docs/local-seed.md).

## Structure

`src/app/` (workplace and APIs) · `src/server/` (policy/domain) · `scripts/local/` (jobs and operations) · `migrations/postgres/` · `ops/macos/` · `docs/` hub (`docs/TOC.md`) · `Designed/mockup/` reference · `fixtures/` · `ops/`

## Business context

`docs/TOC.md` → `docs/plan/README.md` (31 contexts, `@` contract) + `CONTEXT.md` (glossary) + `docs/plan/traceability.md` + `docs/adr/` (ADR-020). Legacy on `backup/rust-2026-09-02`.

## Verification

`bun run lint`, `bun run typecheck`, `bun run deploy:build`, and `bun run test:integration` must pass. Shared development additionally requires deployed smoke and Playwright journeys.
