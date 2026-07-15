// Actor resolution. Reads the signed session and returns the full user row
// (joined with persona role + dept permission). Used everywhere a session actor is needed.

import { query } from '../db';
import { SESSION_COOKIE, verifySession, type SessionPayload } from './sessionToken';
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
    dept_perm: string | null;
    permissions: string[];
  }>(
    `SELECT u.id, u.employee_code, u.fullname,
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
       (SELECT up.permission_id FROM perm.user_permissions up
          WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
            AND up.revoked_at IS NULL
            AND (up.ends_at IS NULL OR up.ends_at > now())
          ORDER BY up.permission_id LIMIT 1) AS dept_perm,
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
    [sess.sub],
  );
  const row = res.rows[0];
  if (!row) return null;
  const parsed = parseRoleId(row.role_id ?? 'officer::5');
  const deptId = row.dept_perm
    ? row.dept_perm.replace(/^user:dept:/, '').replace(/::allow$/, '')
    : null;
  return {
    id: row.id,
    employee_code: row.employee_code,
    fullname: row.fullname,
    department: deptId,
    dept_id: deptId,
    dept_group_name: deptId,
    role_id: row.role_id ?? 'officer::5',
    role_name: parsed?.name ?? 'officer',
    permissions: row.permissions ?? [],
    level: parsed?.level ?? 5,
  };
}

export async function getSessionFromAnySource(reqHeaders?: Record<string, string | string[] | undefined>): Promise<SessionPayload | null> {
  const fromHeaders = reqHeaders ? sessionFromHeaders(reqHeaders) : null;
  if (fromHeaders) return verifySession(fromHeaders);
  try {
    const { cookies } = await import('next/headers');
    const c = await cookies();
    const token = c.get(SESSION_COOKIE)?.value ?? null;
    return verifySession(token);
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