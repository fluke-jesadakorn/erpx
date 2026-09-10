# ERPX Docs — TOC (single hub)

> Start at `README.md` → here. Replaces `.scratch/docs-seamless/prototype/TOC-seamless.md`.

## Workplace simplification — 2026-09-08

- [Work Cards](plan/2026-09-08-work-cards.md): permitted shortcuts, personal pin order, shared summaries, scoped preferences and verification.
- [Shared workspace and employee services](plan/2026-09-08-shared-workspace.md): combined responsibilities, service teams, requests, purchasing handoff, handbook, AI preparation and verification.

- [Expense workflow, AI providers and IT access](plan/2026-09-08-expenses-ai-it.md): v2 routing, draft recovery, provider encryption, document preparation, rollout and verification.
- [Navigation, Administration and daily-work delivery](plan/2026-09-08-workplace-simplification.md): canonical management surfaces, role boundaries, compatibility and verification artifacts.

## Global core implementation — 2026-09-07

- [Implementation status and full module inventory](plan/2026-09-07-global-core-status.md): implemented flows, test evidence and remaining production work.
- [Local data, accounts and acceptance journeys](local-seed.md): realistic fictional roster and connected development scenarios.
- [ERP workflow research](plan/2026-09-06-production-workflows-research.md): primary sources and acceptance requirements.

## Mockup implementation plan — 2026-09-06

- [Enterprise UX/UI delivery](plan/2026-09-06-enterprise-ui.md): complete 31-module / 171-capability interface, live-versus-preview boundaries, local review, and verification.
- [Mockup → Next.js/Bun/Cloudflare implementation plan](plan/2026-09-06-mockup-to-fullstack.md): accepted architecture, current implementation checkpoint, 31-module rollout, Finance acceptance, R2 lifecycle and delivery gates.
- [ADR-022 Cloudflare runtime](adr/ADR-022-cloudflare-runtime.md): Workers/OpenNext, D1, R2, Queues and server-policy decision.
- [ADR-023 workspace routing](adr/ADR-023-three-level-workspace-routing.md): three-level Platform/Holding/Local authorization and immutable canonical slugs.
- [Superseded local stack research](plan/2026-09-06-local-stack-research.md): retained research history for the abandoned PostgreSQL/MinIO direction.

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

- [Staff requests, levels, task approvals and routine schedules](contracts/next/staff-work.md) — interfaces, authorization, calendar rules and migration order.
- [Social + AI public beta](plan/2026-09-07-social-ai-public-beta.md) — primary research, approved conversation-to-completion scope, pilot hypotheses, and pending verification gates.

- [Local Mac deployment and recovery](../ops/macos/README.md) — active runtime (ADR-024).
