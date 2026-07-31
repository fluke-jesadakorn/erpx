# ERPX conversion plan

ERPX is the Rust/Dioxus target architecture. Folio remains the legacy pilot
until each bounded context has a verified replacement. The migration is
incremental and keeps tenant business data behind ordinary domain commands.

## Delivered foundation

1. Control-plane schema for customer 360, holding/company/branch structure,
   entitlements, support tickets, customer-approved support capabilities, and
   immutable expiring role assignments.
2. Next.js Platform Customer Console with audited global-read access, support
   intake, capability requests/revocation, role assignment/renewal routes, and
   read/write connection boundaries.
3. Rust workspace with domain, application, infrastructure, protobuf, Tonic,
   and shared Dioxus console crates. Web, desktop, and mobile use the same
   Dioxus component tree.
4. Domain tests for time windows, parent scope inheritance, support approval
   and expiry, payment-failure read-only mode, catalog immutability, and CQRS
   projection checkpoints.

## Conversion sequence

1. **Identity and access:** move Folio permissions into versioned ERPX role
   assignments, add a controllable clock, renewal queues, parent delegation
   checks, and last-administrator recovery workflows.
2. **Control plane:** replace the development in-memory adapter with SQLx
   repositories over `erpx_control_write` and `erpx_control_read`; publish
   outbox events and rebuildable projections per bounded context.
3. **Commercial and billing:** add catalog revision publication, Stripe
   Product/Price/Entitlement/Meter ports, signed idempotent webhook handling,
   reconciliation, trials, grace/read-only transitions, and entitlement
   allocation from holding headquarters to companies and branches.
4. **Support operations:** add email/API intake, diagnostics bundles,
   incident/SLA workflows, customer-visible replies, session banners, and
   tenant command routing through approved capabilities.
5. **Tenant modules:** migrate organization, finance, sales, procurement,
   inventory, assets, maintenance, HR, legal, CRM/POS, projects, service,
   manufacturing, quality, AI, and integrations as separate DDD contexts.
6. **Client replacement:** expand Dioxus Customer Operations into holding and
   tenant workspaces, then ship the same Rust UI to web, desktop, and mobile.
   Folio routes remain available until each replacement flow passes its
   end-to-end contract tests.

## Exit gates for each context

- Commands enforce authoritative write-side RBAC and entitlement state.
- Read models are isolated, versioned, idempotent, checkpointed, and
  rebuildable after loss.
- Every support or global-read path produces customer-visible audit evidence.
- Secrets, credentials, payment instruments, and private keys are never
  returned by a read model or support diagnostic bundle.
- Rust format, Clippy, unit/integration tests, protobuf compatibility, and
  Dioxus target checks pass alongside Folio's Bun lint and TypeScript checks.
