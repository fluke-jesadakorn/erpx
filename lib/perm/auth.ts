// perm/auth.ts — server-side session loader + DB-backed auth helpers.
// Pure permission check helpers live in auth-client.ts (client-safe).
//
//   hasPermission(session, 'finance:expense:approve::allow')
//   canManageResource(session, 'finance:expense:update::allow', { ownerId, deptId })

import 'server-only';
import { query } from '../db';
import {
  SESSION_COOKIE, verifySession, sessionFromHeaders, type SessionPayload,
} from '../server/sessionToken';
import type { PermSession } from './session';

export { hasPermission, canManageResource, sessionDept, sessionLevel, sessionRoleName, ADMIN_PERM, levelFromRoles } from './auth-client';

export interface DecodedPermToken extends PermSession {
  iat: number;
  exp: number;
}

export interface ActivePermSession {
  session: PermSession;
  decoded: DecodedPermToken;
}

export async function loadPermSessionFromHeaders(
  headers: Record<string, string | string[] | undefined> | Headers,
): Promise<ActivePermSession | null> {
  const tok = sessionFromHeaders(headers);
  const payload = await verifySession(tok);
  if (!payload) return null;
  return await hydratePermSession(payload);
}

export async function loadPermSessionFromCookieValue(
  value: string | null | undefined,
): Promise<ActivePermSession | null> {
  const payload = await verifySession(value);
  if (!payload) return null;
  return await hydratePermSession(payload);
}

async function hydratePermSession(payload: SessionPayload): Promise<ActivePermSession | null> {
  const profile = await query<{
    fullname: string;
    role_id: string | null;
    permissions: string[];
  }>(
    `SELECT u.fullname,
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
      FROM users u
     WHERE u.id = $1`,
    [payload.sub],
  );
  if (profile.rows.length === 0) return null;
  const row = profile.rows[0];
  const session: PermSession = {
    user: {
      id: payload.sub,
      name: row.fullname,
      role: row.role_id ?? 'officer::5',
    },
    permissions: row.permissions ?? [],
  };
  const decoded: DecodedPermToken = { ...session, iat: payload.iat, exp: payload.exp };
  return { session, decoded };
}

export interface LoadPermSessionResult extends ActivePermSession {
  headers: Record<string, string | string[] | undefined>;
}

export async function loadActivePermSession(
  req: Request,
): Promise<ActivePermSession | null> {
  const headers = req.headers as unknown as Record<string, string | string[] | undefined>;
  return await loadPermSessionFromHeaders(headers);
}

export { SESSION_COOKIE };