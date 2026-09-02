# 03-finance-sales-pos — Ledger + quotes/orders→posting + terminal sales

> **Parallel file — new.** Group 03-finance-sales-pos index. Value stream purpose: Ledger + quotes/orders→posting + terminal sales — Finance is first production-real vertical.

**Journey:** Revenue: crm opportunity → sales quote→order → pos sale → finance journal/payment → period close (Journey 1)

| Context | Purpose (1-line) | Light | Deep | Proto (archived) | Next route |
|---|---|---|---|---|---|
| finance | Chart/subledgers/banking/budget/tax/period close — 31 RPCs, 7 invariants, Finance-First | `finance.md` | `../../03-cross-cutting/@-and-ai/finance.md` | `finance/service.proto` | `src/app/api/finance/*` TBD |
| sales | Quotes/orders → finance posting — 13 RPCs, SALES_QUOTE_FIELDS OwnerPublished | `sales.md` | `../../03-cross-cutting/@-and-ai/sales.md` | `sales/service.proto` | `src/app/api/sales/*` TBD |
| pos | Point-of-sale sessions/refunds — 16 RPCs, Refund bounded | `pos.md` | `../../03-cross-cutting/@-and-ai/pos.md` | `pos/service.proto` | `src/app/api/pos/*` TBD |

**Map:** `Plan/MAP.md` (flat index 31) → `docs/TOC.md` (hub) → `Plan/01-vision-and-scope.md` (spine)

Light: `Plan/02-capabilities/03-finance-sales-pos/<ctx>.md` (26-35L, domain invariants + precise Data owned)
Deep: `Plan/03-cross-cutting/@-and-ai/<ctx>.md` (→ trimmed ≤60L delta, see `_shared-contract.md`)
Archive: `git checkout backup/rust-2026-09-02 -- docs/contracts/proto/erpx/<ctx>/service.proto`