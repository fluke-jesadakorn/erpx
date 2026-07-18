// lib/assertRole.ts — server-side role/permission gate.
//
// Reads the actor's role-id via the new grammar. Accepts role NAMES
// (without the ::level suffix). When `opts.perm` is given, also gates via
// the perm-string system.

import 'server-only';
import { query } from '@/db';
import { matchPerm, parseRoleId } from '@/perm/server';

export async function assertRole(
  actorId: number,
  allowedRoles: string[],
  opts?: { perm?: string },
): Promise<string> {
  const res = await query<{ role_id: string | null; permissions: string[] }>(
    `SELECT (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
              LIMIT 1) AS role_id,
     COALESCE((
       SELECT array_agg(DISTINCT p_id ORDER BY p_id)
         FROM (
           SELECT rp.permission_id AS p_id
             FROM perm.user_roles ur
             JOIN perm.role_permissions rp ON rp.role_id = ur.role_id
            WHERE ur.user_id = u.id
           UNION
           SELECT permission_id AS p_id
             FROM perm.user_permissions
            WHERE user_id = u.id AND revoked_at IS NULL
              AND (ends_at IS NULL OR ends_at > now())
           UNION
           SELECT dp.permission_id AS p_id
             FROM perm.user_departments ud
             JOIN perm.department_permissions dp ON dp.department_id = ud.department_id
            WHERE ud.user_id = u.id
         ) t
     ), ARRAY[]::text[]) AS permissions
      FROM users u WHERE u.id = $1`,
    [actorId],
  );
  if (res.rows.length === 0) throw new Error('User not found');
  const row = res.rows[0];
  const roleId = row.role_id ?? 'unconfigured';
  const roleName = parseRoleId(roleId)?.name ?? 'unconfigured';
  if (!allowedRoles.includes(roleName)) {
    throw new Error(
      `Permission denied: role "${roleName}" is not authorized (required: ${allowedRoles.join(', ')})`,
    );
  }
  if (opts?.perm) {
    const allowed = matchPerm(row.permissions ?? [], opts.perm);
    if (!allowed) {
      throw new Error(`Permission denied: missing "${opts.perm}"`);
    }
  }
  return roleName;
}
