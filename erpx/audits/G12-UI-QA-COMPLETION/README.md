# G12 UI/QA completion and DirectoryService acceptance

Status: integrated web read surface; native and external-directory
certification remain open.

## Scope

This packet adds two isolated console modules:

- `apps/console/src/product_completion.rs` — target-neutral typed product-surface
  state and security contracts, plus a read-only Dioxus status component.
- `apps/console/src/qa_completion.rs` — executable QA contract tests for the
  product surface and coordinator integration inventory.

This packet did not change the coordinator-owned console shell, protobuf,
manifest, migration, server/worker registration, shared authorization, or Folio
route. The current console integration is recorded below as observed evidence.

## Delivered contract

The owned modules cover:

- Generic LDAP using RFC 4533, on-premises Microsoft Active Directory Domain
  Services using AD DirSync, and Univention Corporate Server using the UCS
  Provisioning Service.
- Explicit platform and per-holding connector scopes.
- Full-import capacity of up to 1,000,000 users per connector.
- Directory-managed role mappings, immutable external-subject identity links,
  append-only role revocation, TLS-only transport posture, fresh MFA for
  high-risk access, a five-minute revocation budget, and local break-glass-only
  fallback.
- Platform Operations areas for tenants, projections, backups, integrations,
  audit, and settings.
- Operations, Finance, Sales, Procurement, Inventory, Maintenance, Support,
  Marketing/CRM, HR/Payroll, POS, and Legal context snapshots, all explicitly
  read-only at this presentation boundary.
- Loading, empty, denied, stale, read-only, error, offline, and ready states;
  shared `UiStateKind` mapping; safe bounded display text; credential-bearing
  deep-link redaction; and DOM-safe QA selectors.
- Android/iOS target-neutral handoff metadata that reports native certification
  as still required and always returns `claims_certification() == false`.
- Real iOS and macOS Keychain-backed `SecureSessionStore` adapters with bounded,
  revalidated session bytes. Android and Windows/Linux targets remain fail-closed
  without a target-owned protected store.

The module validates the contract boundary; it does not create connectors,
import users, authorize roles, persist data, publish outbox events, run a
worker/projector, or certify a live LDAP/AD/UCS environment.

## Current console integration

The coordinator-owned console shell exposes `/platform/directories` and
renders `DirectoryAdministrationSurface` from an authenticated,
scope-authorized `DirectoryService` read model. The live adapter is web-only:
the `wasm32` path calls `ListDirectoryConnectors` and
`ListDirectoryRoleMappings` over the authenticated gRPC-Web channel, validates
the provider/feed pairing, TLS-only transport, one-million-user import bound,
scope, and append-only role mapping contract, and fails closed on malformed or
non-compliant wire data. The non-web path returns an explicit
`directory administration transport is available only in the web console`
error; it does not substitute local or fabricated directory data.

The route renders loading, empty, error, and authorized read-only states. It
does not expose connector secrets, provider payloads, mutation callbacks, or a
local fallback path.

## Focused tests

Run from `erpx/`:

```text
cargo test -p erpx-console --bin erpx-console --offline --all-targets --no-fail-fast
cargo clippy -p erpx-console --all-targets -- -D warnings
```

The focused console test run completed with **41 passed, 0 failed, 0 ignored**.
The QA tests cover vendor/feed pairing, one-million-user capacity, platform
and holding scope separation, immutable identity links, directory-managed role
mapping, append-only revocation, TLS-only transport, high-risk MFA freshness,
five-minute revocation, explicit state taxonomy, redaction, read-only
behavior, all product contexts, and non-claiming mobile handoff behavior.

These are contract and route-boundary tests. They do not claim Android/iOS
device execution, Android Keystore behavior, or external-vendor certification.
The iOS secure-session adapter is implemented and cross-target type-checked;
Android remains fail-closed until its native Keystore bridge is implemented and
exercised.

## Rollback

Remove the two new source modules and this audit directory. No database,
runtime state, Folio behavior, shared shell, or server/worker behavior depends
on this isolated packet.

## Remaining blockers and evidence boundary

- No credentialed generic LDAP, on-premises AD DS, or UCS environment is
  available locally; RFC 4533, AD DirSync, and UCS Provisioning Service
  acceptance remains queued.
- No live directory import, native change-feed replay, five-minute revocation
  observation, or external TLS/MFA evidence is claimed here.
- No Android or iOS simulator/physical-device execution is available; native
  secure storage, notification, offline/conflict, accessibility, and mobile
  approval journeys remain unverified. See the separate
  `G12-MOBILE-VERIFICATION` audit for target tooling findings.

Therefore this file records the completed web read-surface evidence but does
not close the full G12 gate or any external directory certification gate.

## Coordinator continuation — typed completion evidence states — 2026-07-27

The product-completion surface now renders typed persistence, reconciliation,
authorization, blocked, and unavailable evidence for each product context.
Focused console tests and the full workspace suite pass. This remains a
read-only evidence surface: it does not fabricate readiness, activate a
directory, or provide native secure-storage/device certification. External
LDAP/AD/UCS and physical Android/iOS acceptance remain queued.
