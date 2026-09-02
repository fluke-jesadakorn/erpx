# Identity Access — Deep `@` + AI (Production)

**Group:** `01-platform-governance` · **Code:** `crates/contexts/identity_access` · **Proto:** `crates/proto/proto/erpx/identity_access/service.proto` (`IdentityAccessCommandService, IdentityAccessQueryService` · 34 RPCs) · **Adapters:** `crates/adapters/identity_access-sqlx` · **UI:** `apps/console/src/features/identity_access` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `Plan/02-capabilities/01-platform-governance/identity_access.md`. Central polymorphic `@` contract in `Plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (34)

- **Services:** IdentityAccessCommandService, IdentityAccessQueryService
- **RPCs:** ** SaveDirectoryConnector, StartDirectoryImport, SaveDirectoryRoleMapping, PreflightDirectoryConnector, TransitionDirectoryConnectorLifecycle, LinkDirectoryPrincipal, UnlinkDirectoryPrincipal, ReconcileDirectoryConnector, ProposeDirectoryIdentityReplacement, ApproveDirectoryIdentityReplacement, RejectDirectoryIdentityReplacement, RenewRole, RecoverFinalRole, CreatePrincipal, ActivatePrincipal, SuspendPrincipal, ReactivatePrincipal, DisablePrincipal, AssignRole, RevokeRole, DefineRole, SetRolePermissions, ListDirectoryConnectors, GetDirectoryImportStatus, ListDirectoryRoleMappings, ListDirectoryIdentityLinks, GetDirectoryConnectorReadiness, ListDirectoryConflicts, ListExpiringRoles, ListRoleReview, GetPrincipal, ListPrincipals, GetRole, ListRoles

| RPC | Service | Data touch |
|-----|---------|------------|
| `** SaveDirectoryConnector` | Command/Query | `identity_access` aggregate + outbox |
| `StartDirectoryImport` | Command/Query | `identity_access` aggregate + outbox |
| `SaveDirectoryRoleMapping` | Command/Query | `identity_access` aggregate + outbox |
| `PreflightDirectoryConnector` | Command/Query | `identity_access` aggregate + outbox |
| `TransitionDirectoryConnectorLifecycle` | Command/Query | `identity_access` aggregate + outbox |
| `LinkDirectoryPrincipal` | Command/Query | `identity_access` aggregate + outbox |
| `UnlinkDirectoryPrincipal` | Command/Query | `identity_access` aggregate + outbox |
| `ReconcileDirectoryConnector` | Command/Query | `identity_access` aggregate + outbox |
| `ProposeDirectoryIdentityReplacement` | Command/Query | `identity_access` aggregate + outbox |
| `ApproveDirectoryIdentityReplacement` | Command/Query | `identity_access` aggregate + outbox |
| `RejectDirectoryIdentityReplacement` | Command/Query | `identity_access` aggregate + outbox |
| `RenewRole` | Command/Query | `identity_access` aggregate + outbox |

## Invariants (domain — keep)

- All RBAC assignments expire (`valid_from/until`); assignments are parent-bounded and audited.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `identity_access` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `role_assignments + role_review_* + audit_events + sod_decisions` (light `identity_access.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** Principal lifecycle Create→Activate with expiring assignment — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Route & UI delta

- **Route:** `/identity/principal + /identity/grant + /reference/item/:id` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title IDENTITY ACCESS, list identity_access-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/identity_access/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/identity_access/src/domain.rs`
- **Adapters:** `identity_access-sqlx`
- **UI:** `apps/console/src/features/identity_access`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `identity_access`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `Plan/MAP.md` for flat index.
