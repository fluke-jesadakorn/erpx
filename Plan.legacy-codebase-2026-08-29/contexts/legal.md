# Context — legal

**Crate:** `crates/contexts/legal` · **Proto:** `crates/proto/proto/erpx/legal/service.proto` · **Adapters:** `crates/adapters/legal-sqlx` (where present) · **UI:** `apps/console/src/features/legal` / `crates/ui-domains/*` / `crates/console-features/legal`

## Services & RPCs

CreateContract, AddContractParty, TransitionContract, CreateDocument, AddDocumentVersion, ArchiveDocument, RecordDocumentAccess, GetContract, ListContracts, GetDocument, ListDocuments, ListAccessHistory

Total RPCs: 12.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/legal-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/legal/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/legal` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
