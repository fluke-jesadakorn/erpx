# docs/contracts/next — Route Table (Living, Next.js)

> Living counterpart to `docs/plan/traceability.md` (552 Rust RPCs archived on `backup/rust-2026-09-02`, see `docs/plan/ARCHIVE.md` + `CUTOVER.md`). Grows as `src/app/api/*` lands.

## Routes (initial — 1 example, rest TBD)

| Route | Method | Context | Spec (light → deep) | Proto (archived) | Status | Implemented |
|---|---|---|---|---|---|---|
| `/api/sales/quotes` | POST | sales | `docs/plan/02-capabilities/03-finance-sales-pos/sales.md:9 CreateQuote` → `docs/plan/03-cross-cutting/@-and-ai/sales.md:15` | `sales/service.proto:CreateQuote` | Living (TBD) | — |
| `/api/sales/quotes/:id/send` | POST | sales | `02/sales.md:9 SendQuote` → `03/sales.md:15` | `sales/service.proto:SendQuote` | Living (TBD) | — |
| `/api/sales/quotes/:id/accept` | POST | sales | `02/sales.md:9 AcceptQuote` → `03/sales.md:15` | `sales/service.proto:AcceptQuote` | Living (TBD) | — |
| `/api/sales/orders` | POST | sales | `02/sales.md:9 CreateSalesOrder` → `03/sales.md:15` | `sales/service.proto:CreateSalesOrder` | Living (TBD) | — |
| `/api/finance/journals` | POST | finance | `02/finance.md:9 CreateJournal` → `03/finance.md:15` | `finance/service.proto:CreateJournal` | Living (TBD) | — |
| `/api/finance/periods/:id/close` | POST | finance | `02/finance.md:9 ClosePeriod` → `03/finance.md:15` | `finance/service.proto:ClosePeriod` (HumanDecisionProof) | Living (TBD) | — |
| `/api/pos/sales` | POST | pos | `02/pos.md:9 CreateSale` → `03/pos.md:15` | `pos/service.proto:CreateSale` | Living (TBD) | — |
| `/api/commercial/catalog-plans` | POST | commercial | `02/commercial.md:9 CreateCatalogPlan` → `03/commercial.md:15` | `commercial/service.proto:CreateCatalogPlan` | Living (TBD) | — |
| `/feed` | GET | collaboration | `docs/plan/04-social-paradigm.md` Feed (newest-first + lanes) | `collaboration/service.proto:ListFeed` | Living (TBD) | — |
| `/today` | GET | collaboration | `04-social-paradigm.md` digest | `collaboration/service.proto:ListMyDay` | Living (TBD) | — |
| `/tags/:tag` | GET | collaboration | `04-social-paradigm.md` hashtags | `collaboration/service.proto:ListHashtagTopics` | Living (TBD) | — |
| `/posts/:id` | GET | collaboration | `04-social-paradigm.md` Post | `collaboration/service.proto:GetFeedItem` | Living (TBD) | — |
| `/notifications` | GET | collaboration | `04-social-paradigm.md` inbox | `collaboration/service.proto:ListMentionInbox` | Living (TBD) | — |

Add rows as `src/app/api/<ctx>/*` handlers land. Keep this file in sync with `docs/plan/MAP.md` and `docs/plan/traceability.md` living section.

## Conventions

- **Idempotency:** every command carries `Idempotency-Key` + authoritative tenant/holding scope (from `CONTRACT_POLICY.md`).
- **Versioned namespace:** `/api/v1/...` or `trpc.<ctx>.procedure` — never reuse route/procedure name with different meaning; removed fields stay reserved.
- **Verification:** `npm run lint` + `npm run build` must pass (`src/app` typecheck). Archived Rust gate `cargo xtask verify` stays on `backup/rust-2026-09-02`.

## Links

- Archive RPC table: `docs/plan/traceability.md` (banner Archived — Rust era)
- Spec hub: `docs/TOC.md` → `docs/plan/MAP.md` (31 contexts) → `docs/plan/01-vision-and-scope.md` (spine)
- Policy: `CONTRACT_POLICY.md` + `docs/adr/ADR-020-nextjs-stack.md`
