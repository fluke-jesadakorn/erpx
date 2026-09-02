# docs/contracts/next — Route Table (Living, Next.js)

> **Parallel file — new.** Living counterpart to archived `Plan/traceability.md` (552 Rust RPCs). This table will grow as `src/app/api/*` routes are implemented. Each row must also update `CONTRACT_POLICY.md` + Prisma/Drizzle schema + `Plan/traceability.md` living section (ADR-020, Q18=A). Proto on `backup/rust-2026-09-02` (`sales/service.proto` etc) is archived — see `docs/contracts/README.md` + `Plan/LEGACY.md` + `CUTOVER.md:2026-09-02`.

## Routes (initial — 1 example, rest TBD)

| Route | Method | Context | Spec (light → deep) | Proto (archived) | Status | Implemented |
|---|---|---|---|---|---|---|
| `/api/sales/quotes` | POST | sales | `Plan/02-capabilities/03-finance-sales-pos/sales.md:9 CreateQuote` → `Plan/03-cross-cutting/@-and-ai/sales.md:15` | `sales/service.proto:CreateQuote` | Living (TBD) | — |
| `/api/sales/quotes/:id/send` | POST | sales | `02/sales.md:9 SendQuote` → `03/sales.md:15` | `sales/service.proto:SendQuote` | Living (TBD) | — |
| `/api/sales/quotes/:id/accept` | POST | sales | `02/sales.md:9 AcceptQuote` → `03/sales.md:15` | `sales/service.proto:AcceptQuote` | Living (TBD) | — |
| `/api/sales/orders` | POST | sales | `02/sales.md:9 CreateSalesOrder` → `03/sales.md:15` | `sales/service.proto:CreateSalesOrder` | Living (TBD) | — |
| `/api/finance/journals` | POST | finance | `02/finance.md:9 CreateJournal` → `03/finance.md:15` | `finance/service.proto:CreateJournal` | Living (TBD) | — |
| `/api/finance/periods/:id/close` | POST | finance | `02/finance.md:9 ClosePeriod` → `03/finance.md:15` | `finance/service.proto:ClosePeriod` (HumanDecisionProof) | Living (TBD) | — |
| `/api/pos/sales` | POST | pos | `02/pos.md:9 CreateSale` → `03/pos.md:15` | `pos/service.proto:CreateSale` | Living (TBD) | — |
| `/api/commercial/catalog-plans` | POST | commercial | `02/commercial.md:9 CreateCatalogPlan` → `03/commercial.md:15` | `commercial/service.proto:CreateCatalogPlan` | Living (TBD) | — |

Add rows as `src/app/api/<ctx>/*` handlers land. Keep this file in sync with `Plan/MAP.md` and `Plan/traceability.md` living section.

## Conventions

- **Idempotency:** every command carries `Idempotency-Key` + authoritative tenant/holding scope (from `CONTRACT_POLICY.md`).
- **Versioned namespace:** `/api/v1/...` or `trpc.<ctx>.procedure` — never reuse route/procedure name with different meaning; removed fields stay reserved.
- **Verification:** `npm run lint` + `npm run build` must pass (`src/app` typecheck). Archived Rust gate `cargo xtask verify` stays on `backup/rust-2026-09-02`.

## Links

- Archive RPC table: `Plan/traceability.md` (banner Archived — Rust era)
- Spec hub: `docs/TOC.md` → `Plan/MAP.md` (31 contexts) → `Plan/01-vision-and-scope.md` (spine)
- Policy: `CONTRACT_POLICY.md` + `adr/ADR-020-nextjs-stack.md`
