# Operations — Deep `@` + AI (Production)

**Group:** `04-supply-chain-ops` · **Code:** `crates/contexts/operations` · **Proto:** `crates/proto/proto/erpx/operations/service.proto` (`OperationsCommandService, OperationsQueryService` · 11 RPCs) · **Adapters:** `crates/adapters/operations-sqlx` · **UI:** `apps/console/src/features/operations` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/04-supply-chain-ops/operations.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (11)

- **Services:** OperationsCommandService, OperationsQueryService
- **RPCs:** ** ReassignException, AcknowledgeException, EscalateException, DecideApproval, ResolveException, CloseException, PublishWorkQueuePolicy, GetException, ListExceptionQueue, ListWorkQueuePolicies, GetSummary

| RPC | Service | Data touch |
|-----|---------|------------|
| `** ReassignException` | Command/Query | `operations` aggregate + outbox |
| `AcknowledgeException` | Command/Query | `operations` aggregate + outbox |
| `EscalateException` | Command/Query | `operations` aggregate + outbox |
| `DecideApproval` | Command/Query | `operations` aggregate + outbox |
| `ResolveException` | Command/Query | `operations` aggregate + outbox |
| `CloseException` | Command/Query | `operations` aggregate + outbox |
| `PublishWorkQueuePolicy` | Command/Query | `operations` aggregate + outbox |
| `GetException` | Command/Query | `operations` projection |
| `ListExceptionQueue` | Command/Query | `operations` projection |
| `ListWorkQueuePolicies` | Command/Query | `operations` projection |
| `GetSummary` | Command/Query | `operations` projection |

## Invariants (domain — keep)

- Operations aggregates own completion; cross-context updates via domain events.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `operations` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `operations_* + completion evidence` (light `operations.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Shift handover + production log — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/operations + /work` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title OPERATIONS, list operations-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/operations/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/operations/src/domain.rs`
- **Adapters:** `operations-sqlx`
- **UI:** `apps/console/src/features/operations`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `operations`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
