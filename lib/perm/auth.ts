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
import { validateActiveSession } from '../server/sessionStore';
import type { PermSession } from './session';
import { loadDeptPermissionBundles, expandUserPermissions } from './deptGrant';

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
  const active = await validateActiveSession(payload);
  if (!active) return null;
  const profile = await query<{
    fullname: string;
    role_id: string | null;
    rank: number | null;
    department_id: string | null;
    system_roles: string[];
    permissions: string[];
  }>(
    `SELECT u.fullname,
       COALESCE((
         SELECT ur.role_id FROM perm.user_roles ur
          WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
          ORDER BY ur.granted_at ASC
          LIMIT 1
       ), NULL) AS role_id,
       (SELECT r.rank FROM perm.user_roles ur
          JOIN perm.roles r ON r.id = ur.role_id
         WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
         LIMIT 1) AS rank,
       (SELECT ud.department_id FROM perm.user_departments ud WHERE ud.user_id = u.id) AS department_id,
       COALESCE((SELECT array_agg(ur.role_id ORDER BY ur.role_id)
                   FROM perm.user_roles ur
                  WHERE ur.user_id = u.id AND ur.role_kind = 'system'), ARRAY[]::text[]) AS system_roles,
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
             UNION
             SELECT 'user:dept:' || ud.department_id || '::allow' AS p_id
               FROM perm.user_departments ud
              WHERE ud.user_id = u.id
           ) t
       ), ARRAY[]::text[]) AS permissions
      FROM users u
     WHERE u.id = $1 AND u.is_active IS TRUE`,
    [payload.sub],
  );
  if (profile.rows.length === 0) return null;
  const row = profile.rows[0];
  const bundles = await loadDeptPermissionBundles();
  const base = row.permissions ?? [];
  const expanded = expandUserPermissions(base, bundles);
  const session: PermSession = {
    user: {
      id: payload.sub,
      name: row.fullname,
      role: row.role_id ?? 'unconfigured',
      department: row.department_id,
      rank: row.rank,
      systemRoles: row.system_roles ?? [],
    },
    permissions: Array.from(expanded),
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
