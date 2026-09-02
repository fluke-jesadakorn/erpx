# Support

**Purpose:** Owns support tickets/sessions, capabilities, SLA, attachments.

**Group:** `02-customer-support` · **Code:** `crates/contexts/support` · **Proto:** `crates/proto/proto/erpx/support/service.proto` · **Adapters:** `crates/adapters/support-sqlx` (where present) · **UI:** `apps/console/src/features/support` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (21):** SupportCommandService, SupportQueryService
- OpenTicket, TransitionTicket, AddCustomerReply, AddInternalNote, RequestSession, ApproveSession, RevokeSession, IssueCapability, RevokeCapability, OpenIncident, TransitionIncident, AddIncidentTimeline …

**Invariants:**
- Support tickets/sessions are customer-scoped with capability-gated access and SLA clocks.


**Data owned:** `support_tickets/support_sessions + support_capabilities + incidents`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@support/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/support`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/support.md`.
