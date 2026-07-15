// lib/perm/level.ts — derive a user's effective authority level from role-ids.
//
// Authority level is encoded in the role-id suffix: 'manager::3', 'ceo::1'.
// A user's effective level = MIN(level) over their assigned roles.
// Lower level = higher authority.

import 'server-only';
import { query } from '../db';
import { parseLevelFromRoles } from './grammar';

export type Level = number;

export async function getEffectiveLevel(userId: number): Promise<Level> {
  const { rows } = await query<{ role_id: string }>(
    `SELECT role_id FROM perm.user_roles WHERE user_id = $1`,
    [userId],
  );
  return parseLevelFromRoles(rows.map((r) => r.role_id));
}

export async function getEffectiveLevels(
  userIds: number[],
): Promise<Map<number, Level>> {
  if (userIds.length === 0) return new Map();
  const { rows } = await query<{ user_id: number; role_ids: string[] | null }>(
    `SELECT ur.user_id,
            array_agg(ur.role_id) FILTER (WHERE ur.role_id IS NOT NULL) AS role_ids
       FROM perm.user_roles ur
      WHERE ur.user_id = ANY($1::int[])
      GROUP BY ur.user_id`,
    [userIds],
  );
  const m = new Map<number, Level>();
  for (const r of rows) {
    m.set(r.user_id, parseLevelFromRoles(r.role_ids ?? []));
  }
  return m;
}

export async function getRoleEffectiveLevel(roleId: string): Promise<Level> {
  return parseLevelFromRoles([roleId]);
}
