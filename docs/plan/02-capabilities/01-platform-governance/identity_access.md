# Identity Access

**Purpose:** Owns principals, roles, grants, expiring assignments, access reviews, break-glass.

**Group:** `01-platform-governance` · **Code:** `crates/contexts/identity_access` · **Proto:** `crates/proto/proto/erpx/identity_access/service.proto` · **Adapters:** `crates/adapters/identity_access-sqlx` (where present) · **UI:** `apps/console/src/features/identity_access` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (34):** IdentityAccessCommandService, IdentityAccessQueryService
- SaveDirectoryConnector, StartDirectoryImport, SaveDirectoryRoleMapping, PreflightDirectoryConnector, TransitionDirectoryConnectorLifecycle, LinkDirectoryPrincipal, UnlinkDirectoryPrincipal, ReconcileDirectoryConnector, ProposeDirectoryIdentityReplacement, ApproveDirectoryIdentityReplacement, RejectDirectoryIdentityReplacement, RenewRole …

**Invariants:**
- All RBAC assignments expire (`valid_from/until`); assignments are parent-bounded and audited.


**Data owned:** `role_assignments + role_review_* + audit_events + sod_decisions`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@identity_access/example-28-01-2026-0001`.



## Social surface (paradigm: `docs/plan/04-social-paradigm.md`)

- **Post template:** `{principal} · {role} · expires {date}` — directory internals stay private; grants are the post.
- **Hashtags:** `#access #roles` (restricted audience).
- **Follow:** own grants + reviews; **Lane:** expiring <7d + break-glass for `security` role.

**Verification:** `cargo xtask test-affected` (owns `crates/contexts/identity_access`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/identity_access.md`.
