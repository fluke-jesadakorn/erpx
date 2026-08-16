# Shared Policy and User Experience Requirements

## Channel and interaction requirements

### REQ-UX-001 — Shared product semantics

**Requirement.** ERPX shall present the same business rules, authorization
meaning, validation, state vocabulary, and audit outcomes across Web, Desktop,
iOS, and Android.

**Rationale.** A user must not gain different business authority by changing
device or client.

**Acceptance criteria.** Shared workflows consume the same typed contracts;
target adapters cannot weaken server authorization; target-specific
capabilities are isolated behind explicit interfaces; equivalent actions
produce equivalent receipts and failure categories.

**Sources.** [Dioxus target strategy](../adr/ADR-010-dioxus-target-strategy.md),
[shared UI](../crates/ui/src/lib.rs), [Android bridge](../crates/android-bridge/src/lib.rs).

### REQ-UX-002 — Web delivery baseline

**Requirement.** ERPX shall treat the Web console as the primary delivery and
certification baseline while preserving the shared contracts required by the
other target channels.

**Rationale.** A clear primary channel enables reliable releases without
abandoning cross-target product consistency.

**Acceptance criteria.** All baseline journeys are operable through the Web
console; Web uses same-origin protected session transport; Desktop and mobile
adapters compile against the shared UI contract and cannot introduce a second
business implementation.

**Sources.** [Project overview](../README.md),
[Dioxus target strategy](../adr/ADR-010-dioxus-target-strategy.md),
[console application](../apps/console/src/app.rs).

### REQ-UX-003 — English and Thai experience

**Requirement.** ERPX shall provide English and Thai labels for canonical
navigation, roles, business states, validation, and critical security content.

**Rationale.** The target customer and operating environment require a
consistent bilingual experience.

**Acceptance criteria.** Users can switch locale without changing authority or
route identity; all 21 canonical workspace labels and all standard role labels
are available in both languages; missing translations use a safe, readable
fallback rather than an internal key.

**Sources.** [Localization](../apps/console/src/localization.rs),
[navigation localization](../crates/ui/src/navigation.rs),
[business-unit catalog](../crates/domain/src/data_plane.rs).

### REQ-UX-004 — Accessible operation

**Requirement.** ERPX shall meet WCAG 2.2 AA for target workflows and shall
preserve focus, accessible names, contrast, keyboard operation, and non-color
state communication.

**Rationale.** Enterprise work must remain usable by people with different
access needs and input methods.

**Acceptance criteria.** Interactive controls have programmatic names and
keyboard focus; status is not conveyed by color alone; brand resolution cannot
override safety colors or focus indicators; required journeys pass automated
and manual accessibility review.

**Sources.** [Dioxus target strategy](../adr/ADR-010-dioxus-target-strategy.md),
[UI foundation](../crates/ui-foundation/src/lib.rs),
[brand context](../crates/contexts/brand/src/lib.rs).

### REQ-UX-005 — Explicit screen states

**Requirement.** ERPX shall present loading, empty, ready, denied, stale,
not-ready, conflict, offline, and failure states explicitly without inventing
successful business data.

**Rationale.** Users must distinguish absence, delay, denial, and failure before
making decisions.

**Acceptance criteria.** A stale projection is labeled stale; a timed-out
read-your-writes request reports not-ready; denied actions do not appear as
empty data; retry preserves a safe route and session; offline edits are
limited to explicitly approved drafts with conflict recovery.

**Sources.** [Dioxus target strategy](../adr/ADR-010-dioxus-target-strategy.md),
[product surface contracts](../apps/console/src/product_completion.rs),
[UI state types](../crates/ui/src/lib.rs).

### REQ-UX-006 — Visible scope and exceptional authority

**Requirement.** ERPX shall visibly identify the active business scope and any
read-only, support, impersonation, delegated, or Root authority affecting the
current page.

**Rationale.** Exceptional authority must never be mistaken for ordinary
customer work.

**Acceptance criteria.** Scope and exceptional-mode banners remain visible
through navigation; impersonation attributes both actors; delegated approval
shows the delegation source and expiry; non-brandable security styling remains
intact.

**Sources.** [Support capabilities](../adr/ADR-006-support-capabilities.md),
[corporate identity](../adr/ADR-012-three-layer-corporate-identity.md),
[console composition](../apps/console/src/composition.rs).

### REQ-UX-007 — Governed module discovery

**Requirement.** ERPX shall let users discover authorized modules and, when
business policy permits, understand which standard role is required for a
locked module without exposing permissions or secrets.

**Rationale.** Discoverability supports correct access requests while keeping
authorization server-owned.

**Acceptance criteria.** Available and locked modules are distinguishable;
locked items cannot be opened; role requirements use safe localized role names;
navigation metadata cannot authorize a command; credential-looking metadata is
discarded.

**Sources.** [Navigation model](../crates/ui/src/navigation.rs),
[console composition](../apps/console/src/composition.rs),
[business-unit catalog](../crates/domain/src/data_plane.rs).

### REQ-UX-008 — Safe links, session storage, and offline behavior

**Requirement.** ERPX shall prevent credentials and sensitive session material
from entering browser URLs, ordinary local storage, logs, or unapproved offline
records.

**Rationale.** Navigation and device convenience must not create a secondary
credential leak path.

**Acceptance criteria.** Deep links reject credential-bearing or external
targets; Web session material remains in protected same-origin handling;
native targets use platform-secure storage; only approved draft types can be
stored offline and every conflict requires explicit resolution.

**Sources.** [Dioxus target strategy](../adr/ADR-010-dioxus-target-strategy.md),
[web session](../crates/web-session/src/lib.rs),
[UI redaction](../crates/ui/src/lib.rs).

## Shared command policies

### REQ-POL-001 — Confirmed outcomes and receipts

**Requirement.** ERPX shall return a durable, correlation-safe outcome for
every accepted command and a stable failure category for every rejected
command.

**Rationale.** Users and integrations need to know whether retry is safe and
whether business state changed.

**Acceptance criteria.** Accepted commands return a command receipt and
resulting version; rejected commands return no false success; unknown provider
or infrastructure outcomes remain pending or failed until reconciled.

**Sources.** [CQRS contracts](../crates/cqrs/src/lib.rs),
[contract policy](../CONTRACT_POLICY.md).

### REQ-POL-002 — Optimistic concurrency

**Requirement.** ERPX shall protect versioned business records and policy
drafts from silent overwrite by stale clients.

**Rationale.** Concurrent administrative and business work must not erase a
newer decision.

**Acceptance criteria.** Mutations carry the expected authoritative version;
stale submissions are rejected with the current version reference; clients
offer reload or conflict-resolution behavior rather than automatically
resubmitting.

**Sources.** [CQRS bounded contexts](../adr/ADR-014-cqrs-bounded-context-modular-monolith.md),
[brand context](../crates/contexts/brand/src/lib.rs),
[CQRS contracts](../crates/cqrs/src/lib.rs).

### REQ-POL-003 — Explicit business transitions

**Requirement.** ERPX shall model material lifecycle changes as named business
transitions and shall reject invalid, skipped, or terminal-state transitions.

**Rationale.** Named transitions preserve understandable business control and
audit evidence.

**Acceptance criteria.** Each lifecycle exposes its allowed transitions;
terminal records cannot silently return to an active state; cancellation,
reopen, rollback, or override behavior exists only where its context defines
it and requires a reason when specified.

**Sources.** [Context cores](../crates/contexts),
[integration contracts](../crates/integration-contracts/src/lib.rs).

### REQ-POL-004 — Bounded and redacted presentation

**Requirement.** ERPX shall bound user-visible identifiers and text and shall
redact credential-like or unauthorized values before they reach a client,
audit view, diagnostic, or log.

**Rationale.** A successful request must not become a secret disclosure path.

**Acceptance criteria.** IDs use approved opaque formats and length limits;
credential markers and control characters are rejected or redacted; provider
payloads, DSNs, bind credentials, tokens, payment instruments, and private keys
are absent from presentation models.

**Sources.** [Product display contracts](../apps/console/src/product_completion.rs),
[tenant isolation](../adr/ADR-001-tenant-isolation.md),
[audited reads](../adr/ADR-005-global-read-auditing.md).
