import { loadActor, type ActorWithScope } from '@/server/guard';
import { matchPerm } from '@/perm';
import type { StageName, ActorCtx, ResolverCtx } from '@/perm/server';
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
  return wb.current_stage === 'disbursed'
    && matchPerm(actor.permissions, 'finance:gl:confirm::allow');
}

export function canSaveProcurementAccrual(actor: ActorWithScope): boolean {
  if (matchPerm(actor.permissions, 'admin:system:bypass::allow')) return true;
  if (matchPerm(actor.permissions, 'finance:pr:edit::allow')) return true;
  return ['finance', 'account_officer', 'account_supervisor', 'accounting_manager'].includes(actor.role_name);
}
export async function fetchActorCtx(userId: number): Promise<ActorCtx> {
  const r = await query<{
    role_id: string | null;
    dept_id: string | null;
    level: number;
  }>(
    `SELECT (SELECT up.permission_id FROM perm.user_permissions up
              WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS dept_id,
            (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id
              ORDER BY split_part(ur.role_id, '::', 2)::int ASC NULLS LAST
              LIMIT 1) AS role_id,
            COALESCE((SELECT MIN(split_part(ur.role_id, '::', 2)::int)
                       FROM perm.user_roles ur WHERE ur.user_id = u.id), 5)::int AS level
       FROM users u
      WHERE u.id = $1`,
    [userId],
  );
  const row = r.rows[0];
  const deptId = row?.dept_id
    ? row.dept_id.replace(/^user:dept:/, '').replace(/::allow$/, '')
    : null;
  return {
    userId,
    roleId: row?.role_id ?? '',
    deptId,
    deptGroupId: deptId,
    level: row?.level ?? 5,
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
            (SELECT up.permission_id FROM perm.user_permissions up
              WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS dept_id,
            (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id
              ORDER BY split_part(ur.role_id, '::', 2)::int ASC NULLS LAST
              LIMIT 1) AS role_id,
            COALESCE((SELECT MIN(split_part(ur.role_id, '::', 2)::int)
                       FROM perm.user_roles ur WHERE ur.user_id = u.id), 5)::int AS level
FROM expenses e
       JOIN users u ON u.id = e.submitter_id
       WHERE e.id = $1`,
    [expenseId],
  );
  const row = r.rows[0];
  const deptId = row?.dept_id
    ? row.dept_id.replace(/^user:dept:/, '').replace(/::allow$/, '')
    : null;
  return {
    submitterUserId: row?.submitter_id ?? 0,
    submitterDeptId: deptId,
    submitterRoleId: row?.role_id ?? '',
    submitterLevel: row?.level ?? 5,
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
            (SELECT up.permission_id FROM perm.user_permissions up
              WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS dept_id,
            (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id
              ORDER BY split_part(ur.role_id, '::', 2)::int ASC NULLS LAST
              LIMIT 1) AS role_id,
            COALESCE((SELECT MIN(split_part(ur.role_id, '::', 2)::int)
                       FROM perm.user_roles ur WHERE ur.user_id = u.id), 5)::int AS level
       FROM purchase_requisitions pr
       JOIN users u ON u.id = pr.requester_id
      WHERE pr.id = $1`,
    [prId],
  );
  const row = r.rows[0];
  const deptId = row?.dept_id
    ? row.dept_id.replace(/^user:dept:/, '').replace(/::allow$/, '')
    : null;
  return {
    submitterUserId: row?.requester_id ?? 0,
    submitterDeptId: deptId,
    submitterRoleId: row?.role_id ?? '',
    submitterLevel: row?.level ?? 5,
    alreadyApproved: new Set<StageName>(),
  };
}
