// Actor resolution. Reads the signed session and returns the full user row
// (joined with persona role + dept permission). Used everywhere a session actor is needed.

import { query } from '../db';
import { SESSION_COOKIE, verifySession, type SessionPayload } from './sessionToken';
import { validateActiveSession } from './sessionStore';
import { parseRoleId } from '../perm/grammar';

export interface ActorUser {
  id: number;
  employee_code: string;
  fullname: string;
  department: string | null;
  dept_id: string | null;
  dept_group_name: string | null;
  role_id: string;
  role_name: string;
  permissions: string[];
  level: number;
}

export async function loadActor(): Promise<ActorUser | null> {
  const sess = await getSessionFromAnySource();
  if (!sess) return null;

  const res = await query<{
    id: number;
    employee_code: string;
    fullname: string;
    role_id: string | null;
    rank: number | null;
    department_id: string | null;
    permissions: string[];
  }>(
    `SELECT u.id, u.employee_code, u.fullname,
       (SELECT ur.role_id FROM perm.user_roles ur
         WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
         LIMIT 1) AS role_id,
       (SELECT r.rank FROM perm.user_roles ur
          JOIN perm.roles r ON r.id = ur.role_id AND r.kind = ur.role_kind
         WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
         LIMIT 1) AS rank,
       (SELECT ud.department_id FROM perm.user_departments ud
         WHERE ud.user_id = u.id) AS department_id,
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
    [sess.sub],
  );
  const row = res.rows[0];
  if (!row) return null;
  const parsed = parseRoleId(row.role_id ?? '');
  const deptId = row.department_id;
  return {
    id: row.id,
    employee_code: row.employee_code,
    fullname: row.fullname,
    department: deptId,
    dept_id: deptId,
    dept_group_name: deptId,
    role_id: row.role_id ?? 'unconfigured',
    role_name: parsed?.name ?? 'unconfigured',
    permissions: row.permissions ?? [],
    level: row.rank ?? parsed?.level ?? 99,
  };
}

export async function getSessionFromAnySource(reqHeaders?: Record<string, string | string[] | undefined>): Promise<SessionPayload | null> {
  const fromHeaders = reqHeaders ? sessionFromHeaders(reqHeaders) : null;
  if (fromHeaders) {
    const payload = await verifySession(fromHeaders);
    return payload ? validateActiveSession(payload) : null;
  }
  try {
    const { cookies } = await import('next/headers');
    const c = await cookies();
    const token = c.get(SESSION_COOKIE)?.value ?? null;
    const payload = await verifySession(token);
    return payload ? validateActiveSession(payload) : null;
  } catch {
    return null;
  }
}

export function sessionFromHeaders(headers: Record<string, string | string[] | undefined>): string | null {
  const raw = headers['x-folio-session'];
  if (typeof raw === 'string' && raw) return raw;
  const cookie = headers['cookie'];
  if (typeof cookie !== 'string') return null;
  const match = cookie.match(/(?:^|;\s*)folio_session=([^;]+)/);
  return match ? decodeURIComponent(match[1]) : null;
}
