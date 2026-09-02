# ERPX — Next.js (rebuilt 2026-09-02)

ERPX was a Rust/Dioxus/Tonic/SQLx monorepo (137 crates, Dioxus console on `:3004`, Tonic on `:50051`). On 2026-09-02 the workspace was **hard-cut to Next.js** — all Rust was removed from this branch (`development`) and replaced by a fresh Next.js 16 App Router scaffold. Docs were preserved; code was not migrated.

## Recovery

Full Rust source + history is archived:

```bash
git checkout backup/rust-2026-09-02        # branch with complete Rust monorepo
git show rust-archive-2026-09-02 --stat    # tag at cutover point f97b74d6
git show cc0887d --stat                    # pre-cutover baseline per CUTOVER.md
```

Contracts and schemas (37 proto files + 28 SQL files) were briefly copied to `docs/` at `f97b74d6` then pruned from this branch (`99a66de7`, see `docs/README.md`). Reference remains on `backup/rust-2026-09-02`. Living contracts on this branch will be REST/tRPC + Prisma under `src/app/` and `docs/contracts/next/` — see `CONTRACT_POLICY.md` and `docs/TOC.md`.

## Stack

- **Next.js 16** (App Router, `src/app/`), React 19, TypeScript 5
- **Tailwind CSS 4** (`@tailwindcss/postcss`, `src/app/globals.css`)
- **ESLint** (`eslint-config-next`)
- Node 20+, npm 11+ (or pnpm 10+). No Cargo/Rust toolchain.

## Local development

```bash
npm install
npm run dev      # http://localhost:3000 — hot reload on src/app/*
npm run lint     # eslint
npm run build    # production build + route typecheck
```

Environment via `.env` / `.env.local` (ignored, see `.env.example`). `node_modules/`, `.next/`, `out/` are ignored.

## Project structure

```
src/app/            # App Router — layout.tsx, page.tsx, globals.css, (future route handlers)
public/             # static assets (next.svg, vercel.svg etc)
docs/plan/               # business plan (31 contexts) + traceability.md + ARCHIVE.md canonical
docs/adr/                # 20 ADRs (19 historical + ADR-020 Next.js stack) — see docs/adr/README.md
audits/             # stub → docs/archive/dioxus-2026-08-evidence/ (22 dirs 3.9M, see audits/README.md)
Designed/           # Pencil canvas MainDesign.pen — edit only via Pencil MCP + README.md
docs/               # TOC.md hub + agents/ + archive/ (contracts/schema pruned — see docs/README.md + docs/plan/ARCHIVE.md)
fixtures/ ops/      # fixtures + runbooks (ops/ living Next.js: nextjs-incident.md nextjs-recovery.md — see ops/README.md)
.agents/skills/     # workflow skills (canonical; .claude/skills removed)
```

`Designed/MainDesign.pen` is encrypted — access only via Pencil MCP (see `Designed/AGENTS.md`).

## Business context

See `docs/TOC.md` (single hub) → `docs/plan/README.md` (vision, scope, 31 contexts, `@`-mention contract), `CONTEXT.md` (glossary), `docs/plan/traceability.md` (archived RPC table + living Next.js routes), `docs/adr/` (20 decisions, ADR-020 is the Next.js stack). Legacy specs are archived on `backup/rust-2026-09-02` (`docs/plan/LEGACY.md`).

## Verification

Before handoff:

```bash
npm run lint
npm run build       # must pass with zero TS errors
# optional browser smoke:
# npm run dev & curl -s http://localhost:3000 | head
```

Previous Rust gate (`cargo xtask verify` — fmt, clippy, workspace tests, SQLx acceptance, Dioxus build, heartbeat) is obsolete on this branch; see backup branch for that workflow. Future e2e can reuse `playwright` (previously `devDependencies.playwright@1.62.1`).

## Cutover notes

- `AGENTS.md` tooling section flipped from Cargo to Next.js; it still carries the `nextjs-agent-rules` header injected by `next dev`.
- `.gitignore` now ignores Next.js artifacts (`.next/`, `out/`, `node_modules/`, `*.tsbuildinfo`) plus legacy entries (`.env*`, `.DS_Store`).
- `Cargo.toml`, `crates/`, `apps/`, `xtask/`, `third_party/`, `target/`, `.cargo/`, `.runtime/`, legacy `tailwind.config.js`/`package.json` were deleted on `d15909bb` (docs-only cutover).

To reintroduce Rust or dual-stack, create a new ADR and branch from `backup/rust-2026-09-02`.
