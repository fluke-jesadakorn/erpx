# Legal

**Purpose:** Owns legal holds and entities.

**Group:** `05-people-legal` · **Code:** `crates/contexts/legal` · **Proto:** `crates/proto/proto/erpx/legal/service.proto` · **Adapters:** `crates/adapters/legal-sqlx` (where present) · **UI:** `apps/console/src/features/legal` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (12):** LegalCommandService, LegalQueryService
- CreateContract, AddContractParty, TransitionContract, CreateDocument, AddDocumentVersion, ArchiveDocument, RecordDocumentAccess, GetContract, ListContracts, GetDocument, ListDocuments, ListAccessHistory

**Invariants:**
- Legal holds propagate via typed edges; deletions respect holds.


**Data owned:** `legal holds`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@legal/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/legal`), `cargo xtask verify` for full gate. Traceability: `Plan/traceability.md`.

ASCII: see `Plan/03-cross-cutting/@-and-ai/legal.md`.
