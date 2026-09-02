# Collaboration

**Purpose:** Owns universal threads/comments/reactions/following/mentions/action cards.

**Group:** `13-ai-collaboration` · **Code:** `crates/contexts/collaboration` · **Proto:** `crates/proto/proto/erpx/collaboration/service.proto` · **Adapters:** `crates/adapters/collaboration-sqlx` (where present) · **UI:** `apps/console/src/features/collaboration` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (26):** CollaborationCommandService, CollaborationQueryService, CollaborationStreamService
- CreateThread, AddComment, EditComment, TombstoneComment, TransitionThread, SetReaction, SetFollowing, WithdrawMention, ModerateComment, ProposeActionCard, ConfirmActionCard, AddRecordRelation …

**Invariants:**
- Comments/reactions/following are append-only with moderation tombstones; mentions are typed.


**Data owned:** `collaboration_threads/comments/reactions/mentions (0003..0006)`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@collaboration/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/collaboration`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/collaboration.md`.
