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
    `SELECT d.id, d.display_name AS name, NULL::text AS monthly_budget,
            d.head_user_id, h.fullname AS head_fullname,
            h.employee_code AS head_code,
            count(u.id) FILTER (WHERE u.is_active IS TRUE)::int AS active_members
       FROM perm.departments d
       LEFT JOIN users h ON h.id = d.head_user_id
       LEFT JOIN perm.user_departments ud ON ud.department_id = d.id
       LEFT JOIN users u ON u.id = ud.user_id
      GROUP BY d.id, d.display_name, d.head_user_id, h.fullname, h.employee_code
      ORDER BY d.display_name`,
  );
  return { success: true as const, departments: r.rows };
}

export async function listUserDirectory(args: {
  actorId: number;
  filterRole?: string;
  filterDeptId?: string;
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
    params.push(args.filterDeptId);
    where.push(`ud.department_id = $${params.length}`);
  }
  if (!args.includeInactive) where.push('u.is_active=TRUE');
  const whereSql = where.length ? `WHERE ${where.join(' AND ')}` : '';

  const r = await query(
    `SELECT u.id, u.employee_code, u.fullname, u.is_active,
            u.line_user_id, u.created_at, r.role_id, r.role_name,
            NULL AS dept_code, ud.department_id AS dept_id, d.display_name AS dept_name,
            NULL AS manager_name, NULL AS manager_code
     FROM users u
     LEFT JOIN LATERAL (
            SELECT ur.role_id, ur.role_id AS role_name FROM perm.user_roles ur
              WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
              LIMIT 1
          ) r ON true
     LEFT JOIN perm.user_departments ud ON ud.user_id = u.id
     LEFT JOIN perm.departments d ON d.id = ud.department_id
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
