// lib/perm/level.ts — derive a user's effective authority level from role rank.

import 'server-only';
import { query } from '../db';
import { parseLevelFromRoles } from './grammar';

export type Level = number;

export async function getEffectiveLevel(userId: number): Promise<Level> {
  const { rows } = await query<{ rank: number }>(
    `SELECT r.rank FROM perm.user_roles ur
       JOIN perm.roles r ON r.id = ur.role_id AND r.kind = ur.role_kind
      WHERE ur.user_id = $1 AND ur.role_kind = 'hierarchy'`,
    [userId],
  );
  return rows[0]?.rank ?? 99;
}

export async function getEffectiveLevels(
  userIds: number[],
): Promise<Map<number, Level>> {
  if (userIds.length === 0) return new Map();
  const { rows } = await query<{ user_id: number; level: number }>(
    `SELECT ur.user_id, min(r.rank)::int AS level
       FROM perm.user_roles ur
       JOIN perm.roles r ON r.id = ur.role_id AND r.kind = ur.role_kind
      WHERE ur.user_id = ANY($1::int[])
        AND ur.role_kind = 'hierarchy'
      GROUP BY ur.user_id`,
    [userIds],
  );
  const m = new Map<number, Level>();
  for (const r of rows) {
    m.set(r.user_id, r.level);
  }
  return m;
}

export async function getRoleEffectiveLevel(roleId: string): Promise<Level> {
  const r = await query<{ rank: number | null }>(
    `SELECT rank FROM perm.roles WHERE id = $1 AND kind = 'hierarchy'`,
    [roleId],
  );
  return r.rows[0]?.rank ?? parseLevelFromRoles([roleId]);
}
