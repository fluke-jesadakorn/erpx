import { v5 as uuidv5 } from 'uuid';
import { controlReadQuery, query } from '@/db';
import { matchPerm } from '@/perm/grammar';

export interface OperationsBranchScopeRequest {
  branchId: number | null;
  invalid: boolean;
}

export interface OperationsActorForScope {
  id: number;
  employee_code: string;
  permissions: string[];
}

export interface OperationsScopeBranch {
  id: number;
  ref: string;
  holdingId: string;
  code: string;
  name: string;
}

/**
 * The Operations page accepts legacy Folio branch IDs, but authorization is
 * owned by the platform role-assignment scope. Keep the mapping explicit so
 * a numeric query parameter can never widen a UUID holding/branch grant.
 */
export interface OperationsActorScope {
  platformWide: boolean;
  allowHoldingLevel: boolean;
  holdingIds: string[];
  branches: OperationsScopeBranch[];
}

export const OPERATIONS_SCOPE_UNAVAILABLE = 'operations scope authority is unavailable';
export const OPERATIONS_BRANCH_UNAVAILABLE = 'requested operations branch scope is unavailable';

function operationsScopeQuery<T extends Record<string, unknown> = Record<string, unknown>>(
  text: string,
  params: unknown[],
) {
  const controlReadUrl = process.env.ERPX_CONTROL_READ_DATABASE_URL?.trim();
  if (controlReadUrl) {
    if (process.env.NODE_ENV === 'production' && !/[?&]sslmode=verify-full(?:&|$)/i.test(controlReadUrl)) {
      return Promise.reject(new Error(OPERATIONS_SCOPE_UNAVAILABLE));
    }
    return controlReadQuery<T>(text, params);
  }
  if (process.env.NODE_ENV !== 'production' && process.env.ERPX_LOCAL_BREAK_GLASS_READ === 'true') {
    return query<T>(text, params);
  }
  return Promise.reject(new Error(OPERATIONS_SCOPE_UNAVAILABLE));
}

export function parseOperationsBranchParam(value: string | undefined): OperationsBranchScopeRequest {
  const raw = value?.trim() ?? '';
  if (!raw) return { branchId: null, invalid: false };
  if (!/^[1-9]\d*$/.test(raw)) return { branchId: null, invalid: true };
  const branchId = Number(raw);
  return Number.isSafeInteger(branchId) ? { branchId, invalid: false } : { branchId: null, invalid: true };
}

export function operationsPrincipalRefs(actor: OperationsActorForScope): string[] {
  const refs = [
    `folio:user:${actor.id}`,
    `folio:employee:${actor.employee_code}`,
    String(actor.id),
    actor.employee_code,
  ];
  return [...new Set(refs.filter((ref) => ref.trim().length > 0))];
}

/** The stable UUID used by ERPX for an opaque tenant holding reference. */
export function operationsHoldingScopeId(holdingRef: string): string {
  return uuidv5(`erpx:holding-scope:v1:${holdingRef}`, uuidv5.URL);
}

export function operationsBranchIsAllowed(scope: OperationsActorScope, branchId: number | null): boolean {
  if (scope.platformWide || branchId === null) return true;
  return scope.branches.some((branch) => branch.id === branchId);
}

export function operationsHoldingIsAllowed(scope: OperationsActorScope, holdingRef: string): boolean {
  return scope.platformWide || scope.holdingIds.includes(operationsHoldingScopeId(holdingRef));
}

export function operationsBranchRef(scope: OperationsActorScope, branchId: number): string | null {
  return scope.branches.find((branch) => branch.id === branchId)?.ref ?? null;
}

export function operationsReadBranchRefs(
  scope: OperationsActorScope,
  branchId: number | null,
): string[] | null {
  if (branchId !== null) {
    const ref = operationsBranchRef(scope, branchId);
    return ref ? [ref] : [];
  }
  return scope.platformWide ? null : scope.branches.map((branch) => branch.ref);
}

export async function resolveOperationsActorScope(
  actor: OperationsActorForScope,
): Promise<OperationsActorScope> {
  if (matchPerm(actor.permissions, 'admin:system:bypass::allow')) {
    return {
      platformWide: true,
      allowHoldingLevel: true,
      holdingIds: [],
      branches: [],
    };
  }

  const principalRefs = operationsPrincipalRefs(actor);
  try {
    const [branches, level] = await Promise.all([
      operationsScopeQuery<{
        branch_id: string;
        branch_ref: string;
        holding_id: string;
        code: string;
        name: string;
      }>(
        `SELECT DISTINCT
            fb.id::text AS branch_id,
            pb.id::text AS branch_ref,
            h.id::text AS holding_id,
            fb.code,
            fb.name
           FROM platform.role_assignments ra
           JOIN platform.holdings h
             ON h.status = 'active'
            AND (
              ra.holding_id IS NULL
              OR ra.holding_id = h.id
            )
           JOIN platform.companies co
             ON co.holding_id = h.id
            AND co.status = 'active'
            AND (ra.company_id IS NULL OR ra.company_id = co.id)
           JOIN platform.branches pb
             ON pb.company_id = co.id
            AND pb.status = 'active'
            AND (
              ra.branch_id IS NULL
              OR ra.branch_id = pb.id
              OR (ra.subtree_scope->'branchIds') ? pb.id::text
              OR (ra.subtree_scope->'companyIds') ? co.id::text
            )
           JOIN finance.branches fb
             ON fb.code = pb.branch_code
            AND fb.active
          WHERE ra.principal_type = 'platform_user'
            AND ra.principal_ref = ANY($1::text[])
            AND ra.valid_from <= now()
            AND ra.valid_until > now()
            AND ra.revoked_at IS NULL
            AND (
              ra.holding_id IS NULL
              OR ra.holding_id = h.id
            )
            AND (
              ra.company_id IS NULL
              OR ra.company_id = co.id
            )
          ORDER BY fb.code`,
        [principalRefs],
      ),
      operationsScopeQuery<{ holding_id: string | null; allow_holding_level: boolean }>(
        `SELECT DISTINCT
            ra.holding_id::text AS holding_id,
            (
              ra.company_id IS NULL
              AND ra.branch_id IS NULL
              AND jsonb_array_length(COALESCE(ra.subtree_scope->'companyIds', '[]'::jsonb)) = 0
              AND jsonb_array_length(COALESCE(ra.subtree_scope->'branchIds', '[]'::jsonb)) = 0
            ) AS allow_holding_level
           FROM platform.role_assignments ra
          WHERE ra.principal_type = 'platform_user'
            AND ra.principal_ref = ANY($1::text[])
            AND ra.valid_from <= now()
            AND ra.valid_until > now()
            AND ra.revoked_at IS NULL`,
        [principalRefs],
      ),
    ]);

    const mappedBranches = branches.rows.map((row) => ({
      id: Number(row.branch_id),
      ref: row.branch_ref,
      holdingId: row.holding_id,
      code: row.code,
      name: row.name,
    })).filter((branch) => Number.isSafeInteger(branch.id) && branch.id > 0);
    const holdingIds = [...new Set([
      ...level.rows
      .map((row) => row.holding_id)
      .filter((holdingId): holdingId is string => Boolean(holdingId)),
      ...mappedBranches.map((branch) => branch.holdingId),
    ])];
    const platformWide = level.rows.some((row) => row.holding_id === null && row.allow_holding_level);
    const allowHoldingLevel = platformWide || level.rows.some((row) => row.allow_holding_level);

    if (!level.rows.length || (!platformWide && !mappedBranches.length)) {
      throw new Error(OPERATIONS_SCOPE_UNAVAILABLE);
    }
    return {
      platformWide,
      allowHoldingLevel,
      holdingIds,
      branches: mappedBranches,
    };
  } catch (error) {
    if (error instanceof Error && error.message === OPERATIONS_SCOPE_UNAVAILABLE) throw error;
    throw new Error(OPERATIONS_SCOPE_UNAVAILABLE);
  }
}
