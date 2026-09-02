# Legal — Deep `@` + AI (Production)

**Group:** `05-people-legal` · **Code:** `crates/contexts/legal` · **Proto:** `crates/proto/proto/erpx/legal/service.proto` (`LegalCommandService, LegalQueryService` · 12 RPCs) · **Adapters:** `crates/adapters/legal-sqlx` · **UI:** `apps/console/src/features/legal` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `Plan/02-capabilities/05-people-legal/legal.md`. Central polymorphic `@` contract in `Plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (12)

- **Services:** LegalCommandService, LegalQueryService
- **RPCs:** ** CreateContract, AddContractParty, TransitionContract, CreateDocument, AddDocumentVersion, ArchiveDocument, RecordDocumentAccess, GetContract, ListContracts, GetDocument, ListDocuments, ListAccessHistory

| RPC | Service | Data touch |
|-----|---------|------------|
| `** CreateContract` | Command/Query | `legal` aggregate + outbox |
| `AddContractParty` | Command/Query | `legal` aggregate + outbox |
| `TransitionContract` | Command/Query | `legal` aggregate + outbox |
| `CreateDocument` | Command/Query | `legal` aggregate + outbox |
| `AddDocumentVersion` | Command/Query | `legal` aggregate + outbox |
| `ArchiveDocument` | Command/Query | `legal` aggregate + outbox |
| `RecordDocumentAccess` | Command/Query | `legal` aggregate + outbox |
| `GetContract` | Command/Query | `legal` projection |
| `ListContracts` | Command/Query | `legal` projection |
| `GetDocument` | Command/Query | `legal` projection |
| `ListDocuments` | Command/Query | `legal` projection |
| `ListAccessHistory` | Command/Query | `legal` projection |

## Invariants (domain — keep)

- Legal holds propagate via typed edges; deletions respect holds.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `legal` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `legal holds` (light `legal.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** CreateContract → AddParty → Transition (draft→active) + document versioning + access audit — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/legal + /work` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title LEGAL, list legal-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/legal/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/legal/src/domain.rs`
- **Adapters:** `legal-sqlx`
- **UI:** `apps/console/src/features/legal`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `legal`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `Plan/MAP.md` for flat index.
