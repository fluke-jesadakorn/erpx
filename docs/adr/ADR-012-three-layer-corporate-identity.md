# ADR-012: Governed Three-Layer Corporate Identity

> **Next.js era: Reconfirmed (2026-09-02) — still authoritative**
> See `CUTOVER.md` (2026-09-02) and `ADR-020-nextjs-stack.md`.


## Status

Accepted.

## Context

ERPX serves platform staff, customer holdings, and the same canonical twelve
business units through web, desktop, iOS, and Android Dioxus targets. A
customer-hosted holding hosts only its data plane; the Root console, login
orchestration, authorization, and public presentation bootstrap remain control
plane responsibilities.

Allowing arbitrary customer CSS, scripts, HTML, remote fonts, or expiring asset
URLs would make accessibility, upgrades, pre-login safety, and administrative
actor attribution unreliable. Conversely, a single ERPX accent color cannot
represent customer and business-unit corporate identities.

## Decision

ERPX stores versioned, typed corporate-identity profiles in the control plane at
three scopes: platform, holding, and business unit. Resolution applies mandatory
ERPX safety tokens first, then the published platform profile, holding overrides,
business-unit overrides, and finally user presentation preferences.

Platform and holding profiles may lock identity categories against child
overrides. Security/status colors, focus indication, destructive action styling,
support/impersonation banners, and Root actor identity are never brandable.

Brand drafts use optimistic concurrency. Publication and rollback require fresh
MFA, a reason, permission at the exact scope, immutable audit, and outbox
evidence. ERPX Root can propose customer branding but cannot silently publish it.

Published identity is public presentation metadata. A verified hostname may
resolve a holding profile before authentication; an unknown hostname receives
the same neutral ERPX response. Corporate identity never contains credentials,
holding data, infrastructure references, executable styling, or customer-hosted
agent commands.

Asset references are immutable digest-scoped control-plane object keys. Runtime
font requests are limited to bundled, licensed font packs. Clients map typed
tokens to a fixed CSS-variable allow-list and retain the last valid public bundle
for offline presentation fallback.

## Consequences

- ERPX-managed and customer-hosted holdings use the same identity resolver.
- Existing holdings inherit the neutral ERPX profile until an administrator
  publishes an override.
- Business-unit branding changes presentation only while that unit context is
  active and never changes its holding database, directory, roles, or scope.
- Pre-login pages always retain an ERPX security attribution and cannot be made
  indistinguishable from an untrusted login form.
- New tokens require a versioned contract and all-target accessibility review;
  arbitrary CSS is intentionally unsupported.