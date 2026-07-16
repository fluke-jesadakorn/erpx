// lib/orgScope.ts — actor scope + org tree derivation.
//
// Reads role-id and dept directly from perm tables using the new grammar.
// No legacy rbac matrix; no separate dept column on users.

import 'server-only';
import { query } from '@/db';
import {
  parseRoleId, parseDeptFromPerms, matchPerm, parseLevelFromRoles,
} from '@/perm/server';

export interface OrgNode {
  id: number;
  fullname: string;
  employee_code: string;
  role_name: string;
  role_id: string | null;
  level: number;
  dept_id: string | null;
  dept_name: string | null;
  is_active: boolean;
  children: OrgNode[];
}

export interface UserScopeResult {
  actor: {
    id: number;
    role_id: string;
    role_name: string;
    level: number;
    dept_id: string | null;
  };
  permissions: string[];
  isHrManager: boolean;
  isHr: boolean;
  isHod: boolean;
  subtreeIds: number[];
}

async function fetchActor(actorId: number) {
  const r = await query<{
    id: number;
    role_id: string | null;
    permissions: string[];
  }>(
    `SELECT u.id,
       COALESCE((
         SELECT ur.role_id FROM perm.user_roles ur
          WHERE ur.user_id = u.id
          ORDER BY (CASE WHEN ur.role_id LIKE '%::1' THEN 0
                         WHEN ur.role_id LIKE '%::2' THEN 1
                         WHEN ur.role_id LIKE '%::3' THEN 2
                         WHEN ur.role_id LIKE '%::4' THEN 3
                         WHEN ur.role_id LIKE '%::5' THEN 4
                         ELSE 5 END), ur.granted_at ASC
          LIMIT 1
       ), 'officer::5') AS role_id,
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
           ) t
       ), ARRAY[]::text[]) AS permissions
      FROM users u WHERE u.id = $1`,
    [actorId],
  );
  if (r.rows.length === 0) throw new Error('Actor not found');
  return r.rows[0];
}

async function loadActorScope(actorId: number): Promise<UserScopeResult> {
  const actor = await fetchActor(actorId);
  const perms = actor.permissions ?? [];
  const parsed = parseRoleId(actor.role_id ?? 'officer::5');
  const role_name = parsed?.name ?? 'officer';
  const level = parsed?.level ?? 5;
  const dept_id = parseDeptFromPerms(perms);

  const isHrManager = matchPerm(perms, 'user:role:assign::allow') || matchPerm(perms, 'user:dept:edit::allow');
  const isHr = matchPerm(perms, 'tile:directory:view::allow');
  const isHod = matchPerm(perms, 'user:subtree:edit::allow');

  return {
    actor: { id: actor.id, role_id: actor.role_id ?? 'officer::5', role_name, level, dept_id },
    permissions: perms,
    isHrManager,
    isHr,
    isHod,
    subtreeIds: [],
  };
}

export async function resolveActorScope(actorId: number): Promise<UserScopeResult> {
  return loadActorScope(actorId);
}

export async function assertCanEditUser(
  scope: UserScopeResult,
  targetUserId: number,
): Promise<void> {
  if (scope.isHrManager) return;
  if (scope.isHod) {
    if (scope.actor.id === targetUserId) {
      throw new Error('You cannot edit your own account');
    }
    if (!scope.subtreeIds.includes(targetUserId)) {
      throw new Error('Outside your chain of command');
    }
    return;
  }
  throw new Error('Permission denied: must be an HR Manager or Head of Department');
}

export async function loadOrgTree(actorId: number): Promise<OrgNode[]> {
  await loadActorScope(actorId);

  const userRows = await query<{
    id: number;
    fullname: string;
    employee_code: string;
    is_active: boolean;
    role_id: string | null;
    dept_id: string | null;
  }>(
    `SELECT u.id, u.fullname, u.employee_code, u.is_active,
            (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id
              ORDER BY (CASE WHEN ur.role_id LIKE '%::1' THEN 0
                            WHEN ur.role_id LIKE '%::2' THEN 1
                            WHEN ur.role_id LIKE '%::3' THEN 2
                            WHEN ur.role_id LIKE '%::4' THEN 3
                            WHEN ur.role_id LIKE '%::5' THEN 4
                            ELSE 5 END), ur.granted_at ASC
              LIMIT 1) AS role_id,
            (SELECT up.permission_id FROM perm.user_permissions up
              WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS dept_id
       FROM users u
       ORDER BY u.id`,
  );

  const ROLE_RANK_LOCAL: Record<string, number> = {
    ceo: 0, cfo: 1, admin: 2,
    finance: 3, sales_supervisor: 4,
    hr_manager: 10, accounting_manager: 11, manager: 12, sales_rep: 13,
    account_supervisor: 20, supervisor: 21,
    account_officer: 22, officer: 23, hr: 24, it: 25,
  };
  const tier = (role: string) => ROLE_RANK_LOCAL[role] ?? 99;

  const nodes = new Map<number, OrgNode>();
  for (const u of userRows.rows) {
    const parsed = parseRoleId(u.role_id ?? 'officer::5');
    nodes.set(u.id, {
      id: u.id,
      fullname: u.fullname,
      employee_code: u.employee_code,
      role_id: u.role_id,
      role_name: parsed?.name ?? 'officer',
      level: parsed?.level ?? 5,
      dept_id: u.dept_id ? u.dept_id.replace(/^user:dept:/, '').replace(/::allow$/, '') : null,
      dept_name: null,
      is_active: u.is_active,
      children: [],
    });
  }

  const sortByHierarchy = (a: OrgNode, b: OrgNode) => {
    const tr = tier(a.role_name) - tier(b.role_name);
    if (tr !== 0) return tr;
    return (a.fullname || '').localeCompare(b.fullname || '');
  };
  for (const node of nodes.values()) node.children.sort(sortByHierarchy);

  const roots: OrgNode[] = [];
  for (const node of nodes.values()) roots.push(node);
  roots.sort(sortByHierarchy);

  const stack: { node: OrgNode; depth: number }[] = roots.map((n) => ({ node: n, depth: 0 }));
  while (stack.length) {
    const { node, depth } = stack.pop() as { node: OrgNode; depth: number };
    node.level = depth;
    for (const child of node.children) stack.push({ node: child, depth: depth + 1 });
  }
  return roots;
}

export async function getUserLevels(): Promise<Map<number, number>> {
  const r = await query<{ id: number; role_id: string | null }>(
    `SELECT u.id,
            (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id LIMIT 1) AS role_id
       FROM users u`,
  );
  const levels = new Map<number, number>();
  for (const row of r.rows) {
    const parsed = parseRoleId(row.role_id ?? 'officer::5');
    levels.set(row.id, parsed?.level ?? 5);
  }
  return levels;
}

export async function getUserStaffLevels(): Promise<Map<number, number>> {
  const r = await query<{ user_id: number; role_ids: string[] | null }>(
    `SELECT ur.user_id, array_agg(ur.role_id) AS role_ids
       FROM perm.user_roles ur
      GROUP BY ur.user_id`,
  );
  const out = new Map<number, number>();
  for (const row of r.rows) {
    out.set(row.user_id, parseLevelFromRoles(row.role_ids ?? []));
  }
  return out;
}
