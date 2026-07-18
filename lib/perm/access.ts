import 'server-only';
import { withTransaction } from '../db';
import { authorize, type ActorForAuth } from './authorize';

export interface AccessInput {
  departmentId: string;
  hierarchyRoleId: string;
  systemRoleIds: string[];
}

export interface AccessActor extends ActorForAuth {
  departmentId?: string | null;
}

export class AccessError extends Error {
  status: number;

  constructor(message: string, status = 400) {
    super(message);
    this.name = 'AccessError';
    this.status = status;
  }
}

interface AccessRow {
  department_id: string | null;
  hierarchy_role_id: string | null;
  system_role_ids: string[];
}

function validId(id: string): boolean {
  return /^[a-z][a-z0-9_-]*$/.test(id);
}

async function deny(actorId: number, targetId: number, reason: string): Promise<never> {
  await withTransaction(async (q) => {
    await q(
      `INSERT INTO perm.decision_log
         (actor_user_id, action_kind, action_target, resource_type, resource_id, decision, reason)
       VALUES ($1, 'perm', 'perm:rbac:role:assign::allow', 'user', $2, 'deny', $3)`,
      [actorId, String(targetId), reason],
    );
  });
  throw new AccessError(reason, 403);
}

export async function setUserAccess(
  actor: AccessActor,
  userId: number,
  input: AccessInput,
): Promise<{ before: AccessRow; after: AccessRow }> {
  const decision = await authorize(
    actor,
    { kind: 'perm', perm: 'rbac:role:assign::allow' },
    { type: 'user', id: userId },
  );
  if (!decision.allow) throw new AccessError(decision.reason, 403);
  if (actor.id === userId) await deny(actor.id, userId, 'Users cannot change their own access');
  if (!validId(input.departmentId) || !validId(input.hierarchyRoleId)) {
    throw new AccessError('Invalid department or hierarchy role');
  }
  const systemRoleIds = [...new Set(input.systemRoleIds)];
  if (systemRoleIds.length > 1 || systemRoleIds.some((id) => !validId(id))) {
    throw new AccessError('At most one valid system role may be assigned');
  }

  return withTransaction(async (q) => {
    const user = await q<{ id: number; is_active: boolean }>(
      `SELECT id, is_active FROM users WHERE id = $1 FOR UPDATE`,
      [userId],
    );
    if (!user.rows[0]) throw new AccessError('User not found', 404);
    if (!user.rows[0].is_active) throw new AccessError('Inactive users cannot be assigned access');

    const dept = await q<{ id: string }>(
      `SELECT id FROM perm.departments WHERE id = $1`,
      [input.departmentId],
    );
    if (!dept.rows[0]) throw new AccessError('Unknown department');

    const hierarchy = await q<{ id: string; department_id: string }>(
      `SELECT id, department_id FROM perm.roles WHERE id = $1 AND kind = 'hierarchy'`,
      [input.hierarchyRoleId],
    );
    if (!hierarchy.rows[0]) throw new AccessError('Unknown hierarchy role');
    if (hierarchy.rows[0].department_id !== input.departmentId) {
      throw new AccessError('Hierarchy role does not belong to the selected department');
    }

    if (systemRoleIds.length) {
      const systems = await q<{ id: string }>(
        `SELECT id FROM perm.roles WHERE kind = 'system' AND id = ANY($1::text[])`,
        [systemRoleIds],
      );
      if (systems.rows.length !== systemRoleIds.length) throw new AccessError('Unknown system role');
    }

    const existing = await q<AccessRow>(
      `SELECT ud.department_id,
              (SELECT ur.role_id FROM perm.user_roles ur
                WHERE ur.user_id = $1 AND ur.role_kind = 'hierarchy') AS hierarchy_role_id,
              COALESCE((SELECT array_agg(ur.role_id ORDER BY ur.role_id)
                          FROM perm.user_roles ur
                         WHERE ur.user_id = $1 AND ur.role_kind = 'system'), ARRAY[]::text[]) AS system_role_ids
         FROM (SELECT $1::int AS user_id) x
         LEFT JOIN perm.user_departments ud ON ud.user_id = x.user_id`,
      [userId],
    );
    const before = existing.rows[0] ?? {
      department_id: null,
      hierarchy_role_id: null,
      system_role_ids: [],
    };

    if (
      before.department_id &&
      ['hr', 'it'].includes(before.department_id) &&
      !['hr', 'it'].includes(input.departmentId)
    ) {
      const admins = await q<{ count: number }>(
        `SELECT count(*)::int AS count
           FROM perm.user_departments ud
           JOIN users u ON u.id = ud.user_id AND u.is_active IS TRUE
           JOIN perm.user_roles ur ON ur.user_id = ud.user_id AND ur.role_kind = 'hierarchy'
          WHERE ud.department_id IN ('hr', 'it') AND ud.user_id <> $1`,
        [userId],
      );
      if ((admins.rows[0]?.count ?? 0) === 0) {
        throw new AccessError('The final access administrator cannot be removed', 409);
      }
    }

    await q(
      `INSERT INTO perm.user_departments (user_id, department_id, assigned_by)
       VALUES ($1, $2, $3)
       ON CONFLICT (user_id) DO UPDATE
         SET department_id = EXCLUDED.department_id,
             assigned_by = EXCLUDED.assigned_by,
             assigned_at = now()`,
      [userId, input.departmentId, actor.id],
    );
    await q(
      `DELETE FROM perm.user_roles WHERE user_id = $1`,
      [userId],
    );
    await q(
      `INSERT INTO perm.user_roles (user_id, role_id, role_kind, granted_by)
       VALUES ($1, $2, 'hierarchy', $3)`,
      [userId, input.hierarchyRoleId, `user:${actor.id}`],
    );
    for (const roleId of systemRoleIds) {
      await q(
        `INSERT INTO perm.user_roles (user_id, role_id, role_kind, granted_by)
         VALUES ($1, $2, 'system', $3)`,
        [userId, roleId, `user:${actor.id}`],
      );
    }
    await q(
      `UPDATE perm.user_permissions
          SET revoked_at = now(), revoked_by = $2
        WHERE user_id = $1
          AND permission_id LIKE 'user:dept:%::allow'
          AND revoked_at IS NULL`,
      [userId, `user:${actor.id}`],
    );
    await q(
      `INSERT INTO perm.user_permissions (user_id, permission_id, granted_by, reason)
       VALUES ($1, $2, $3, 'Department compatibility grant')`,
      [userId, `user:dept:${input.departmentId}::allow`, `user:${actor.id}`],
    );

    const after: AccessRow = {
      department_id: input.departmentId,
      hierarchy_role_id: input.hierarchyRoleId,
      system_role_ids: systemRoleIds,
    };
    await q(
      `INSERT INTO perm.audit (kind, actor, target)
       VALUES ('access.assign', $1, $2)`,
      [`user:${actor.id}`, { user_id: userId, before, after }],
    );
    return { before, after };
  });
}
