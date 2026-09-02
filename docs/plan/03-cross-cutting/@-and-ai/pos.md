# POS — Deep `@` + AI (Production)

**Group:** `03-finance-sales-pos` · **Code:** `crates/contexts/pos` · **Proto:** `crates/proto/proto/erpx/pos/service.proto` (`PointOfSaleCommandService, PointOfSaleQueryService` · 16 RPCs) · **Adapters:** `crates/adapters/pos-sqlx` · **UI:** `apps/console/src/features/pos`

**Scope:** Extends `docs/plan/02-capabilities/03-finance-sales-pos/pos.md`. Central polymorphic `@` contract applies verbatim — see `_shared-contract.md` for shared @/AI/Forms/Verify/Spine/ASCII frame.

## Services & RPCs (16)

- **Services:** PointOfSaleCommandService, PointOfSaleQueryService
- **RPCs:** CreateSale, OpenSale, HoldSale, ResumeSale, RecordPayment, CompleteSale, VoidSale, RequestRefund, ApproveRefund, RejectRefund, PostRefund, CancelRefund, GetSale, ListSales, GetRefund, ListRefunds

| RPC | Data touch |
|-----|------------|
| `CreateSale` | `pos` aggregate + outbox |
| `OpenSale` | `pos` aggregate + outbox |
| `RecordPayment` | `pos` aggregate + outbox |
| `PostRefund` | `pos` aggregate + outbox |
| `GetSale` | `pos` projection |
| `ListSales` | `pos` projection |

## Invariants (domain)

- Refund is bounded by original settlement; refund state machine is append-only.
- Generic 3 (holding-isolation/time-bounded/append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `pos_* + pos_refund` aggregate + `outbox_events` in holding pair (light `pos.md:16` canonical, fixes generic)
- Read: rebuildable projection; reference graph see `_shared-contract.md`

## Editable fields

- **This module:** `OwnerUnsupported (spec-ahead)` — no owner-published schema
- Binding: see `_shared-contract.md`

## Business flows (unique)

1. **Happy:** Open terminal session → scan lines → payment → receipt — creates typed edges.
2. **Exception:** Void + inventory reservation release — validates refund bounded.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/pos + /sales` + `/work + /reference/item/:id` — terminal sale flow
- **Spec row:** `— OwnerUnsupported (spec-ahead) no runtime schema`

## Traceability

- **Proto:** `crates/proto/proto/erpx/pos/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/pos/src/domain.rs`
- **Adapters:** `pos-sqlx`
- **UI:** `apps/console/src/features/pos`
- **Verify:** see `_shared-contract.md`

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md`.
