# Folio RBAC standard

Folio has one authorization system. The schema is `perm`, the server implementation is in `lib/perm`, and the current reference catalog is in `db/seed.sql`.

## Permission IDs

Every permission uses this shape:

```text
<domain>:<subject>:<verb>[:<qualifier>]::<effect>
```

The effect is `allow` or `deny`. It is part of the ID; there is no separate effect column. An omitted qualifier is global. Typical IDs are:

```text
finance:expense:create::allow
stage:accounting_verification:act::allow
tile:expense:view::allow
user:dept:accounting::allow
admin:system:bypass::allow
```

`lib/perm/grammar.ts` parses and matches permission IDs. `lib/perm/taxonomy.ts` contains the application catalog.

## Organization model

`perm.departments` contains the five baseline departments: `it`, `hr`, `accounting`, `finance`, and `executive`.

Hierarchy roles use stable IDs and belong to one department through `perm.roles.department_id`. Authority is stored in `perm.roles.rank`; a smaller rank has greater authority. The baseline has 14 hierarchy roles:

| Department | Roles |
|---|---|
| IT | `it_manager`, `it_supervisor`, `it_officer` |
| HR | `hr_manager`, `hr_supervisor`, `hr_officer` |
| Accounting | `accounting_manager`, `accounting_supervisor`, `accounting_officer` |
| Finance | `cfo`, `finance_manager`, `finance_supervisor`, `finance_officer` |
| Executive | `ceo` |

CEO is rank 1, CFO is rank 2, managers are rank 3, supervisors are rank 4, and officers are rank 5.

## Assignment tables

| Table | Purpose |
|---|---|
| `perm.user_departments` | One user's department membership |
| `perm.user_roles` | Hierarchy and system role assignments |
| `perm.user_permissions` | Direct permanent or time-bound permission grants |
| `perm.role_permissions` | Role permission bundles |
| `perm.department_permissions` | Product access shared by a department |

A hierarchy role assignment must match the selected department. The application enforces this through `lib/perm/access.ts`, and the database enforces role-to-department ownership with foreign keys and checks.

Effective permissions are the union of role grants, department grants, active direct grants, and the derived `user:dept:<department>::allow` marker. Explicit matching denies take precedence. `admin:system:bypass::allow` grants global access, but the baseline seed does not grant it to any role.

## Tiles and policies

`perm.tiles.view_perm_id` is the sole tile visibility gate. Tile decisions use the actor's effective permission set; there are no level or department columns on a tile.

`perm.policies` stores JSON policy rules for resource-specific decisions. Stage actions still require the matching `stage:<stage>:act::allow` permission.

## Access boundaries

- IT owns platform configuration, RBAC, AI settings, integrations, audit, and Law administration.
- HR owns employee lifecycle, organization assignment, leave, quotas, and access-request handling.
- Accounting owns accounting verification, supervision, authorization, and GL posting.
- Finance owns payment, disbursement, finance authorization, budgets, and sales processing.
- CEO and CFO receive only their explicit executive and finance authority; neither authority is implied by IT administration.

## Source map

| Concern | Source |
|---|---|
| Schema and constraints | `db/schema.sql` |
| Departments, roles, permissions, grants, tiles, policies | `db/seed.sql` |
| Permission grammar | `lib/perm/grammar.ts` |
| Session hydration | `lib/perm/auth.ts` |
| Rank lookup | `lib/perm/level.ts` |
| Stage authorization | `lib/perm/chain.ts` and `lib/perm/stages.ts` |
| Department grants | `lib/perm/deptGrant.ts` |
| Organization scope | `lib/org/scope.ts` |

The baseline seed contains no identities. Create the first access administrator through `/api/auth/bootstrap`; subsequent assignments go through the authenticated administration flows.
