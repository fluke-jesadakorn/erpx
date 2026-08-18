# Marketing and CRM Requirements

## Portfolio boundary

Marketing owns acquisition profiles, consent, suppression, audiences,
campaigns, budgets, content, journeys, delivery, and attribution. CRM owns
accepted leads and opportunities. Finance owns recognized revenue and Sales
owns quotes and orders.

## Marketing

### REQ-MKT-001 — Acquisition and consent authority

**Requirement.** ERPX shall maintain source, purpose, consent, suppression, and
privacy status for marketing profiles and engagement candidates.

**Rationale.** Marketing eligibility depends on provable permission and
purpose.

**Acceptance criteria.** Consent records subject, channel, purpose, source,
time, and version; absence or withdrawal denies affected delivery; suppression
overrides audience membership; identity matching does not rely solely on
mutable email or username.

**Sources.** [Marketing boundaries](../../adr/ADR-009-marketing-crm-boundaries.md),
[marketing domain](../../crates/domain/src/marketing.rs),
[Marketing API](../../crates/proto/proto/erpx/marketing/v2/service.proto).

### REQ-MKT-002 — Lead-candidate lifecycle

**Requirement.** ERPX shall manage marketing lead candidates through creation,
qualification, rejection, and governed handoff to CRM.

**Rationale.** Marketing qualification and sales opportunity ownership are
different stages.

**Acceptance criteria.** Candidate records source and qualification evidence;
only qualified current candidates can be handed off; handoff is idempotent;
CRM acceptance creates its own stable lead identity and does not transfer
Marketing aggregate ownership.

**Sources.** [Marketing context](../../crates/contexts/marketing/src/lib.rs),
[CRM context](../../crates/contexts/crm/src/lib.rs),
[marketing boundaries](../../adr/ADR-009-marketing-crm-boundaries.md).

## Lifecycle-completion requirements

| ID | Title | Normative requirement | Rationale | Acceptance paths | Source | Scenarios |
|---|---|---|---|---|---|---|
| REQ-MKT-090 | Marketing lifecycle coverage | ERPX shall govern consent, audience, content, campaign, journey, delivery, callback, suppression, attribution, closure, erasure, and archive. | Marketing execution needs complete privacy-aware states. | Success: valid transition pins policy/content; Denial: suppressed/ineligible contact is blocked; Failure: provider uncertainty reconciles. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-05 |
| REQ-MKT-091 | Marketing reconciliation | ERPX shall reconcile consent, customer, content, budget, provider delivery, CRM handoff, sales, cash, revenue, and erasure facts. | Marketing metrics cross owners. | Success: each outcome has source provenance; Denial: Marketing cannot assert provider/Finance success; Failure: mismatch creates work. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-05, E2E-06 |
| REQ-CRM-090 | CRM lifecycle coverage | ERPX shall govern accepted lead, qualification, opportunity, activity, forecast, proposal handoff, win, loss, reopen, closure, and archive. | CRM needs lifecycle beyond lead storage. | Success: valid transition records rationale; Denial: unaccepted marketing candidate cannot become CRM lead silently; Failure: stale opportunity conflicts. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-06 |
| REQ-CRM-091 | CRM reconciliation | ERPX shall reconcile customer authority, marketing source, activities, forecast, quote, order, fulfillment, invoice, and loss/win outcomes. | CRM status must reflect source facts. | Success: opportunity view distinguishes forecast from actuals; Denial: CRM cannot edit Sales/Finance state; Failure: mismatch is explicit. | [FSD-003](../decisions/FSD-003-enterprise-suite-expansion.md) | E2E-06 |

### REQ-MKT-003 — Audience eligibility

**Requirement.** ERPX shall build audience membership from versioned criteria
and current consent, suppression, tenant, and scope rules.

**Rationale.** A campaign audience must be reproducible and legally eligible.

**Acceptance criteria.** Audience snapshot identifies criteria and source
positions; membership is tenant-bound; suppressed subjects are excluded;
rebuild from the same facts is deterministic; unauthorized raw profile data is
not exported.

**Sources.** [Marketing domain](../../crates/domain/src/marketing.rs),
[marketing projection](../../apps/worker/src/marketing_projection.rs).

### REQ-MKT-004 — Campaign budget lifecycle

**Requirement.** ERPX shall manage campaign budgets through creation,
submission, approval decision, start, reservation, release, completion, and
cancellation.

**Rationale.** Marketing spend requires financial and business control before
provider delivery.

**Acceptance criteria.** Budget records currency, period, allocations, owner,
and version; submitted changes are version-protected; approval follows policy;
reserved spend cannot exceed approved available amount; duplicate spend
requests are idempotent.

**Sources.** [Marketing context](../../crates/contexts/marketing/src/lib.rs),
[marketing campaign-budget migration](../../crates/migration/migrations_tenant_write/0069_marketing_campaign_budget_guards.sql).

### REQ-MKT-005 — Governed content workflow

**Requirement.** ERPX shall manage marketing content through draft, review,
approval, publication, supersession, and archive with immutable asset and
version references.

**Rationale.** Delivered content must match the version that was reviewed for
brand, consent, and legal policy.

**Acceptance criteria.** Publication requires the approved current version;
editing published content creates a new draft; assets are tenant-bound and
digest-addressed; scheduled delivery pins the content version; archived
versions remain auditable.

**Sources.** [Marketing content domain](../../crates/domain/src/marketing_content.rs),
[content workflow worker](../../apps/worker/src/marketing_content_workflow.rs),
[Marketing content API](../../crates/proto/proto/erpx/marketing/v2/service.proto).

### REQ-MKT-006 — Durable journey execution

**Requirement.** ERPX shall execute approved marketing journeys as durable,
idempotent, restartable steps with explicit wait, branch, delivery, and
completion state.

**Rationale.** Long-running customer journeys must survive worker restart
without duplicate communication.

**Acceptance criteria.** Each participant and step has stable identity and
checkpoint; retry cannot duplicate a completed step; waits resume at the
correct time; invalid journey or content version blocks new execution;
cancellation stops future eligible steps.

**Sources.** [Marketing journey domain](../../crates/domain/src/marketing_journey.rs),
[journey execution worker](../../apps/worker/src/marketing_journey_execution.rs).

### REQ-MKT-007 — Last-moment consent and suppression

**Requirement.** ERPX shall re-evaluate consent and suppression immediately
before every marketing delivery, even when eligibility was valid at scheduling
time.

**Rationale.** A later opt-out must stop already queued communication.

**Acceptance criteria.** Delivery records both scheduling and send-time checks;
withdrawal or suppression cancels the queued send; unavailable consent
authority fails closed; retry repeats the check and cannot bypass it.

**Sources.** [Marketing boundaries](../../adr/ADR-009-marketing-crm-boundaries.md),
[marketing delivery worker](../../apps/worker/src/marketing_delivery.rs).

### REQ-MKT-008 — Provider delivery boundary

**Requirement.** ERPX shall send marketing content only through configured,
tenant-bound provider accounts and routes with secret-managed credentials and
typed response identity.

**Rationale.** Provider setup and retries must not leak credentials or confuse
delivery outcomes.

**Acceptance criteria.** Provider secrets never enter domain state, audit, or
client payload; response identity and provider message ID are persisted safely;
unconfigured providers fail rather than synthesize success; channel routing is
versioned and scope-authorized.

**Sources.** [Marketing provider worker](../../apps/worker/src/marketing_provider.rs),
[provider secret manager](../../apps/worker/src/marketing_provider_secret_manager.rs),
[marketing provider migrations](../../crates/migration/migrations/0049_marketing_provider_onboarding.sql).

### REQ-MKT-009 — Callback and delivery reconciliation

**Requirement.** ERPX shall verify, inbox, deduplicate, and reconcile provider
callbacks and delivery outcomes before updating marketing projections.

**Rationale.** Providers may retry or deliver callbacks out of order.

**Acceptance criteria.** Callback authenticity is verified by provider policy;
raw callback identity is persisted once; out-of-order transitions cannot move
a message backward incorrectly; unknown outcomes remain reconcilable; replay
does not duplicate engagement.

**Sources.** [Marketing callback domain](../../crates/domain/src/marketing_callback.rs),
[marketing delivery worker](../../apps/worker/src/marketing_delivery.rs),
[callback inbox migration](../../crates/migration/migrations_tenant_write/0027_marketing_provider_callback_inbox.sql).

### REQ-MKT-010 — Privacy erasure and suppression retention

**Requirement.** ERPX shall execute authorized marketing-profile erasure while
retaining the minimum non-reversible suppression and audit evidence required to
prevent future re-contact.

**Rationale.** Erasure must remove personal marketing data without undoing an
opt-out.

**Acceptance criteria.** Erasure verifies scope, authority, legal hold, and
current version; personal profile and content references are removed or
anonymized as defined; suppression remains non-contactable; completion is
idempotent and auditable.

**Sources.** [Marketing privacy worker](../../apps/worker/src/marketing_content_privacy.rs),
[marketing erasure migration](../../crates/migration/migrations_tenant_write/0049_marketing_profile_erasure.sql),
[marketing boundaries](../../adr/ADR-009-marketing-crm-boundaries.md).

### REQ-MKT-011 — Attribution and Finance reconciliation

**Requirement.** ERPX shall calculate versioned marketing attribution from
traceable engagement, CRM, Sales, and Finance facts and shall not represent
attributed value as recognized revenue.

**Rationale.** Marketing performance must reconcile to financial authority
without redefining revenue.

**Acceptance criteria.** Attribution identifies model version and source
facts; duplicate events are idempotent; ROAS distinguishes spend, attributed
value, invoiced value, cash, and recognized revenue; Finance differences are
visible.

**Sources.** [Marketing boundaries](../../adr/ADR-009-marketing-crm-boundaries.md),
[marketing projection](../../apps/worker/src/marketing_projection.rs),
[Finance context](../../crates/contexts/finance/src/lib.rs).

### REQ-MKT-012 — Safe public marketing surfaces

**Requirement.** ERPX shall provide bounded public forms and previews with
tenant-safe branding, abuse protection, consent capture, and no privileged
session exposure.

**Rationale.** Public acquisition surfaces are an untrusted boundary into the
platform.

**Acceptance criteria.** Public IDs are opaque and validated; submissions are
rate- and abuse-controlled; consent and purpose are explicit; previews cannot
execute arbitrary scripts or expose drafts outside their capability;
privileged credentials never enter public URLs.

**Sources.** [Console public marketing routes](../../apps/console/src/composition.rs),
[marketing public-abuse worker](../../apps/worker/src/marketing_public_abuse.rs),
[corporate identity decision](../../adr/ADR-012-three-layer-corporate-identity.md).

## CRM

### REQ-CRM-001 — Accepted CRM lead

**Requirement.** ERPX shall create a CRM lead only from an authorized,
qualified Marketing handoff or another explicitly approved CRM source.

**Rationale.** CRM should own revenue qualification after, not before,
marketing acceptance.

**Acceptance criteria.** Lead records source identity and version; duplicate
handoff returns the existing lead; rejected or stale Marketing candidates do
not create a lead; CRM assigns its own stable ID.

**Sources.** [CRM context](../../crates/contexts/crm/src/lib.rs),
[Marketing context](../../crates/contexts/marketing/src/lib.rs),
[CRM API](../../crates/proto/proto/erpx/crm/v2/service.proto).

### REQ-CRM-002 — Opportunity lifecycle

**Requirement.** ERPX shall manage opportunities through explicit stages with
customer, owner, scope, expected value, probability, and version.

**Rationale.** Pipeline reporting requires controlled stage meaning.

**Acceptance criteria.** Stage transitions follow allowed paths; owner is
currently eligible; currency and value are bounded; closed outcomes are
distinct; stale updates are rejected and history retained.

**Sources.** [CRM context](../../crates/contexts/crm/src/lib.rs),
[CRM API](../../crates/proto/proto/erpx/crm/v2/service.proto).

### REQ-CRM-003 — Opportunity forecast

**Requirement.** ERPX shall maintain versioned opportunity forecasts and shall
separate forecast value from quotes, orders, invoices, cash, and recognized
revenue.

**Rationale.** Forecast confidence must not be reported as booked or earned
value.

**Acceptance criteria.** Forecast records amount, currency, probability,
expected date, basis, actor, and version; update requires current opportunity
state; reports label forecast and authoritative downstream values separately;
closed opportunities follow policy-defined forecast behavior.

**Sources.** [CRM context](../../crates/contexts/crm/src/lib.rs),
[CRM forecast migration](../../crates/migration/migrations_tenant_write/0067_crm_opportunity_forecast_guards.sql).

### REQ-CRM-004 — Sales handoff

**Requirement.** ERPX shall hand an eligible CRM opportunity to Sales through
a versioned fact without allowing CRM to create or mutate a Sales quote
directly.

**Rationale.** Pipeline and commercial commitment require separate ownership.

**Acceptance criteria.** Handoff identifies opportunity, customer, scope,
commercial summary, and version; duplicate handoff is idempotent; Sales applies
its own authorization and validation; resulting quote references the CRM
source.

**Sources.** [CRM context](../../crates/contexts/crm/src/lib.rs),
[Sales context](../../crates/contexts/sales/src/lib.rs),
[marketing boundaries](../../adr/ADR-009-marketing-crm-boundaries.md).
