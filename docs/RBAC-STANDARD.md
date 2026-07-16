# Folio — RBAC Standard

Single source of truth for permissions, roles, departments, and tiles.

Last revised: 2026-07-16 (folio flatten: domain logic in `lib/perm/`; grammar
now uses inline `::effect` and role-id `::level` — see
`folio/lib/perm/grammar.ts`).

> **Grammar migration.** The 4-segment grammar below (`domain:subject:verb:scope`)
> has been replaced by an inline-effect form:
> `<domain>:<subject>:<verb>[:<qualifier>]::<effect>` with effect ∈
> `{allow, deny}` encoded in the string itself. There is no separate `effect`
> column on any RBAC table. Roles use `<name>::<level>` with level ∈ 1..10.
>
> **`lib/perm/grammar.ts` is now the canonical source of truth** — it owns the
> single `parsePerm` / `parseRoleId` / `buildPerm` / `buildRoleId` /
> `effectOf` / `matchPerm` parser-encoder pair, with no effect / dept / scope
> columns anywhere in the schema.
>
> See `db/perm/9000-rebuild-string-grammar.sql` and `db/perm/9001-seed-new-grammar.sql`
> for the schema migration that removed the `effect`, `kind`, `level`,
> `required_level`, `required_dept_id`, and `dept_group_id` columns.

---

## 1. Conceptual model

| Term | Definition |
|---|---|
| **Role** | A persona + an org-tree position. Level encoded in the role id as `<name>::<level>` (level 1 = CEO, 10 = lowest). All roles live in `perm.roles`. |
| **Permission** | A verb on a subject in a domain; format `<domain>:<subject>:<verb>[:<qualifier>]::<effect>`. Effect (allow / deny) is encoded inline. |
| **User** | A person (employee). Bound to roles via `perm.user_roles`. Department membership is granted via `user:dept:<id>::allow` on `perm.user_permissions`. |
| **Department** | A `perm.roles` row whose id follows `<name>::<level>` — no `kind` flag. Has `parent_role_id` for the tree. |
| **Tile** | A navigable surface, gated by the union of the actor's effective permissions plus the tile's `view_perm_id`. No `required_level` or `required_dept_id` columns anymore. |
| **Acting grant** | A time-bound perm row on `perm.user_permissions` (`ends_at NOT NULL`). Used for **mutation perms only** (head-of-department, acting-as responsibilities). Never bypasses tile view. |

---

## 2. Permission grammar — inline-effect

```
<domain>:<subject>:<verb>[:<qualifier>]::<effect>
```

`domain ∈ {rbac, user, org, finance, stage, tile, hook, ai, policy, access_request, admin}`.
`subject` and `verb` are lowercase snake. `qualifier` is optional (omit or `*` for global).
`effect ∈ {allow, deny}` is required and **encoded inline** in the string.

### Examples

```
finance:expense:approve::allow               ← global allow
finance:expense:approve:finance-2::allow    ← dept-scoped allow
user:dept:finance-2::allow                   ← dept membership marker
tile:expense:view::allow                     ← tile view gate
admin:system:bypass::allow                   ← admin bypass — grants everything
```

### Why inline effect

- One canonical shape; no `effect` column to drift between the string and the row.
- `effectOf()` parses the string; no hardcoded lists anywhere.
- Adding a new perm requires zero code changes.

---

## 3. Schema (current state)

```
perm.roles               (id PK = '<name>::<level>', display_name, description,
                          is_system, sort_order, parent_role_id,
                          display_name_th, display_name_de, monthly_budget,
                          head_user_id)                      -- NO 'kind', NO 'level'
perm.permissions         (id PK = full '::' string, description)
perm.role_permissions    (role_id, permission_id, granted_at, granted_by)   -- NO 'effect'
                          required_dept_id text NULL FK → perm.roles(kind='department'))

rbac.perm_grants         (id, user_id, permission_id, starts_at, ends_at NOT NULL,
                          granted_by, reason, source 'manual'|'seed'|'bulk'|'access_request',
                          revoked_at, revoked_by)
```

`rbac.*` other tables: dropped (consolidated into `perm.*`).

`users.dept_group_id`: kept as cached FK (trigger-maintained from `perm.user_roles`).

---

## 4. Lifetime semantics

| Path | ends_at | revoked_at |
|---|---|---|
| Permanent role grant (`perm.user_roles`) | n/a | DELETE row |
| Permanent head perm (`rbac.perm_grants` for `dept:X:head:N:all`) | NULL or far future | SET revoked_at |
| Acting / temp (`rbac.perm_grants` for time-bound perms) | NOT NULL, future | optional SET revoked_at |
| Expired (cron) | past | auto SET by `rbac.expireOverdueGrants()` |

---

## 5. Overlap rules

| Perm category | Multiple grants allowed? | Enforced by |
|---|---|---|
| Action perms (`finance:expense:approve:*`) | ✅ Yes | `perm.effective_user_perms` view unions |
| Level (`rbac:level:grant:min:N:all`) | ❌ No | `perm.enforce_one_dept_per_user` trigger (dept) + app logic (level) |
| Dept `:belong:self` | ❌ No (one primary dept per user) | trigger `perm.enforce_one_dept_per_user` |
| Dept `:manage:dept` / `:head:N` | ❌ No (one head per dept) | partial unique index `rbac_pg_one_head_per_user_dept` |
| Dept `:assign:any` | ✅ Yes | (no constraint) |
| Tile (`tile:*:view:all`) | ✅ Yes | (mirrors underlying action perm) |

---

## 6. Level as range permission

Lower N = higher authority. The canonical mapping lives in
`app/src/lib/roles/display.ts` (`ROLE_LEVEL`) and is mirrored to
`perm.roles.level` via `db/perm/0027_align_levels.sql`.

```
rbac:level:grant:min:1:all     CEO
rbac:level:grant:min:2:all     C-Level (cfo, admin, finance)
rbac:level:grant:min:3:all     Manager (manager, hr_manager, accounting_manager)
rbac:level:grant:min:4:all     Supervisor (supervisor, account_supervisor)
rbac:level:grant:min:5:all     Officer (staff, hr, it, account_officer)
rbac:level:grant:min:6-10:all  Reserved (interns, read-only)
```

> **Persona collapse (2026-07-08):** The legacy `accountant` persona has
> been merged into `account_officer`. Both displayed as "Accounting
> Officer" but `accountant` held read-only review perms while
> `account_officer` held approve/reject. The collapsed role carries
> the union of both sets. Run `db/perm/9001-seed-new-grammar.sql`
> before this version.

`effectiveLevel(userId) = MIN(extractMinLevel(held level perms))`.

Granting rule: granter must have `effectiveLevel < N` (higher authority).

---

## 7. Department as role

Departments are rows in `perm.roles` with `kind='department'`. Tree via `parent_role_id`. Membership via `perm.user_roles`.

User's primary dept = the (only, by trigger) dept-kind role they hold.

```
SELECT id FROM perm.roles WHERE kind = 'department' ORDER BY sort_order;
-- dept-development, dept-executive, dept-finance-2, dept-marketing, dept-it, dept-hr-2
```

`users.dept_group_id` is a cached FK (trigger `perm.sync_user_dept_cache`).

---

## 8. Acting flow

HR creates an acting assignment → UI calls `lib/perm/grants.ts:grantActingBundle(userId, roleId, endsAt, grantedBy, reason)` → inserts N rows in `rbac.perm_grants` (one per role perm) with `source='manual'`.

Effective perms for a user: `perm.effective_user_perms` view (real + temp).

```
effective_user_perms(user_id) = 
  role_permissions UNION active_temp_perms
```

Cron daily: `rbac.expireOverdueGrants()` sets revoked_at where ends_at < now().

---

## 9. Head perm flow

Head = a time-bound user→perm grant with key `dept:<slug>:head:<N>:all` where N is the required authority level.

HR sets head via `/api/departments PATCH` → inserts row in `rbac.perm_grants` with `ends_at = now() + 100 years`.

Partial unique index `rbac_pg_one_head_per_user_dept` enforces one head per (user, dept) pair.

Viewing: `/api/departments GET` joins `rbac.perm_grants` to derive `head_user_id`.

---

## 10. Tiles

DB catalog in `perm.tiles`. Visibility = `perm.tiles.required_permission` must be in `effective_user_perms`.

```
perm.tiles.required_permission IS the perm key (4-segment)
```

Frozen tile groups: `hub`, `workflow`, `workflow-approval`, `workflow-procurement`, `finance`, `cockpit`, `policy`, `it`, `hr`.

---

## 11. Code map

| Concern | File |
|---|---|
| `hasPermission`, `canManageResource`, session hydration | `lib/perm/auth.ts` |
| `getActorScope` (parses 4th segment) | `lib/perm/scope.ts` |
| `getEffectiveLevel` (MIN of level perms) | `lib/perm/level.ts` |
| `permScope(perm)` helper | inline in `lib/perm/scope.ts` |
| CASL ability | `lib/perm/ability.ts` |
| Approval chain | `lib/perm/chain.ts` |
| `perm.effective_user_perms` view query | `lib/perm/auth.ts:hydratePermSession` |
| Acting CRUD (`createGrant`, `revokeGrant`, `grantActingBundle`) | `lib/perm/grants.ts` |
| Perm catalog constants | `lib/perm/taxonomy.ts` (3-segment, tolerate via hasPermission fallback) |
| Edge middleware | `app/src/middleware.ts` |
| Hooks (client perm check) | `app/src/lib/access/hooks.ts` |

---

## 12. UI tokens

`app/src/app/globals.tokens.css` — Tailwind 4 `@theme` directive.

Frozen tones: `emerald | amber | rose | indigo | cyan | purple | slate`.

Frozen level palette mapped to tones:
- L1–2: rose (executive)
- L3: amber (senior management)
- L4–5: cyan (middle management)
- L6: emerald (staff)
- L7–10: indigo → slate (junior → read-only)

Frozen buckets P1–P5 (UI groupings of levels).

---

## 13. Migration recipes

### Add a new permission

```sql
INSERT INTO perm.permissions (domain, subject, verb, scope, description)
VALUES ('finance', 'invoice', 'approve', 'all', 'Approve vendor invoice');

INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
VALUES
  ('cfo',              'finance:invoice:approve:all', 'allow', 'manual'),
  ('accounting_manager','finance:invoice:approve:all', 'allow', 'manual');
```

### Add a new tile

```sql
INSERT INTO perm.tiles (id, display_name, icon, accent, group_name, href, required_permission, sort_order)
VALUES ('invoice-approve', 'Invoice Approval', '📒', 'emerald', 'finance', '/invoice-approve', 'tile:invoice-approve:view:all', 50);

-- The tile:<slug>:view:all gate must exist in perm.permissions first.
```

### Add a new role

```sql
INSERT INTO perm.roles (id, display_name, kind, level, sort_order, is_system)
VALUES ('senior_accountant', 'Senior Accountant', 'persona', 4, 35, false);

INSERT INTO perm.role_permissions (role_id, permission_id, effect)
SELECT 'senior_accountant', id, 'allow' FROM perm.permissions
 WHERE domain = 'finance' AND scope = 'all';
```

### Add a new department

```sql
INSERT INTO perm.roles (id, display_name, display_name_th, kind, parent_role_id, sort_order, is_system)
VALUES ('dept-legal', 'Legal', 'ฝ่ายกฎหมาย', 'department', 'dept-hq', 11, true);

-- Assign members via perm.user_roles
INSERT INTO perm.user_roles (user_id, role_id, granted_by)
VALUES ($user_id, 'dept-legal', 'manual');
```

The trigger `perm.ur_one_dept` enforces single primary dept per user.

### Grant acting (HR portal)

```ts
import { grantActingBundle } from '@folio-lib/perm/grants';

await grantActingBundle({
  user_id: johnId,
  role_id: 'manager',
  ends_at: new Date(Date.now() + 14 * 86400_000).toISOString(),
  granted_by: hrId,
  reason: 'Sarah on leave',
});
```

### Set dept head (HR portal)

```ts
// Via /api/departments PATCH { department_id, head_user_id }
await fetch('/api/departments', {
  method: 'PATCH',
  body: JSON.stringify({ department_id: 'dept-engineering', head_user_id: 20 }),
});
```

---

## 14. Audit log

`perm.audit` table. Target is jsonb. Examples:

```json
{ "actor": "hr-1", "target": { "user_id": 20, "added": ["manager"], "removed": [] } }
{ "actor": "hr-1", "target": { "perm": "dept:engineering:head:3:all", "granted_to": 20 } }
{ "actor": "system.expiry", "target": { "user_id": 25, "perm": "finance:expense:approve:dept" } }
```

---

## 15. Verification

```sh
# Lint + tsc
cd web-admin && bun run lint && bunx tsc --noEmit

# DB sanity
psql -c "SELECT scope, count(*) FROM perm.permissions GROUP BY scope;"
#   all | dept | subtree

psql -c "SELECT kind, count(*) FROM perm.roles GROUP BY kind;"
#   persona | department

psql -c "SELECT count(*) FROM rbac.perm_grants WHERE revoked_at IS NULL AND ends_at > now();"

# Endpoints (all should return 200)
for path in / /roles /perm /directory /departments /dashboard /audit \
             /api/perm/me /api/perm/users /api/perm/roles /api/perm/permissions \
             /api/departments /api/users /api/org-tree /api/tiles; do
  curl -o /dev/null -w "$path %{http_code}\n" -b /tmp/erp-cookies.txt http://localhost:3003$path
done
```

---

## 16. Out of scope (explicit non-goals)

- Multi-tenant RBAC
- IP / geo / time-of-day conditions
- Permission inheritance beyond `allow` / `deny`
- External IdP / SSO (cookie session only for PoC)
- ABAC attributes beyond `dept_group_id` and `reports_to_user_id`

---

## 17. Cleanup log (Steps 8–11, completed)

- ✅ `perm.roles.level` column kept but now derived — trigger `perm.sync_role_level_from_perms()` recomputes from level perm grants on every role_permissions change. Migration `0026_sync_level_column.sql`.
- ✅ `perm.acl_rules` table dropped — scope is declarative in the 4th perm segment. Migration `0027_drop_acl_rules.sql`. `lib/perm/ability.ts` rewritten to derive object-level conditions from `permScope(perm)` + the user's dept via `perm.user_roles`.
- ✅ `lib/perm/{scope,chain,level}.ts` KEPT (not deleted) — these are core (chain.ts is approval logic, scope.ts is row-level filtering in `lib/server/guard.ts`, level.ts is used by anyone querying effective authority). They were updated in Steps 2–3 to use the new model.
- ✅ `users.staff_level` column kept (legacy readers).

Final RBAC tables (after all 11 steps):
```
perm.roles               27 rows  (21 personas + 6 departments)
perm.permissions         165 rows (4-segment with scope)
perm.role_permissions    760 rows (level perms auto-grant)
perm.user_roles          38 rows  (persona + dept assignments)
perm.tiles               31 rows  (DB-driven tile catalog)
perm.audit               3 rows   (carry-over from prior migrations)
rbac.perm_grants         0 rows   (ready for acting + head)
```