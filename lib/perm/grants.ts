// lib/perm/grants.ts — CRUD for perm.user_permissions.
//
// Single persistence path: perm.user_permissions (supports both permanent
// via ends_at=NULL and time-bound via ends_at set). The effect is encoded
// in each permission_id via the '::allow'/'::deny' suffix.

import 'server-only';
import { query } from '../db';
import { effectOf } from './grammar';

export type GrantSource = 'manual' | 'seed' | 'bulk' | 'access_request';

export interface UserPerm {
  id: number;
  user_id: number;
  permission_id: string;
  granted_by: string;
  reason: string | null;
  granted_at: string;
  revoked_at: string | null;
  revoked_by: string | null;
  starts_at: string;
  ends_at: string | null;
}

export interface CreatePermInput {
  user_id: number;
  permission_id: string;
  starts_at?: string;
  ends_at?: string | null;
  granted_by: string;
  reason?: string;
  source?: GrantSource;
}

export async function createGrant(input: CreatePermInput): Promise<UserPerm> {
  const { rows } = await query<UserPerm>(
    `INSERT INTO perm.user_permissions
       (user_id, permission_id, starts_at, ends_at, granted_by, reason)
     VALUES ($1, $2, COALESCE($3, now()), $4, $5, $6)
     RETURNING id, user_id, permission_id, granted_by, reason, granted_at,
               revoked_at, revoked_by, starts_at, ends_at`,
    [
      input.user_id,
      input.permission_id,
      input.starts_at ?? null,
      input.ends_at ?? null,
      input.granted_by,
      input.reason ?? null,
    ],
  );
  return rows[0];
}

export async function revokeGrant(grantId: number, revokedBy: string): Promise<void> {
  await query(
    `UPDATE perm.user_permissions
        SET revoked_at = now(), revoked_by = $2
      WHERE id = $1 AND revoked_at IS NULL`,
    [grantId, revokedBy],
  );
}

export async function listUserGrants(
  userId: number,
  options: { activeOnly?: boolean } = {},
): Promise<UserPerm[]> {
  const where = options.activeOnly
    ? 'WHERE user_id = $1 AND revoked_at IS NULL AND (ends_at IS NULL OR now() BETWEEN starts_at AND ends_at)'
    : 'WHERE user_id = $1 AND revoked_at IS NULL';
  const { rows } = await query<UserPerm>(
    `SELECT id, user_id, permission_id, granted_by, reason, granted_at,
            revoked_at, revoked_by, starts_at, ends_at
       FROM perm.user_permissions ${where}
      ORDER BY COALESCE(ends_at, 'infinity') DESC, granted_at DESC`,
    [userId],
  );
  return rows;
}

export async function listActiveGrantsForPerm(permissionId: string): Promise<UserPerm[]> {
  const { rows } = await query<UserPerm>(
    `SELECT id, user_id, permission_id, granted_by, reason, granted_at,
            revoked_at, revoked_by, starts_at, ends_at
       FROM perm.user_permissions
      WHERE permission_id = $1
        AND revoked_at IS NULL
        AND (ends_at IS NULL OR now() BETWEEN starts_at AND ends_at)
      ORDER BY ends_at ASC NULLS LAST`,
    [permissionId],
  );
  return rows;
}

export async function expireOverdueGrants(): Promise<number> {
  const { rowCount } = await query(
    `UPDATE perm.user_permissions
        SET revoked_at = now(), revoked_by = 'system.expiry'
      WHERE ends_at IS NOT NULL AND ends_at < now() AND revoked_at IS NULL`,
  );
  return rowCount ?? 0;
}

export interface ActingBundleInput {
  user_id: number;
  role_id: string;
  ends_at?: string | null;
  granted_by: string;
  reason?: string;
}

export async function grantActingBundle(input: ActingBundleInput): Promise<string[]> {
  const { rows: perms } = await query<{ permission_id: string }>(
    `SELECT permission_id FROM perm.role_permissions WHERE role_id = $1`,
    [input.role_id],
  );
  const allow = perms.map((p) => p.permission_id).filter((p) => effectOf(p) !== 'deny');
  if (allow.length === 0) return [];
  for (const p of allow) {
    await createGrant({
      user_id: input.user_id,
      permission_id: p,
      ends_at: input.ends_at,
      granted_by: input.granted_by,
      reason: input.reason ?? `Acting as ${input.role_id}`,
    });
  }
  return allow;
}

export async function listActiveUserPerms(userId: number): Promise<UserPerm[]> {
  const { rows } = await query<UserPerm>(
    `SELECT id, user_id, permission_id, granted_by, reason, granted_at,
            revoked_at, revoked_by, starts_at, ends_at
       FROM perm.user_permissions
      WHERE user_id = $1 AND revoked_at IS NULL
        AND (ends_at IS NULL OR ends_at > now())
      ORDER BY granted_at DESC`,
    [userId],
  );
  return rows;
}

export async function revokeUserPerm(rowId: number, revokedBy: string): Promise<void> {
  await query(
    `UPDATE perm.user_permissions
        SET revoked_at = now(), revoked_by = $2
      WHERE id = $1 AND revoked_at IS NULL`,
    [rowId, revokedBy],
  );
}

export interface UpsertUserPermsInput {
  user_id: number;
  desired_perm_ids: string[];
  granted_by: string;
  reason?: string;
  ends_at?: Date | string | null;
}

export async function setUserPermanentPerms(input: UpsertUserPermsInput): Promise<{
  added: string[];
  removed: number;
}> {
  const cur = await listActiveUserPerms(input.user_id);
  const before = new Set(cur.map((r) => r.permission_id));
  const after = new Set([
    ...input.desired_perm_ids,
    ...cur.filter((r) => r.permission_id.startsWith('user:dept:')).map((r) => r.permission_id),
  ]);
  const added = [...after].filter((p) => !before.has(p));
  const removed = [...before].filter((p) => !after.has(p));
  if (removed.length > 0) {
    await query(
      `UPDATE perm.user_permissions
          SET revoked_at = now(), revoked_by = $2
        WHERE user_id = $1 AND permission_id = ANY($3::text[]) AND revoked_at IS NULL`,
      [input.user_id, input.granted_by, removed],
    );
  }
  for (const pid of added) {
    await query(
      `INSERT INTO perm.user_permissions (user_id, permission_id, granted_by, reason)
       VALUES ($1, $2, $3, $4)`,
      [input.user_id, pid, input.granted_by, input.reason ?? null],
    );
  }
  return { added, removed: removed.length };
}
