# Stage Approval — Tiered Access Design

> **Historical snapshot.** This doc was written before the folio flatten
> (2026-07-16) and the inline-effect perm grammar. The references to
> `app/src/app/actions.ts:441` / `:809` and to `rbac_role_id` levels
> (`L3`, `L4`) are out of date. The current state is:
>
> - Stages live in `lib/perm/stages.ts` (FINANCE_STANDARD enum, 12 pips);
>   canonical finance-standard keys (`accounting_verification`,
>   `accounting_supervision`, `accounting_authorization`,
>   `disbursement_authorization`, `cfo_authorization`, `ceo_authorization`,
>   `awaiting_disbursement`, `disbursed`, `rejected`).
> - Action handlers live in `app/actions/{expense,procurement,waybill,sales,slips,ai,hr,law}.ts`
>   (still server-only).
> - Roles use the `<name>::<level>` form (1 = CEO, 10 = lowest). The matrix
>   matches via `lib/perm/chain.ts` → `canActOnStage()` against
>   `lib/perm/taxonomy.ts` → PERM catalog.
> - Waybill approval actions call `appendWaybillEvent()` from
>   `lib/waybill/append.ts` (signed audit log).

The rest of this doc is retained as a historical record of the original
design problem and Option A resolution.

## Problem

`app/src/app/actions.ts:441` (expense) and `:809` (PR) throw when an actor's RBAC role does not have `update` on the stage module matching the expense's current `status`.

Today the matrix is strict:

| Stage | RBAC role with `update` |
|---|---|
| `stage-supervisor-review` | L2B |
| `stage-manager-review` | L3 |
| `stage-account-officer-review` | L2B |
| `stage-account-supervisor-review` | L2A |
| `stage-accounting-review` | L3 |
| `stage-cfo-review` | L4 |
| `stage-po-pending` | L3 |
| `stage-po-cfo` | L4 |

So an `accounting_manager` (Emily, `rbac_role_id='L3'`) encounters the error whenever an expense sits at `manager_review` — even though the matrix actually grants L3 `update` on `stage-manager-review`. The error fires only for actors whose `rbac_role_id` lacks the grant for the *current* stage module.

## Goal (from product)

1. **All accountants can approve** — `account_officer`, `account_supervisor`, `accounting_manager` can advance any stage.
2. **Higher level of department can approve** — `manager_of_department`, `cfo` can advance any stage.
3. **CEO can override approve** — dedicated `ceoForceDecision` path with mandatory reason + audit row in `ceo_overrides`.

## Recommended design — Option A: Broad matrix grants

Grant `update` on every `stage-*` module to L3 and L4. Keep L2A/L2B as they are (lower tier — restricted to their own stages). CEO override path stays untouched.

Resulting matrix after change:

| Stage | L2A | L2B | L3 | L4 |
|---|---|---|---|---|
| stage-supervisor-review | – | ✓ | **+** | **+** |
| stage-manager-review | – | – | ✓ | **+** |
| stage-account-officer-review | – | ✓ | **+** | **+** |
| stage-account-supervisor-review | ✓ | – | **+** | **+** |
| stage-accounting-review | – | – | ✓ | **+** |
| stage-cfo-review | – | – | **+** | ✓ |
| stage-po-pending | – | – | ✓ | **+** |
| stage-po-cfo | – | – | **+** | ✓ |

(L4 already had everything from the original seed; only L3 changes for 6 modules.)

This matches the user's intent exactly:
- **Accountants** = `accounting_manager` (L3) + `account_officer`/`account_supervisor` (L2B/L2A) — but the user's wording is "all accountants can approve". The legacy seed maps `account_officer`→L2B and `account_supervisor`→L2A, so we additionally grant L2A and L2B all stage modules (or at least the ones above the account-officer tier). Tightened version: grant L2B all, keep L2A on its own stage (account supervisor is operational, not a reviewer tier).
- **Higher dept levels** = L3 (`manager_of_department` is co-mapped with `accounting_manager`) + L4 (cfo).
- **CEO override** = existing `ceoForceDecision` server action + `ceo_overrides` audit table.

### Why matrix-driven, not code-driven

- `evaluateStage()` already routes through `isAccessAllowed` → `resolveCellWithGroups` → matrix.
- The matrix editor UI (`/admin/matrix`) is the configured control surface; granting via seed keeps it visible to admins.
- No code change in `lib/rbac/stage.ts` or `actions.ts:441/809` — the throw stays as a safety net for roles not in the broad tier.

### Tradeoffs

| | Pros | Cons |
|---|---|---|
| **Option A** (matrix grants) | Single source of truth, no code change, configurable per role | Initial-stage picker (`APPROVER_TO_STAGE`) still picks by first chain role — semantic of "who's expected to act first" stays, but anyone in the tier can act |
| Option B (drop gate, role-list check) | Simplest code | Bypasses matrix; loses admin configurability |
| Option C (new `perm-can-advance-any-stage` module) | Most explicit | Extra module, two-tier check, harder to reason about |

## Implementation

### 1. New seed migration — `db/perm/9001-seed-new-grammar.sql`

```sql
BEGIN;

-- Wipe prior seed-0017 stage grants so the file is re-runnable.
DELETE FROM rbac.permissions
 WHERE module_id LIKE 'stage-%'
   AND updated_by = 'seed-0017';

-- L3 (manager_of_department, accounting_manager): grant update on every stage.
INSERT INTO rbac.permissions (role_id, module_id, action, state, updated_by)
SELECT 'L3', m.id, 'update', 'allow', 'seed-0017'
FROM rbac.modules m
WHERE m.id LIKE 'stage-%'
ON CONFLICT DO NOTHING;

-- L4 (admin, ceo, cfo, it): already has everything from seed-0006, but be
-- explicit so future schema drift doesn't lose coverage.
INSERT INTO rbac.permissions (role_id, module_id, action, state, updated_by)
SELECT 'L4', m.id, 'update', 'allow', 'seed-0017'
FROM rbac.modules m
WHERE m.id LIKE 'stage-%'
ON CONFLICT DO NOTHING;

-- L2B (supervisor, account_officer): grant update on every stage so
-- "all accountants can approve" is true even for the L2B-tier personas.
INSERT INTO rbac.permissions (role_id, module_id, action, state, updated_by)
SELECT 'L2B', m.id, 'update', 'allow', 'seed-0017'
FROM rbac.modules m
WHERE m.id LIKE 'stage-%'
ON CONFLICT DO NOTHING;

INSERT INTO rbac.audit (kind, actor, target)
SELECT 'matrix.bulk_grant', 'seed-0017',
       jsonb_build_object('role_id', role_id, 'module_id', module_id, 'action', 'update')
FROM rbac.permissions
WHERE updated_by = 'seed-0017'
ON CONFLICT DO NOTHING;

COMMIT;
```

> **Note on L2A (`account_supervisor`)**: keeping its access on its own stage only. If "all accountants" must include `account_supervisor` too, add the same L2A grant block.

### 2. Chain picker — `app/src/app/actions.ts`

The initial-stage assignment at `:302` (expense) and `:665` (PR) picks the stage from `APPROVER_TO_STAGE[chain[0]]`. With broad L3+L4 access, this still works — first chain role determines who is *expected* to act first, but the tier lets others skip ahead.

No code change required. If we want to express "any tier can pick up the first action", we leave the initial status as-is.

### 3. Audit trail — `approval_logs` / `pr_approval_logs`

Already records actor + previous_status + new_status + stage + comments. When an accounting_manager acts on a `manager_review` expense, the log will show:

```
actor=Emily Manager (accounting_manager)
previous_status=manager_review  → new_status=accounting_review (or approved if last in chain)
stage=manager_review
chain_index=1   -- because we still advance by chain index, not by actor
```

No change required. If the product wants a separate "tier-skip" audit flag, add `is_tier_skip BOOLEAN DEFAULT FALSE` and set it true when `actor.rbac_role_id` lacks the canonical stage role.

### 4. CEO override path — already exists

`ceoForceDecision()` at `app/src/app/actions.ts:493` writes to `ceo_overrides` + `approval_logs.stage='ceo_override'`. Gate is `requireActionFor(actorId, 'ceo_override', …)`. No change.

## Verification

1. Apply migration: `psql … -f db/perm/9001-seed-new-grammar.sql`
2. Check grants:
   ```sql
   SELECT role_id, count(*) FROM rbac.permissions
   WHERE module_id LIKE 'stage-%' AND action='update' AND state='allow'
   GROUP BY role_id ORDER BY role_id;
   -- expect: L2A=1, L2B=8, L3=8, L4=8
   ```
3. Static: `cd folio/app && bun run lint && bunx tsc --noEmit`
4. Smoke: as Emily Manager (accounting_manager), open an expense at `manager_review` and click Approve — should advance to `accounting_review` (or `approved` if last in chain) without the throw.
5. Boundary: as John Staff (L2A, not in any approver tier), clicking Approve on the same expense still throws — confirms the tier gate is real, not a free-for-all.

## Files touched

| File | Change |
|---|---|
| `folio/db/perm/9001-seed-new-grammar.sql` | **existing** — broad stage grants for L3/L4/L2B |
| `folio/app/src/app/actions.ts` | **none** (throw stays as safety net) |
| `folio/lib/perm/stages.ts` | **none** (matrix is the source of truth) |

## Rollback

```sql
DELETE FROM perm.user_permissions
 WHERE permission_id LIKE 'stage:%:act::allow'
   AND granted_by = 'seed-9001';
```

Restores the original seed-0006 grants exactly.