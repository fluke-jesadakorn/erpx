# Marketing — Deep `@` + AI (Production)

**Group:** `06-marketing-crm` · **Code:** `crates/contexts/marketing` · **Proto:** `crates/proto/proto/erpx/marketing/service.proto` (`MarketingCommandService, MarketingQueryService` · 18 RPCs) · **Adapters:** `crates/adapters/marketing-sqlx` · **UI:** `apps/console/src/features/marketing` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/06-marketing-crm/marketing.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (18)

- **Services:** MarketingCommandService, MarketingQueryService
- **RPCs:** ** SubmitMarketingForm, EraseMarketingProfile, IssueMarketingPreview, RevokeMarketingPreview, IssueMarketingDomainVerification, VerifyMarketingDomain, BindMarketingDomain, ConfigureMarketingProviderAccount, AcknowledgeMarketingProviderVerification, SetMarketingProviderAccountState, CreateLeadCandidate, CaptureEngagement, QualifyLeadCandidate, RejectLeadCandidate, GetMarketingDeliveryRead, ListMarketingContentReadRows, GetLeadCandidate, ListLeadCandidates

| RPC | Service | Data touch |
|-----|---------|------------|
| `** SubmitMarketingForm` | Command/Query | `marketing` aggregate + outbox |
| `EraseMarketingProfile` | Command/Query | `marketing` aggregate + outbox |
| `IssueMarketingPreview` | Command/Query | `marketing` aggregate + outbox |
| `RevokeMarketingPreview` | Command/Query | `marketing` aggregate + outbox |
| `IssueMarketingDomainVerification` | Command/Query | `marketing` aggregate + outbox |
| `VerifyMarketingDomain` | Command/Query | `marketing` aggregate + outbox |
| `BindMarketingDomain` | Command/Query | `marketing` aggregate + outbox |
| `ConfigureMarketingProviderAccount` | Command/Query | `marketing` aggregate + outbox |
| `AcknowledgeMarketingProviderVerification` | Command/Query | `marketing` aggregate + outbox |
| `SetMarketingProviderAccountState` | Command/Query | `marketing` aggregate + outbox |
| `CreateLeadCandidate` | Command/Query | `marketing` aggregate + outbox |
| `CaptureEngagement` | Command/Query | `marketing` aggregate + outbox |

## Invariants (domain — keep)

- Marketing delivery is provider-scoped with abuse windows and scheduler claims.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `marketing` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `marketing_* + delivery scheduler + abuse windows` (light `marketing.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** marketing.content: MARKETING_CONTENT_FIELDS (marketing_content.title Text, marketing_content.body Text)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Lead candidate → Qualify → Content publish → Engagement capture — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Feed mapping (paradigm: `docs/plan/04-social-paradigm.md`)

- **Emit:** campaigns→posts; verifications/scheduler claims→lane (`marketing-ops`); engagement→post updates.
- **Comments:** open; provider secrets never in text.

## Route & UI delta

- **Route:** `/marketing + /public/marketing/form/:id` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title MARKETING, list marketing-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/marketing/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/marketing/src/domain.rs`
- **Adapters:** `marketing-sqlx`
- **UI:** `apps/console/src/features/marketing`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `marketing`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
