# ERPX Docs — Single Hub (TOC)

> **One hub for everything.** Start at `README.md`, land here, then follow the walk that fits you. This file replaces the prototype in `.scratch/docs-seamless/prototype/TOC-seamless.md`.

## Tree — 3 layers (after 2026-09-02 prune)

```
README.md                           # entry — Next.js reboot 2026-09-02, recovery: backup/rust-2026-09-02 / tag rust-archive-2026-09-02 / cc0887d
├── AGENTS.md + CLAUDE.md           # @AGENTS.md — tooling (Node 20+ / npm) + skills; fences at top/bottom are machine-owned
├── CUTOVER.md                      # 2 entries: 2026-09-02 Rust→Next.js (new, on top) + 2026-07-31 Folio→Rust (history)
├── CONTEXT.md                      # canonical glossary (46 terms) — implementation-free, single source for every term
├── CONTRACT_POLICY.md              # Next.js policy: REST/tRPC + Prisma/Drizzle (protobuf era archived on backup)
├── docs/plan/                           # business plan — 31 contexts, living spec
│   ├── README.md                   # Code is truth → archived contracts on backup + living src/app
│   ├── 01-vision-and-scope.md      # vision, tiers, @-mention contract + spine
│   ├── MAP.md                      # **flat index 31** — find any context without remembering group (e.g., sales = 03)
│   ├── 02-capabilities/<group>/<context>.md  # 13 groups / 31 contexts — one-pagers 26-35L
│   ├── 03-cross-cutting/@-and-ai/  # 31 deep 60-65L delta + _shared-contract.md 109L single source
│   ├── traceability.md             # Archived 552 rows in <details> + living docs/contracts/next/route-table.md
│   ├── glossary.md                 # pointer → CONTEXT.md
│   └── ARCHIVE.md                  # canonical for pruned 43+40 + 37 proto + 28 SQL (replaces LEGACY)
├── docs/adr/                            # 20 ADRs: 15 Reconfirmed ✅ + 3 Deprecated (010,014,016) + 1 Superseded (002→020) + 020 Next.js stack
├── docs/
│   ├── TOC.md                      # ← you are here
│   ├── README.md                   # archive status — contracts/schema pruned on this branch, see backup
│   ├── contracts/README.md         # inventory of archived 37 proto files + descriptors (pruned, on backup)
│   ├── schema/README.md            # inventory of archived 28 SQL migrations (pruned, on backup)
│   ├── contracts/next/README.md    # skeleton — living contracts policy
│   ├── contracts/next/route-table.md # **living route table** — grows as src/app/api/* handlers land (see Temp below)
│   ├── archive/delivery-2026-08/   # Delivery/README.md 768L (Rust DDD) — archived with banner
│   ├── archive/wayfinder-2026-09-02/ # wayfinder decisions (map + 10 tickets + prototype) — audit trail
│   └── agents/                     # skills meta — domain.md (single-context) + issue-tracker.md (GitHub)
├── Designed/
│   ├── README.md                   # stub — 27 PNGs + routes /work-queue, /billing/*, /sales, /work + link to Plan
│   ├── AGENTS.md                   # Pencil-only guardrail
│   └── MainDesign.pen (15M) + images/ (27 PNG, 16M)  # single source — do not grep .pen
├── audits/                         # stub → docs/archive/dioxus-2026-08-evidence/ (22 dirs 3.9M; see audits/README.md)
├── docs/archive/
│   ├── dioxus-2026-08-evidence/    # 22 dirs 3.9M (pruned 330→22) — see docs/archive/dioxus-2026-08-evidence/README.md + backup
│   ├── delivery-2026-08/           # Delivery 768L (Rust DDD) archived
│   ├── ops-dioxus-2026-09-02/      # ops 68K (Dioxus runbooks :3004/:50051, erpx_*) archived
│   └── wayfinder-2026-09-02/       # 10 tickets + prototype — audit trail
├── fixtures/ + ops/                # fixtures (ai/* 4 json, migration) + ops/ living Next.js (nextjs-incident.md nextjs-recovery.md)
├── src/app/                        # living Next.js — layout.tsx, page.tsx, globals.css, future route handlers
├── .agents/skills/                 # canonical 6 skills (domain-modeling, grilling, prototype, research, wayfinder)
├── .github/workflows/
│   ├── nextjs.yml                  # living CI — npm run lint + build
│   └── erpx-targets.yml            # Deprecated — Rust CI, kept for history
└── .gitignore                      # Next.js + legacy Rust ignores
```

## Sales — Quick Jump (1 hop)

- `docs/plan/02-capabilities/03-finance-sales-pos/sales.md` 26L — 13 RPCs `CreateQuote→Fulfill`
- `docs/plan/03-cross-cutting/@-and-ai/sales.md` 69L — `SALES_QUOTE_FIELDS` + Happy `CreateQuote→Send→Accept→CreateSalesOrder→Fulfill` + `/sales` — see `_shared-contract.md` for @/AI
- `docs/plan/MAP.md` row `sales | 03 | sales.md | @-and-ai/sales.md | POST /api/sales/quotes | @sales/quote-2026-042`
- Living: `docs/contracts/next/route-table.md` — `POST /api/sales/quotes` (TBD)
- Archived: `backup/rust-2026-09-02:crates/proto/erpx/sales/service.proto` + `docs/archive/dioxus-2026-08-evidence`

**One-liner:** `README.md` → `docs/TOC.md` → `docs/plan/MAP.md` search `sales` → `02/sales.md` + `03/sales.md` + `_shared-contract.md` — no `grep`.

## How to walk (3 paths)

1. **Newcomer:** `README.md` → `docs/plan/01-vision-and-scope.md` → `docs/plan/MAP.md` pick 31 rows → `CONTEXT.md` → `src/app/page.tsx` → history: `CUTOVER.md` + `docs/plan/ARCHIVE.md` → `git checkout backup/rust-2026-09-02`.
2. **Term:** `docs/plan/glossary.md` → `CONTEXT.md` (46 terms).
3. **History:** `CUTOVER.md` + `docs/plan/ARCHIVE.md` → `backup` + `rust-archive-2026-09-02` + `cc0887d`.

## Links

- Map: `docs/plan/MAP.md` + `docs/plan/01-vision-and-scope.md`
- Sales: `docs/plan/02-capabilities/03-finance-sales-pos/sales.md` → `docs/plan/03-cross-cutting/@-and-ai/sales.md` + `_shared-contract.md`
- Routes: `docs/contracts/next/route-table.md`
- Decisions: `docs/adr/ADR-020` + `docs/adr/`
- Archive: `docs/plan/ARCHIVE.md` + `backup`
- Code: `src/app/page.tsx`

## Notes

- **Refactored 2026-09-02:** 31 deep 213→60L (6150→2156L), `MAP.md` 31 rows, `_shared-contract.md` 109L, `docs/plan`+`docs/adr` under `docs/`, 13 group READMEs removed (MAP covers), `Plan→docs/plan` + `adr→docs/adr`.
- The 7 contradictions (second cutover, protobuf vs pruned, 558 dead links) resolved in `CUTOVER.md`, `CONTRACT_POLICY.md`, `docs/plan/traceability.md`.
