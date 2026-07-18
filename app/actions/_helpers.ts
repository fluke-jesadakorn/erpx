import { loadActor, type ActorWithScope } from '@/server/guard';
import { hasPermission, type StageName, type ActorCtx, type ResolverCtx } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';
import { query } from '@/db';

export interface WbForCheck {
  id: string;
  current_stage: string;
  origin: 'expense' | 'pr' | 'po' | 'so';
  submitter_id: number | null;
  status: string;
}

export async function actorForWaybill(): Promise<ActorWithScope> {
  const actor = await loadActor();
  if (!actor) throw new Error('unauthorized');
  return actor;
}

export function canConfirmGl(actor: ActorWithScope, wb: WbForCheck): boolean {
  return wb.current_stage === 'settlement'
    && hasPermission(actor, PERM.finance.expense.settlement_post);
}

export function canSaveProcurementAccrual(actor: ActorWithScope): boolean {
  if (hasPermission(actor, PERM.admin.system.bypass)) return true;
  return hasPermission(actor, 'finance:gl:post::allow');
}
export async function fetchActorCtx(userId: number): Promise<ActorCtx> {
  const r = await query<{
    role_id: string | null;
    dept_id: string | null;
    level: number;
  }>(
    `SELECT (SELECT ud.department_id FROM perm.user_departments ud
              WHERE ud.user_id = u.id) AS dept_id,
            (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
              LIMIT 1) AS role_id,
            COALESCE((SELECT r.rank FROM perm.user_roles ur
                       JOIN perm.roles r ON r.id = ur.role_id AND r.kind = ur.role_kind
                      WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
                      LIMIT 1), 99)::int AS level
       FROM users u
      WHERE u.id = $1`,
    [userId],
  );
  const row = r.rows[0];
  const deptId = row?.dept_id ?? null;
  return {
    userId,
    roleId: row?.role_id ?? '',
    deptId,
    deptGroupId: deptId,
    level: row?.level ?? 99,
  };
}

export async function fetchSubmitterCtx(expenseId: number): Promise<ResolverCtx> {
  const r = await query<{
    submitter_id: number;
    dept_id: string | null;
    role_id: string | null;
    level: number;
  }>(
    `SELECT e.submitter_id,
            (SELECT ud.department_id FROM perm.user_departments ud
              WHERE ud.user_id = u.id) AS dept_id,
            (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
              LIMIT 1) AS role_id,
            COALESCE((SELECT r.rank FROM perm.user_roles ur
                       JOIN perm.roles r ON r.id = ur.role_id AND r.kind = ur.role_kind
                      WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
                      LIMIT 1), 99)::int AS level
FROM expenses e
       JOIN users u ON u.id = e.submitter_id
       WHERE e.id = $1`,
    [expenseId],
  );
  const row = r.rows[0];
  const deptId = row?.dept_id ?? null;
  return {
    submitterUserId: row?.submitter_id ?? 0,
    submitterDeptId: deptId,
    submitterRoleId: row?.role_id ?? '',
    submitterLevel: row?.level ?? 99,
    alreadyApproved: new Set<StageName>(),
  };
}

export async function fetchPrSubmitterCtx(prId: number): Promise<ResolverCtx> {
  const r = await query<{
    requester_id: number;
    dept_id: string | null;
    role_id: string | null;
    level: number;
  }>(
    `SELECT pr.requester_id,
            (SELECT ud.department_id FROM perm.user_departments ud
              WHERE ud.user_id = u.id) AS dept_id,
            (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
              LIMIT 1) AS role_id,
            COALESCE((SELECT r.rank FROM perm.user_roles ur
                       JOIN perm.roles r ON r.id = ur.role_id AND r.kind = ur.role_kind
                      WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
                      LIMIT 1), 99)::int AS level
       FROM purchase_requisitions pr
       JOIN users u ON u.id = pr.requester_id
      WHERE pr.id = $1`,
    [prId],
  );
  const row = r.rows[0];
  const deptId = row?.dept_id ?? null;
  return {
    submitterUserId: row?.requester_id ?? 0,
    submitterDeptId: deptId,
    submitterRoleId: row?.role_id ?? '',
    submitterLevel: row?.level ?? 99,
    alreadyApproved: new Set<StageName>(),
  };
}
