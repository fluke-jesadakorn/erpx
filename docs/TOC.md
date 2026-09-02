# ERPX Docs — Single Hub (TOC)

> **One hub for everything.** Start at `README.md`, land here, then follow the walk that fits you. This file replaces the prototype in `.scratch/docs-seamless/prototype/TOC-seamless.md`.

## Tree — 3 layers (after 2026-09-02 prune)

```
README.md                           # entry — Next.js reboot 2026-09-02, recovery: backup/rust-2026-09-02 / tag rust-archive-2026-09-02 / cc0887d
├── AGENTS.md + CLAUDE.md           # @AGENTS.md — tooling (Node 20+ / npm) + skills; fences at top/bottom are machine-owned
├── CUTOVER.md                      # 2 entries: 2026-09-02 Rust→Next.js (new, on top) + 2026-07-31 Folio→Rust (history)
├── CONTEXT.md                      # canonical glossary (46 terms) — implementation-free, single source for every term
├── CONTRACT_POLICY.md              # Next.js policy: REST/tRPC + Prisma/Drizzle (protobuf era archived on backup)
├── Plan/                           # business plan — 31 contexts, living spec
│   ├── README.md                   # Code is truth → archived contracts on backup + living src/app
│   ├── 01-vision-and-scope.md      # vision, tiers (Local → Self-hosted → Hosted), @-mention contract + spine
│   ├── MAP.md                      # **flat index 31 contexts** — find any context without remembering its group number (e.g., sales = 03)
│   ├── 02-capabilities/<group>/README.md # **group indexes ×13** — e.g., 03 Finance/Sales/POS lists finance/sales/pos with Purpose 1-line
│   ├── 02-capabilities/<group>/<context>.md  # 13 groups / 31 contexts — lightweight one-pagers (26-35L, domain invariants + precise Data owned)
│   ├── 03-cross-cutting/@-and-ai/  # 33 deep specs per context (→ trimmed ≤60L delta, see _shared-contract.md for @/AI/Forms/Verify/Spine/ASCII frame)
│   │   ├── _shared-contract.md     # **single source** for 150L template duplicated 31× (tokenizer, AI lifecycle, Forms, Spine)
│   │   └── <ctx>.md                # per-module delta: RPC table (where different) + Editable fields + Happy/Exception 1-line + Route
│   ├── traceability.md             # Archived Rust RPC table (banner) — living Next.js routes in docs/contracts/next/route-table.md
│   ├── glossary.md                 # 5-line pointer → CONTEXT.md
│   └── LEGACY.md                   # stub → backup for pruned baselines (Plan.legacy-* removed 2026-09-02)
├── adr/                            # 20 ADRs: 15 Reconfirmed ✅ + 3 Deprecated (010,014,016) + 1 Superseded (002→020) + 020 Next.js stack
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
├── audits/                         # after prune 330→22, 72M→3.9M (see audits/README.md)
│   ├── README.md                   # index — before/after + keep-list (22 dirs)
│   ├── CURRENT-CERTIFICATION.md    # Dioxus cert (archived) + Next.js gate pointer
│   └── (keep) DIOXUS-CUTOVER*, ADR-017-PHASES-0-6, e2e-all-modules, finance_darkmode_fixed, finance_live_full …
├── fixtures/ + ops/                # fixtures (ai/* 4 json, migration) + runbooks (ops/runbooks/* Deprecated)
├── src/app/                        # living Next.js — layout.tsx, page.tsx, globals.css, future route handlers
├── .agents/skills/                 # canonical 6 skills (domain-modeling, grilling, prototype, research, wayfinder)
├── .github/workflows/
│   ├── nextjs.yml                  # living CI — npm run lint + build
│   └── erpx-targets.yml            # Deprecated — Rust CI, kept for history
└── .gitignore                      # Next.js + legacy Rust ignores
```

## Sales — Quick Jump (1 hop)

> **“How sales works” in 1 hop:** No need to remember `03-finance-sales-pos` + hunt 213L deep file.

- **Light:** `Plan/02-capabilities/03-finance-sales-pos/sales.md` (26L, 13 RPCs: CreateQuote→Fulfill, Quote→order→posting invariants) — 5-section scaffold, `ASCII: see Plan/03-cross-cutting/@-and-ai/sales.md`
- **Group index:** `Plan/02-capabilities/03-finance-sales-pos/README.md` — lists finance(31 RPCs)/sales(13)/pos(16) with Purpose 1-line + link 02+03
- **Deep (trimmed):** `Plan/03-cross-cutting/@-and-ai/sales.md` (213L → ≤60L delta) — RPC table 13 rows + `SALES_QUOTE_FIELDS: quote.notes Text 2,048 / payment_terms_days 0-365 / valid_until DateTime / language en/th` (OwnerPublished, only sales+procurement) + Happy `CreateQuote→Send→Accept→CreateSalesOrder→Fulfill` + Route `/sales + /work + /reference/item/:id` — shared @/AI/Forms/Verify/Spine/ASCII frame now in `_shared-contract.md`
- **Flat index:** `Plan/MAP.md` — row `sales | 03-finance-sales-pos | sales.md | @-and-ai/sales.md | sales/service.proto (archived) | POST /api/sales/quotes TBD | @sales/quote-2026-042`
- **Living route (TBD):** `docs/contracts/next/route-table.md` — `POST /api/sales/quotes`, `POST /api/sales/quotes/:id/send`, `POST /api/sales/orders` (grows as `src/app/api` lands)
- **Archived proto:** `docs/contracts/README.md` lists `sales/service.proto` among 37 — code on `backup/rust-2026-09-02` (`crates/proto/proto/erpx/sales/service.proto` + `crates/contexts/sales/src/domain.rs` + `audits/G9-SALES-*` deleted `fe6c286e`, see `audits/README.md: G9-SALES-* archived on backup`)
- **Design:** `Designed/README.md` — 27 PNGs + routes now includes `/sales (/sales, /sales/:id), /work` — single source `MainDesign.pen` 15M (Pencil MCP only)
- **Spine:** `Plan/01-vision-and-scope.md:52` + `Plan/03-cross-cutting/@-and-ai/_journeys.md:13` Revenue `crm/opportunity-2026-011 → sales/quote-2026-042 → sales/order-2026-042 → finance/journal-2026-117 → finance/payment`

**One-liner:** `README.md` → `docs/TOC.md` (you are here) → `Plan/MAP.md` search `sales` → `02/sales.md` + `03/sales.md` (delta) + `_shared-contract.md` for @/AI — no `grep` needed.

## How to walk the docs (3 paths)

1. **Newcomer:** `README.md` → `Plan/01-vision-and-scope.md` → `Plan/MAP.md` pick any of 31 rows you care about (or `Plan/02-capabilities/<group>/README.md` for value-stream) → `CONTEXT.md` for terms → `src/app/page.tsx` (code) → if you need history: `CUTOVER.md` + `Plan/LEGACY.md` + `docs/README.md` → `git checkout backup/rust-2026-09-02`.

2. **Find a term:** every `Plan/*/glossary.md` points here → `CONTEXT.md` is the only canonical glossary (46 terms, no implementation).

3. **Find history:** `CUTOVER.md` (2 entries) + `Plan/LEGACY.md` + `docs/README.md` → `backup/rust-2026-09-02` + tag `rust-archive-2026-09-02` + baseline `cc0887d` + `f97b74d6` archive point.

## Links

- Entry: `../README.md`
- Map: `../Plan/MAP.md` (flat 31) + `../Plan/01-vision-and-scope.md` (spine)
- Business plan: `../Plan/README.md` → `../Plan/02-capabilities/03-finance-sales-pos/README.md` (sales group example) → `../Plan/02-capabilities/03-finance-sales-pos/sales.md` → `../Plan/03-cross-cutting/@-and-ai/sales.md`
- Shared contract: `../Plan/03-cross-cutting/@-and-ai/_shared-contract.md`
- Living routes: `contracts/next/route-table.md` + `contracts/next/README.md`
- Glossary: `../CONTEXT.md`
- Decisions: `../adr/ADR-020-nextjs-stack.md` (living) + `../adr/`
- Archive: `README.md` + `contracts/README.md` + `schema/README.md` + `archive/`
- Design: `../Designed/README.md` (single source) + `../Designed/MainDesign.pen` (Pencil MCP only)
- Evidence: `../audits/README.md` → `../audits/CURRENT-CERTIFICATION.md`
- Code: `../src/app/page.tsx` + `../src/app/layout.tsx`
- Cutover: `../CUTOVER.md`

## Notes

- **Refactored 2026-09-02:** `Plan/MAP.md` + 13 group READMEs + `_shared-contract.md` (shared 150L) + 4 deep trimmed (213L→60L, sales/finance/pos/commercial as example, rest 27 to follow same pattern) + `docs/contracts/next/route-table.md` (living) resolve 9 hops → 1 hop for sales, 62-file duplication, group-number indirection, Archive vs Living confusion. See `docs/archive/wayfinder-2026-09-02/map.md` for decisions 02-10 and `Plan/MAP.md` for full index.
- No file is duplicated in two places anymore (legacy dirs deleted, `audits/` pruned 330→22, contracts split archive/living, skills single-homed at `.agents/skills/`).
- The 7 critical contradictions (second cutover invisible, protobuf vs pruned, 558 dead links) are resolved by banners in `CUTOVER.md`, `CONTRACT_POLICY.md`, `Plan/traceability.md`, and this hub.
- If you came from `.scratch/docs-seamless/prototype/TOC-seamless.md` — that was the throwaway prototype; this `docs/TOC.md` is the living one.
