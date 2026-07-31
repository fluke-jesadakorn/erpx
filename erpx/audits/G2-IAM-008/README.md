# G2-IAM-008 — Universal mutation authorization route audit

## Objective

Prove that every authenticated ERPX server mutation reaches an authoritative
write-side authorization boundary, while platform/control-plane mutations
remain distinct from commercial tenant mutations.

## Audit result

- Tenant mutations for Support, HR, POS, Legal, CRM, Marketing, and Finance
  call `AppState::authorize_entitled_command`.
- That boundary binds the authenticated actor to the command context,
  customer scope, active identity/session token, recursive role chain,
  billing state, purchased entitlement, company allocation, branch activation,
  SoD decision, and mutation policy.
- Projection rebuild, incident, and support-session revocation mutations use
  platform/control-plane role authorization with explicit scope checks.
- Role renewal uses `authorize_role_lifecycle_command`; tenant renewal enters
  the entitlement boundary and platform renewal remains control-plane scoped.
  Final-role recovery additionally requires independently resolved approver and
  reviewer identities and now writes a durable recovery receipt.
- The unauthenticated Marketing form endpoint is intentionally outside RBAC:
  it is public acquisition ingestion and is bounded by tenant/domain binding,
  consent, suppression, deduplication, and abuse controls before persistence.

## Independent evidence

Run from `erpx/`:

```sh
bash audits/G2-IAM-008/verify_authorization_routes.sh
```

The inventory script passed for all 45 authenticated tenant mutation methods,
five platform/control-plane mutation methods, and both role lifecycle methods.
The same integration wave passed workspace clippy/tests, Dioxus web/desktop
checks, Folio lint/typecheck, database readiness, and the live Folio route.

## Nonclaims

This is an authorization-adoption closure, not a claim that all G0-G14
business modules, client targets, migration cutover, or Folio write retirement
are complete.

## Directory-managed authorization continuation — 2026-07-28

The server role boundary now consults `SqlxIdentityStore::directory_role_access`
before evaluating the local `erpx_control.role_assignments` aggregate. A
persisted directory projection is therefore authoritative for that principal:
the requested permission must match an active connector-scoped mapping and
current group set, the connector must remain enabled, and the feed timestamp
must be within the five-minute revocation SLA. Disabled, revoked, stale, or
missing mapped access returns an explicit denial; it cannot fall through to a
local role assignment. Only principals with no directory projection use the
local role path. Mappings marked high-risk additionally require the
write-side TOTP verification timestamp to be within five minutes.

The ignored PostgreSQL directory acceptance fixture exercised the real path
for an active mapped identity, a disabled identity, and a stale projection;
all three assertions passed on a fresh disposable database on 2026-07-28.
Vendor certification, physical MFA/device journeys, and universal production
cutover evidence remain open.
