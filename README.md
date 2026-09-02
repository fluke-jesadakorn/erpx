# ERPX — Next.js (rebuilt 2026-09-02)

Rust/Dioxus/Tonic/SQLx 137 crates (`:3004/:50051`) hard-cut to **Next.js 16** on `development` (`d15909bb`). Docs preserved, code not migrated. Recovery:

```bash
git checkout backup/rust-2026-09-02        # full Rust monorepo
git show rust-archive-2026-09-02 --stat    # f97b74d6
git show cc0887d --stat                    # baseline
# 37 proto + 28 SQL pruned at 99a66de7, see docs/plan/ARCHIVE.md; living REST/tRPC+Prisma in src/app/ + docs/contracts/next/
```

## Stack

Next.js 16 App Router + React 19 + TS5 + Tailwind4 + ESLint. Node 20+ / npm 11+. No Cargo.

```bash
npm install; npm run dev   # :3000 hot reload src/app/*
npm run lint; npm run build
```

## Structure

`src/app/` (layout/page/route handlers) · `public/` · `docs/` hub (`docs/TOC.md` → `docs/plan/MAP.md` 31 contexts) · `docs/plan/` (31) + `docs/adr/` (20, ADR-020 living) · `audits/` stub → `docs/archive/dioxus-2026-08-evidence/` · `Designed/MainDesign.pen` (Pencil MCP) · `fixtures/` `ops/` (living runbooks) · `.agents/skills/`

## Business context

`docs/TOC.md` → `docs/plan/README.md` (31 contexts, `@` contract) + `CONTEXT.md` (glossary) + `docs/plan/traceability.md` + `docs/adr/` (ADR-020). Legacy on `backup/rust-2026-09-02`.

## Verification

`npm run lint` + `npm run build` must pass. Previous `cargo xtask verify` on backup. Optional `playwright` e2e.
