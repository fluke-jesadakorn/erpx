# Marketing

**Purpose:** Owns marketing content/journeys/delivery with provider scoping.

**Group:** `06-marketing-crm` · **Code:** `crates/contexts/marketing` · **Proto:** `crates/proto/proto/erpx/marketing/service.proto` · **Adapters:** `crates/adapters/marketing-sqlx` (where present) · **UI:** `apps/console/src/features/marketing` / `crates/ui-domains/*`

**Actors & scopes:** `AuthorityScope::{Platform, Holding, Company, Branch, Subtree}` via `crates/kernel`; RBAC through expiring `role_assignments` (ADR-004) where context is gated.

**Services & RPCs (18):** MarketingCommandService, MarketingQueryService
- SubmitMarketingForm, EraseMarketingProfile, IssueMarketingPreview, RevokeMarketingPreview, IssueMarketingDomainVerification, VerifyMarketingDomain, BindMarketingDomain, ConfigureMarketingProviderAccount, AcknowledgeMarketingProviderVerification, SetMarketingProviderAccountState, CreateLeadCandidate, CaptureEngagement …

**Invariants:**
- Marketing delivery is provider-scoped with abuse windows and scheduler claims.


**Data owned:** `marketing_* + delivery scheduler + abuse windows`

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins).

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@marketing/example-28-01-2026-0001`.



**Verification:** `cargo xtask test-affected` (owns `crates/contexts/marketing`), `cargo xtask verify` for full gate. Traceability: `docs/plan/traceability.md`.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/marketing.md`.
