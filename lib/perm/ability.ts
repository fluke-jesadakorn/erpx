// perm/ability.ts — CASL ability builder.
//
// Loads a user's role grants (each grant is a full '<d>:<s>:<v>[:q]::<effect>'
// string), then splits into allow/deny sets via grammar.effectOf().
// Object-level conditions are derived from the qualifier segment.

import 'server-only';
import { AbilityBuilder, createMongoAbility, type MongoAbility } from '@casl/ability';
import { query } from '../db';
import {
  effectOf, parseDeptFromPerms, type Effect,
} from './grammar';

export type Actions = string;
export type Subjects = string | { [k: string]: any };
export type AppAbility = MongoAbility<[Actions, Subjects]>;
export const createAppAbility = () => createMongoAbility();

export async function loadUserRoleIds(userId: number): Promise<string[]> {
  const { rows } = await query<{ role_id: string }>(
    `SELECT role_id FROM perm.user_roles WHERE user_id = $1`,
    [userId],
  );
  return rows.map((r) => r.role_id);
}

export async function loadRoleGrants(roleIds: string[]): Promise<{ allow: Set<string>; deny: Set<string> }> {
  const allow = new Set<string>();
  const deny = new Set<string>();
  if (roleIds.length === 0) return { allow, deny };
  const { rows } = await query<{ permission_id: string }>(
    `SELECT permission_id FROM perm.role_permissions WHERE role_id = ANY($1)`,
    [roleIds],
  );
  for (const r of rows) {
    const eff: Effect | null = effectOf(r.permission_id);
    if (eff === 'deny') deny.add(r.permission_id);
    else allow.add(r.permission_id);
  }
  return { allow, deny };
}

async function loadUserDirectPerms(userId: number): Promise<string[]> {
  const { rows } = await query<{ permission_id: string }>(
    `SELECT permission_id FROM perm.user_permissions
      WHERE user_id = $1 AND revoked_at IS NULL
        AND (ends_at IS NULL OR ends_at > now())`,
    [userId],
  );
  return rows.map((r) => r.permission_id);
}

function qualifierOf(perm: string): string | null {
  const idx = perm.indexOf('::');
  if (idx < 0) return null;
  const head = perm.slice(0, idx);
  const seg = head.split(':');
  return seg.length === 4 ? seg[3] : null;
}

async function loadUserDept(userId: number, permSet: Set<string>): Promise<string | null> {
  const r = await query<{ department_id: string }>(
    `SELECT department_id FROM perm.user_departments WHERE user_id = $1`,
    [userId],
  );
  if (r.rows[0]) return r.rows[0].department_id;
  const fromPerms = parseDeptFromPerms(permSet);
  if (fromPerms) return fromPerms;
  return null;
}

async function loadDeptSubtree(deptId: string): Promise<string[]> {
  const { rows } = await query<{ id: string }>(
    `SELECT id FROM perm.departments WHERE id = $1`,
    [deptId],
  );
  return rows.map((r) => r.id);
}

export async function buildAbilityFor(userId: number): Promise<AppAbility> {
  const { can, cannot, build } = new AbilityBuilder<AppAbility>(createMongoAbility);

  const roleIds = await loadUserRoleIds(userId);
  const { allow, deny } = await loadRoleGrants(roleIds);
  const directPerms = await loadUserDirectPerms(userId);
  for (const p of directPerms) {
    if (effectOf(p) === 'deny') deny.add(p);
    else allow.add(p);
  }

  if (allow.has('admin:system:bypass::allow')) {
    can('manage', 'all');
    return build();
  }

  for (const p of allow) {
    const domain = p.split(':', 1)[0];
    can(p, domain);
  }
  for (const p of deny) {
    const domain = p.split(':', 1)[0];
    cannot(p, domain);
  }

  const userDept = await loadUserDept(userId, allow);
  const subtreeDepts = userDept ? new Set(await loadDeptSubtree(userDept)) : new Set<string>();

  for (const p of allow) {
    const q = qualifierOf(p);
    if (!q || q === '*' || q === 'all') continue;
    const domain = p.split(':', 1)[0];
    const subject = domain === 'rbac' ? 'User' : domain.charAt(0).toUpperCase() + domain.slice(1);
    if (q === 'self') {
      can(p, subject, { submitter_id: userId });
      can(p, subject, { requester_id: userId });
      can(p, subject, { owner_id: userId });
    } else if (q === 'dept' && userDept) {
      can(p, subject, { dept_group_id: userDept });
    } else if (q === 'subtree') {
      for (const d of subtreeDepts) can(p, subject, { dept_group_id: d });
    }
  }

  return build();
}
