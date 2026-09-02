# Context — manufacturing

**Crate:** `crates/contexts/manufacturing` · **Proto:** `crates/proto/proto/erpx/manufacturing/service.proto` · **Adapters:** `crates/adapters/manufacturing-sqlx` (where present) · **UI:** `apps/console/src/features/manufacturing` / `crates/ui-domains/*` / `crates/console-features/manufacturing`

## Services & RPCs

PublishSpecification, CreateOrder, ReleaseOrder, ReserveMaterial, StartOrder, IssueMaterial, ReportLabor, ReportResult, ApplyQualityDisposition, RecordCost, CompleteOrder, CloseOrder, GetSpecification, GetOrder, ListOrders, GetGenealogy, GetWipCost, GetRecord

Total RPCs: 18.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` present).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/manufacturing-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/manufacturing/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/manufacturing` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
