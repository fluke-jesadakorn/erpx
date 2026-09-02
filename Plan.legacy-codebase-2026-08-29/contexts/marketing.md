# Context — marketing

**Crate:** `crates/contexts/marketing` · **Proto:** `crates/proto/proto/erpx/marketing/service.proto` · **Adapters:** `crates/adapters/marketing-sqlx` (where present) · **UI:** `apps/console/src/features/marketing` / `crates/ui-domains/*` / `crates/console-features/marketing`

## Services & RPCs

SubmitMarketingForm, EraseMarketingProfile, IssueMarketingPreview, RevokeMarketingPreview, IssueMarketingDomainVerification, VerifyMarketingDomain, BindMarketingDomain, ConfigureMarketingProviderAccount, AcknowledgeMarketingProviderVerification, SetMarketingProviderAccountState, CreateLeadCandidate, CaptureEngagement, QualifyLeadCandidate, RejectLeadCandidate, GetMarketingDeliveryRead, ListMarketingContentReadRows, GetLeadCandidate, ListLeadCandidates

Total RPCs: 18.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/marketing-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/marketing/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/marketing` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
