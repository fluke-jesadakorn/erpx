import 'server-only';
import { query } from '@/db';
import { assertRole } from '@/perm/assertRole';
import { getUserLevels, getUserStaffLevels, resolveActorScope, loadOrgTree, type OrgNode } from '@/org/scope';

export async function listOrgTree(actorId: number) {
  try { await assertRole(actorId, [], { perm: 'tile:org_chart:view::allow' }); }
  catch { return { success: false as const, tree: [] }; }
  const tree: OrgNode[] = await loadOrgTree(actorId);
  return { success: true as const, tree };
}

export async function listDepartments(actorId: number) {
  try { await assertRole(actorId, [], { perm: 'tile:departments:view::allow' }); }
  catch { return { success: false as const, departments: [] }; }
  const r = await query(
    `SELECT DISTINCT split_part(id, ':', 3) AS id, split_part(id, ':', 3) AS name,
            NULL::text AS monthly_budget, NULL::int AS head_user_id,
            NULL::text AS head_fullname, NULL::text AS head_code,
            NULL::int AS active_members
       FROM perm.permissions
      WHERE id LIKE 'user:dept:%::allow'
      ORDER BY id`,
  );
  return { success: true as const, departments: r.rows };
}

export async function listUserDirectory(args: {
  actorId: number;
  filterRole?: string;
  filterDeptId?: number;
  includeInactive?: boolean;
}) {
  await assertRole(args.actorId, [], { perm: 'tile:directory:view::allow' });
  const scope = await resolveActorScope(args.actorId);
  if (!scope.isHrManager && !scope.isHr) {
    throw new Error('Permission denied');
  }

  const where: string[] = [];
  const params: any[] = [];
  if (args.filterRole) {
    params.push(args.filterRole);
    where.push(`r.role_name=$${params.length}`);
  }
  if (args.filterDeptId) {
    params.push(`user:dept:${args.filterDeptId}::allow`);
    where.push(`EXISTS (
      SELECT 1 FROM perm.active_user_permissions up
       WHERE up.user_id = u.id AND up.permission_id = $${params.length}
    )`);
  }
  if (!args.includeInactive) where.push('u.is_active=TRUE');
  const whereSql = where.length ? `WHERE ${where.join(' AND ')}` : '';

  const r = await query(
    `SELECT u.id, u.employee_code, u.fullname, u.is_active,
            u.line_user_id, u.created_at, r.role_id, r.role_name,
            NULL AS dept_code, dg.dept_id, dg.dept_id AS dept_name,
            NULL AS manager_name, NULL AS manager_code
     FROM users u
     LEFT JOIN LATERAL (
            SELECT ur.role_id, split_part(ur.role_id, '::', 1) AS role_name FROM perm.user_roles ur
              WHERE ur.user_id = u.id
              ORDER BY (CASE WHEN ur.role_id LIKE '%::1' THEN 0
                             WHEN ur.role_id LIKE '%::2' THEN 1
                             WHEN ur.role_id LIKE '%::3' THEN 2
                             WHEN ur.role_id LIKE '%::4' THEN 3
                             WHEN ur.role_id LIKE '%::5' THEN 4
                             ELSE 5 END), ur.granted_at ASC
              LIMIT 1
          ) r ON true
     LEFT JOIN LATERAL (
       SELECT split_part(up.permission_id, ':', 3) AS dept_id FROM perm.active_user_permissions up
         WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
         ORDER BY up.permission_id LIMIT 1
     ) dg ON true
     ${whereSql}
     ORDER BY u.id`,
    params
  );
  const levels = await getUserLevels();
  const staffLevels = await getUserStaffLevels();
  const users = r.rows.map((row: any) => ({
    ...row,
    level: levels.get(row.id) ?? 0,
    staff_level: staffLevels.get(row.id) ?? null,
  }));
  return { success: true as const, users };
}

export async function listRoleOptions(actorId: number) {
  await assertRole(actorId, [], { perm: 'tile:directory:view::allow' });
  const r = await query(`SELECT id, display_name AS name FROM perm.roles ORDER BY id`);
  return { success: true as const, roles: r.rows };
}
