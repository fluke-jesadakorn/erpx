# ERPX Docs — TOC (single hub)

> Start at `README.md` → here. Replaces `.scratch/docs-seamless/prototype/TOC-seamless.md`.

## Tree (pruned 2026-09-02)

```
README.md · AGENTS.md+CLAUDE.md · CUTOVER.md (2026-09-02 + 2026-07-31) · CONTEXT.md (46 terms) · CONTRACT_POLICY.md
docs/plan/ 31 contexts: README · 01-vision · MAP 31 flat · 02/<group>/<ctx>.md 31 one-pagers · 03/@-and-ai/ 31 deep 60L+_shared 109L · traceability 552 <details>+living route-table · ARCHIVE canonical
docs/adr/ 20 (15 Reconfirmed 3 Deprecated 1 Superseded 020 Next.js) · docs/TOC.md ←here · docs/README → ARCHIVE · docs/contracts/next/route-table.md living · docs/agents/ · docs/archive/ (delivery 768L + wayfinder 10 tickets + dioxus 22 dirs 3.9M + ops 68K)
Designed/mockup/ clickable prototype (feed+31 contexts, from docs) · audits/ stub→archive · src/app/ (layout/page) · .agents/skills/ 6 · .github/nextjs.yml living · fixtures/ops/ runbooks
```

## Sales — 1 hop

`docs/plan/02/03-finance-sales-pos/sales.md` 26L 13 RPCs · `03/@-and-ai/sales.md` 69L · `MAP.md` row `sales|03|POST /api/sales/quotes|@sales/quote-2026-042` · living `docs/contracts/next/route-table.md` · archived `backup:crates/proto/sales/service.proto`

`README.md` → `docs/TOC.md` → `MAP.md` search `sales` → `02/sales.md`+`03/sales.md`+`_shared-contract.md`

## Feed — 0 hops

`/feed` → Role Lane (needs-your-role) → Following (newest-first) → post → canonical record. Paradigm: `docs/plan/04-social-paradigm.md` (ADR-021).

## Walk (3 paths)

1. Newcomer: `README.md`→`docs/plan/01-vision`→`MAP.md` 31→`CONTEXT.md`→`src/app/page.tsx`→history `CUTOVER.md`+`ARCHIVE.md`→`git checkout backup/rust-2026-09-02`
2. Term: `glossary.md`→`CONTEXT.md`
3. History: `CUTOVER.md`+`ARCHIVE.md`→`backup` `rust-archive-2026-09-02` `cc0887d`

Links: `MAP.md` · `sales.md`→`@-and-ai/sales.md`+`_shared` · `route-table.md` · `ADR-020` · `ARCHIVE.md` · `src/app/page.tsx`

Notes: 31 deep 213→60L (6150→2156L) + MAP 31 + _shared 109L + Plan→docs/plan 13 READMEs removed; 7 contradictions resolved in `CUTOVER.md` `CONTRACT_POLICY.md` `traceability.md`.
