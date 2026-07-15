// lib/perm/chain.ts — approval chain resolver.
//
// Approver lookup joins perm.user_roles and parses the level from the
// role-id suffix. Department is derived from the user's `user:dept:<id>::allow`
// permission grants.

import 'server-only';
import { query } from '../db';
import type { StageName } from './stages';
import { STAGE_ORDER, normalizeStage, stagePrimaryRole, stageRoles } from './stages';
import { levelOf, roleNameOf } from './grammar';

export { STAGE_TO_ROLE, STAGE_TO_PERM, normalizeStage, stageRoles, stagePrimaryRole } from './stages';
export type { StageName } from './stages';
export { STAGE_ORDER } from './stages';

const OVERRIDE_ROLE_NAMES = new Set(['cfo', 'ceo', 'admin']);
const CROSS_DEPT_STAGES = new Set<string>([
  'accounting_verification',
  'accounting_supervision',
  'accounting_authorization',
  'disbursement_authorization',
  'cfo_authorization',
  'ceo_authorization',
  'po_cfo',
]);
const ALREADY_APPROVED_DEPT_KEYS = new Set<string>([
  'manager_review',
  'supervisor_review',
  'dept_verification',
  'dept_authorization',
]);

export interface ApproverCheck {
  stage: StageName;
  roleId: string;
  userId: number | null;
  approverLevel: number;
  source: 'perm_user_roles' | 'override';
}

export interface ResolverCtx {
  submitterUserId: number;
  submitterDeptId: string | null;
  submitterRoleId: string;
  submitterLevel: number;
  alreadyApproved: Set<StageName>;
}

export interface ActorCtx {
  userId: number;
  roleId: string;
  deptId: string | null;
  deptGroupId?: string | null;
  level: number;
}

async function findApprover(
  roleName: string,
  deptId: string,
): Promise<{ user_id: number | null; level: number } | null> {
  // Match any role-id whose name part equals `roleName` (e.g. 'manager' → 'manager::3').
  const r = await query<{ user_id: number | null; level: number | null }>(
    `SELECT ur.user_id,
            MIN(CASE
              WHEN ur.role_id ~ '\\m[0-9]+$'
                THEN split_part(ur.role_id, '::', 2)::int
              ELSE 5
            END) AS level
       FROM perm.user_roles ur
       JOIN users u ON u.id = ur.user_id
      WHERE split_part(ur.role_id, '::', 1) = $1
        AND u.is_active IS NOT FALSE
        AND EXISTS (
          SELECT 1 FROM perm.user_permissions up
           WHERE up.user_id = u.id
             AND up.permission_id = $2
             AND up.revoked_at IS NULL
        )
      GROUP BY ur.user_id
      ORDER BY level ASC NULLS LAST
      LIMIT 1`,
    [roleName, `user:dept:${deptId}::allow`],
  );
  if (r.rows.length === 0) return null;
  const row = r.rows[0];
  return { user_id: row.user_id, level: row.level ?? 5 };
}

export const CEO_ESCALATION_THRESHOLD_THB = 200_000;

export async function resolveApprovalChain(
  ctx: ResolverCtx,
  totalAmount?: number,
): Promise<{
  chain: ApproverCheck[];
  nextStage: StageName | null;
  completed: boolean;
  injectedCeo: boolean;
}> {
  const chain: ApproverCheck[] = [];
  const hasAmount = typeof totalAmount === 'number' && Number.isFinite(totalAmount);
  const needCeo = hasAmount && (totalAmount as number) >= CEO_ESCALATION_THRESHOLD_THB;
  const skipCeo = hasAmount && !needCeo;

  const withCeo: StageName[] = needCeo
    ? STAGE_ORDER.flatMap((s) =>
        s === 'cfo_authorization' ? (['cfo_authorization', 'ceo_authorization'] as StageName[]) : [s],
      )
    : skipCeo
    ? STAGE_ORDER.filter((s) => s !== 'ceo_authorization')
    : STAGE_ORDER;

  const seen = new Set<StageName>();
  const dynamicOrder: StageName[] = [];
  for (const s of withCeo) {
    if (seen.has(s)) continue;
    seen.add(s);
    dynamicOrder.push(s);
  }

  for (const stage of dynamicOrder) {
    const roleName = stagePrimaryRole(stage);
    if (!roleName) continue;
    let userId: number | null = null;
    let approverLevel = 5;
    if (ctx.submitterDeptId) {
      const assignee = await findApprover(roleName, ctx.submitterDeptId);
      userId = assignee?.user_id ?? null;
      approverLevel = assignee?.level ?? 5;
    }
    chain.push({ stage, roleId: roleName, userId, approverLevel, source: 'perm_user_roles' });
  }

  let nextStage: StageName | null = null;
  for (const c of chain) {
    if (!ctx.alreadyApproved.has(c.stage)) {
      nextStage = c.stage;
      break;
    }
  }

  return { chain, nextStage, completed: nextStage === null, injectedCeo: needCeo };
}

export async function canActOnStage(
  actor: ActorCtx,
  submitter: ResolverCtx,
  stage: StageName,
  alreadyApproved: Set<StageName>,
  actorPerms?: Set<string>,
): Promise<{ allow: boolean; reason: string }> {
  const roles = stageRoles(stage);
  if (roles.length === 0) return { allow: false, reason: `Unknown stage "${stage}"` };

  const actorRoleName = roleNameOf(actor.roleId);
  if (OVERRIDE_ROLE_NAMES.has(actorRoleName)) {
    const approvedArr = Array.from(alreadyApproved) as string[];
    const deptApproved = approvedArr.some((s) => ALREADY_APPROVED_DEPT_KEYS.has(s));
    if (!deptApproved) {
      return { allow: false, reason: `${actorRoleName.toUpperCase()} override requires manager or supervisor approval first` };
    }
    return { allow: true, reason: `Override by ${actorRoleName}` };
  }

  const hasStagePerm =
    actorPerms?.has(`stage:${stage}:act::allow`) === true ||
    actorPerms?.has(`stage:${stage}:act:all::allow`) === true;

  if (!hasStagePerm && !roles.includes(actorRoleName)) {
    return {
      allow: false,
      reason: `Stage "${stage}" requires one of roles [${roles.join(', ')}], actor is "${actorRoleName}"`,
    };
  }
  if (roles.includes(actorRoleName) && !CROSS_DEPT_STAGES.has(stage)) {
    if (!actor.deptId) {
      return { allow: false, reason: `Actor "${actorRoleName}" has no department binding` };
    }
    if (actor.deptId !== submitter.submitterDeptId) {
      return { allow: false, reason: `Stage "${stage}" requires same department as submitter` };
    }
  }
  if (actor.level > submitter.submitterLevel) {
    return {
      allow: false,
      reason: `Actor level ${actor.level} is below submitter level ${submitter.submitterLevel}`,
    };
  }
  return { allow: true, reason: hasStagePerm ? `Granted stage:${stage}:act` : 'Same-dept, sufficient tier' };
}

export async function getApprovedStages(
  entityType: 'expense' | 'pr',
  entityId: number,
): Promise<Set<StageName>> {
  const r = await query<{ stage: string }>(
    `SELECT DISTINCT stage FROM approval_transitions
      WHERE target_type = $1 AND target_id = $2
        AND stage IS NOT NULL
        AND new_status NOT IN ('rejected')`,
    [entityType, entityId],
  );
  const set = new Set<StageName>();
  for (const row of r.rows) {
    const norm = normalizeStage(row.stage);
    if (norm && STAGE_ORDER.includes(norm)) set.add(norm);
  }
  return set;
}

export function resolveNextStage(
  currentStage: string,
  _actorRole: string,
  _amount?: number,
  domain?: 'expense' | 'procurement' | 'sales',
): { stage: string; completed: boolean } | null {
  if (domain === 'sales') {
    const salesOrder = ['so_draft', 'so_sales_review', 'so_credit_check', 'so_invoiced', 'so_paid'];
    const idx = salesOrder.indexOf(currentStage);
    if (idx < 0 || idx >= salesOrder.length - 1) return null;
    return { stage: salesOrder[idx + 1], completed: false };
  }
  const norm = normalizeStage(currentStage);
  if (!norm) return null;
  const idx = STAGE_ORDER.indexOf(norm);
  if (idx < 0 || idx >= STAGE_ORDER.length - 1) return { stage: 'awaiting_disbursement', completed: true };
  return { stage: STAGE_ORDER[idx + 1], completed: false };
}

export function isFinalApprovalStage(stage: string): boolean {
  const norm = normalizeStage(stage);
  return norm === STAGE_ORDER[STAGE_ORDER.length - 1];
}

export { levelOf };
